use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
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

use super::{AppState, BODY_LIMIT, INFO_ROUTE, INVENTORY_ROUTE};
use crate::session::SessionError;

pub(super) const CLIENT_HEADER: HeaderName = HeaderName::from_static(protocol::CLIENT_HEADER);
pub(super) const TOKEN_HEADER: HeaderName = HeaderName::from_static(protocol::TOKEN_HEADER);
const PLACE_ID_HEADER: HeaderName = HeaderName::from_static(protocol::PLACE_ID_HEADER);

const TOKEN_REQUIRED: &str = "token required";
const TOKEN_REJECTED: &str = "token rejected";

pub(super) const POST_BODY_SLOTS: usize = 2;

const BODY_READ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug)]
pub(super) struct Authenticated;

#[derive(Debug)]
pub(super) struct PostGate {
    slots: Arc<Semaphore>,
    wait: Duration,
    tasks: TaskTracker,
    inventory_body: usize,
}

impl PostGate {
    pub(super) fn new(wait: Duration, tasks: TaskTracker, inventory_body: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(POST_BODY_SLOTS)),
            wait,
            tasks,
            inventory_body,
        }
    }

    fn body_limit(&self, path: &str) -> usize {
        if path == INVENTORY_ROUTE {
            self.inventory_body
        } else {
            BODY_LIMIT
        }
    }
}

#[derive(Debug)]
enum Refusal {
    Forbidden(String),
    Unauthorized(&'static str),
}

impl Refusal {
    fn forbidden(reason: &str) -> Self {
        Self::Forbidden(reason.to_owned())
    }

    fn reason(&self) -> &str {
        match self {
            Self::Forbidden(reason) => reason,
            Self::Unauthorized(reason) => reason,
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden(reason) => (StatusCode::FORBIDDEN, reason).into_response(),
            Self::Unauthorized(reason) => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": reason })),
            )
                .into_response(),
        }
    }
}

pub(super) struct Gate {
    hosts: [String; 3],
    serve_place_id: Option<u64>,
    state: Arc<AppState>,
    unpublished_logged: AtomicBool,
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
            unpublished_logged: AtomicBool::new(false),
        }
    }

    async fn admit(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<bool, Refusal> {
        if headers.contains_key(header::ORIGIN) {
            return Err(Refusal::forbidden("cross-origin requests are not allowed"));
        }
        if !self.host_is_allowed(headers) {
            return Err(Refusal::forbidden("host is not allowed"));
        }
        if headers.get(CLIENT_HEADER).is_none_or(HeaderValue::is_empty) {
            return Err(Refusal::Forbidden(format!(
                "{}: missing {} header",
                protocol::OUTDATED_PLUGIN_TOKEN,
                protocol::CLIENT_HEADER
            )));
        }
        let authenticated = match self.check_token(headers) {
            Ok(()) => true,
            Err(_) if path == INFO_ROUTE => false,
            Err(refusal) => return Err(refusal),
        };
        match self.serve_place_id {
            Some(expected) if !place_id_matches(headers, expected) => {
                Err(Refusal::forbidden("place id mismatch"))
            }
            Some(_) => Ok(authenticated),
            None if !authenticated => self
                .refuse_other_bound_place(headers)
                .map(|()| authenticated),
            None => {
                if method == Method::POST
                    && !headers.contains_key(PLACE_ID_HEADER)
                    && let Some(bound) = self.state.bound_place.get()
                {
                    return Err(Refusal::Forbidden(unplaced_write_refused(*bound)));
                }
                self.bind_place(headers)
                    .await
                    .map_err(Refusal::Forbidden)
                    .map(|()| authenticated)
            }
        }
    }

    fn check_token(&self, headers: &HeaderMap) -> Result<(), Refusal> {
        match headers.get(TOKEN_HEADER) {
            None => Err(Refusal::Unauthorized(TOKEN_REQUIRED)),
            Some(presented) if self.state.token().matches(presented.as_bytes()) => Ok(()),
            Some(_) => Err(Refusal::Unauthorized(TOKEN_REJECTED)),
        }
    }

    fn refuse_other_bound_place(&self, headers: &HeaderMap) -> Result<(), Refusal> {
        let Some(bound) = self.state.bound_place.get() else {
            return Ok(());
        };
        let Some(place_id) = requested_place_id(headers).map_err(Refusal::Forbidden)? else {
            return Ok(());
        };
        if place_id == protocol::UNPUBLISHED_PLACE_ID {
            Err(Refusal::Forbidden(unpublished_place_refused(*bound)))
        } else if place_id == *bound {
            Ok(())
        } else {
            Err(Refusal::Forbidden(place_bound_elsewhere(*bound, place_id)))
        }
    }

    async fn bind_place(&self, headers: &HeaderMap) -> Result<(), String> {
        let Some(place_id) = requested_place_id(headers)? else {
            return Ok(());
        };
        if place_id == protocol::UNPUBLISHED_PLACE_ID {
            return self.admit_unpublished();
        }
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

    fn admit_unpublished(&self) -> Result<(), String> {
        if let Some(bound) = self.state.bound_place.get() {
            return Err(unpublished_place_refused(*bound));
        }
        if !self.unpublished_logged.swap(true, Ordering::Relaxed) {
            tracing::info!(
                target: "naht::server",
                "a request names an unpublished place, so the project stays unbound"
            );
        }
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

fn unpublished_place_refused(bound: u64) -> String {
    format!(
        "the request names place {}, an unpublished place, but this project is bound to place {bound}: publish the place in Studio, or open place {bound}",
        protocol::UNPUBLISHED_PLACE_ID
    )
}

fn unplaced_write_refused(bound: u64) -> String {
    format!(
        "missing {} header: this project is bound to place {bound}, a write must name it",
        protocol::PLACE_ID_HEADER
    )
}

pub(super) async fn guard(
    State(gate): State<Arc<Gate>>,
    mut request: Request,
    next: Next,
) -> Response {
    let decision = gate
        .admit(request.method(), request.uri().path(), request.headers())
        .await;
    match decision {
        Ok(authenticated) => {
            if authenticated {
                request.extensions_mut().insert(Authenticated);
            }
            next.run(request).await
        }
        Err(refusal) => {
            tracing::warn!(target: "naht::server", reason = refusal.reason(), "request refused");
            refusal.into_response()
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
    let limit = gate.body_limit(request.uri().path());
    let (parts, body) = request.into_parts();
    match time::timeout(BODY_READ_TIMEOUT, to_bytes(body, limit)).await {
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
