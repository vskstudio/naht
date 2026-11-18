use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::LengthLimitError;
use naht_core::protocol;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time;
use tokio_util::task::TaskTracker;

use super::{AppState, BODY_LIMIT};
use crate::session::SessionError;

pub(super) const CLIENT_HEADER: HeaderName = HeaderName::from_static(protocol::CLIENT_HEADER);
const PLACE_ID_HEADER: HeaderName = HeaderName::from_static(protocol::PLACE_ID_HEADER);

pub(super) const POST_BODY_SLOTS: usize = 2;

const BODY_READ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub(super) struct PostGate {
    slots: Arc<Semaphore>,
    wait: Duration,
    tasks: TaskTracker,
}

impl PostGate {
    pub(super) fn new(wait: Duration, tasks: TaskTracker) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(POST_BODY_SLOTS)),
            wait,
            tasks,
        }
    }
}

pub(super) struct Gate {
    hosts: [String; 3],
    serve_place_id: Option<u64>,
    state: Arc<AppState>,
}

impl Gate {
    pub(super) fn new(port: u16, state: Arc<AppState>) -> Self {
        Self {
            hosts: [
                format!("127.0.0.1:{port}"),
                format!("localhost:{port}"),
                format!("[::1]:{port}"),
            ],
            serve_place_id: state.serve_place_id,
            state,
        }
    }

    async fn admit(&self, method: &Method, headers: &HeaderMap) -> Result<(), String> {
        if headers.contains_key(header::ORIGIN) {
            return Err("cross-origin requests are not allowed".to_owned());
        }
        if !self.host_is_allowed(headers) {
            return Err("host is not allowed".to_owned());
        }
        if headers.get(CLIENT_HEADER).is_none_or(HeaderValue::is_empty) {
            return Err(format!(
                "{}: missing {} header",
                protocol::OUTDATED_PLUGIN_TOKEN,
                protocol::CLIENT_HEADER
            ));
        }
        match self.serve_place_id {
            Some(expected) if !place_id_matches(headers, expected) => {
                Err("place id mismatch".to_owned())
            }
            Some(_) => Ok(()),
            None => {
                if method == Method::POST
                    && !headers.contains_key(PLACE_ID_HEADER)
                    && let Some(bound) = self.state.bound_place.get()
                {
                    return Err(unplaced_write_refused(*bound));
                }
                self.bind_place(headers).await
            }
        }
    }

    async fn bind_place(&self, headers: &HeaderMap) -> Result<(), String> {
        let Some(place_id) = requested_place_id(headers)? else {
            return Ok(());
        };
        if let Some(bound) = self.state.bound_place.get() {
            return if *bound == place_id {
                Ok(())
            } else {
                Err(place_bound_elsewhere(*bound, place_id))
            };
        }
        self.state
            .session_lock()
            .await
            .bind_place(place_id)
            .map_err(|error| error.to_string())?;
        self.state.bound_place.get_or_init(|| place_id);
        Ok(())
    }

    fn host_is_allowed(&self, headers: &HeaderMap) -> bool {
        let mut hosts = headers.get_all(header::HOST).iter();
        match (hosts.next(), hosts.next()) {
            (Some(host), None) => host
                .to_str()
                .is_ok_and(|host| self.hosts.iter().any(|allowed| allowed == host)),
            _ => false,
        }
    }
}

fn place_id_matches(headers: &HeaderMap, expected: u64) -> bool {
    requested_place_id(headers).is_ok_and(|requested| requested == Some(expected))
}

fn requested_place_id(headers: &HeaderMap) -> Result<Option<u64>, String> {
    headers
        .get(PLACE_ID_HEADER)
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|text| text.parse::<u64>().ok())
                .ok_or_else(|| "malformed place id header".to_owned())
        })
        .transpose()
}

fn place_bound_elsewhere(bound: u64, requested: u64) -> String {
    SessionError::PlaceBoundElsewhere { bound, requested }.to_string()
}

fn unplaced_write_refused(bound: u64) -> String {
    format!(
        "missing {} header: this project is bound to place {bound}, a write must name it",
        protocol::PLACE_ID_HEADER
    )
}

pub(super) async fn guard(State(gate): State<Arc<Gate>>, request: Request, next: Next) -> Response {
    match gate.admit(request.method(), request.headers()).await {
        Ok(()) => next.run(request).await,
        Err(reason) => {
            tracing::warn!(target: "naht::server", reason = reason.as_str(), "request refused");
            (StatusCode::FORBIDDEN, reason).into_response()
        }
    }
}

pub(super) async fn limit_post_bodies(
    State(gate): State<Arc<PostGate>>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }
    let Ok(Ok(permit)) = time::timeout(gate.wait, Arc::clone(&gate.slots).acquire_owned()).await
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (parts, body) = request.into_parts();
    match time::timeout(BODY_READ_TIMEOUT, to_bytes(body, BODY_LIMIT)).await {
        Ok(Ok(bytes)) => {
            let buffered = Request::from_parts(parts, Body::from(bytes));
            run_holding(permit, &gate.tasks, next, buffered).await
        }
        Ok(Err(error)) => body_error_status(&error).into_response(),
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

async fn run_holding(
    permit: OwnedSemaphorePermit,
    tasks: &TaskTracker,
    next: Next,
    request: Request,
) -> Response {
    let handler = tasks.spawn(async move {
        let _permit = permit;
        next.run(request).await
    });
    handler
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn body_error_status(error: &axum::Error) -> StatusCode {
    let mut source: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(current) = source {
        if current.is::<LengthLimitError>() {
            return StatusCode::PAYLOAD_TOO_LARGE;
        }
        source = current.source();
    }
    StatusCode::BAD_REQUEST
}

#[cfg(test)]
mod place_binding;
#[cfg(test)]
mod tests;
