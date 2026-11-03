//! The localhost HTTP server the Studio plugin talks to (architecture §7).
//!
//! All endpoints speak MessagePack: `GET /info` (handshake + `servePlaceId` guard + reconnect
//! re-diff), `GET /patches` (text long-poll held open until a change or timeout), `POST /changes`
//! (Studio → filesystem), `GET /blobs` + `POST /blobs` (the separate terrain blob channel),
//! `POST /ack` (the plugin confirms applied patches), and `GET /heartbeat` (liveness). All sync
//! decisions are delegated to the [`Session`]; this layer is pure transport.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::error_handling::HandleErrorLayer;
use axum::extract::DefaultBodyLimit;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use naht_core::protocol::{
    self, Ack, BlobChangeBatch, BlobPatchBatch, ChangeBatch, PatchBatch, Pong, ProtocolError,
    ServerInfo,
};
use naht_core::vfs::VfsError;
use serde::Deserialize;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinError;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower::timeout::error::Elapsed;
use tower::{BoxError, ServiceBuilder};
use tower_http::limit::RequestBodyLimitLayer;

use crate::session::{Session, SessionError};
use std::io;
use tokio::net::TcpListener;
use tokio::sync::MutexGuard;
use tokio::task;
use tokio::time;

const BODY_LIMIT: usize = 64 * 1024 * 1024;

const MAX_IN_FLIGHT: usize = 64;

const TIMEOUT_SLACK: Duration = Duration::from_secs(10);

/// Shared server state: the session behind a mutex, plus notifiers that wake parked long-polls
/// whenever a queue may have grown. Text patches and binary blobs ride separate channels, so each
/// has its own notifier and parks independently.
#[derive(Debug)]
pub struct AppState {
    session: Mutex<Session>,
    patches_ready: Notify,
    blobs_ready: Notify,
    long_poll: Duration,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

impl AppState {
    fn wake_all_long_polls(&self) {
        self.patches_ready.notify_waiters();
        self.blobs_ready.notify_waiters();
    }

    /// Wrap a session, parking long-polls for at most `long_poll` before they return empty.
    pub fn new(session: Session, long_poll: Duration) -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(session),
            patches_ready: Notify::new(),
            blobs_ready: Notify::new(),
            long_poll,
            shutdown: CancellationToken::new(),
            tasks: TaskTracker::new(),
        })
    }

    /// The token that stops the server and every tracked task once cancelled.
    pub fn shutdown(&self) -> &CancellationToken {
        &self.shutdown
    }

    /// The tracker every background task of the daemon is spawned on, so shutdown can await them.
    pub fn tasks(&self) -> &TaskTracker {
        &self.tasks
    }

    /// Run `work` on a blocking thread with the session locked, so disk and database access never
    /// stalls the runtime or holds the lock across an await.
    pub async fn with_session<T, F>(self: &Arc<Self>, work: F) -> Result<T, JoinError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Session) -> T + Send + 'static,
    {
        let state = Arc::clone(self);
        task::spawn_blocking(move || work(&mut state.session.blocking_lock())).await
    }

    /// Lock the session for exclusive access. Used by the watcher to reconcile on a filesystem event.
    pub async fn session_lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().await
    }

    /// Wake any parked long-polls to re-check the patch queue.
    pub fn notify_patches(&self) {
        self.patches_ready.notify_waiters();
    }

    /// Wake any parked blob long-polls to re-check the blob queue.
    pub fn notify_blobs(&self) {
        self.blobs_ready.notify_waiters();
    }
}

/// Build the router over `state`.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/info", get(info))
        .route("/patches", get(patches))
        .route("/changes", post(changes))
        .route("/blobs", get(blobs).post(post_blobs))
        .route("/ack", post(ack))
        .route("/heartbeat", get(heartbeat))
        .route("/activity", get(activity))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::<_, ()>::new(overload))
                .layer(TimeoutLayer::new(
                    state.long_poll.saturating_add(TIMEOUT_SLACK),
                ))
                .layer(ConcurrencyLimitLayer::new(MAX_IN_FLIGHT))
                .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
                .layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .with_state(state)
}

async fn overload(error: BoxError) -> StatusCode {
    if error.is::<Elapsed>() {
        StatusCode::REQUEST_TIMEOUT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// Serve `state` on an already-bound `listener` until the shutdown token is cancelled, then let
/// in-flight requests finish.
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> io::Result<()> {
    let shutdown = state.shutdown.clone();
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
}

#[derive(Deserialize)]
struct InfoQuery {
    place_id: Option<u64>,
}

async fn info(State(state): State<Arc<AppState>>, Query(query): Query<InfoQuery>) -> Response {
    let outcome = state
        .with_session(move |session| handshake(session, query.place_id))
        .await;
    let info = match outcome {
        Ok(Ok(info)) => info,
        Ok(Err(rejection)) => return rejection.into_response(),
        Err(error) => return Rejection::Blocking(error).into_response(),
    };
    tracing::info!(target: "naht::server", "handshake ok; re-diffed on connect");
    state.patches_ready.notify_waiters();
    state.blobs_ready.notify_waiters();
    msgpack(&info)
}

fn handshake(session: &mut Session, place_id: Option<u64>) -> Result<ServerInfo, Rejection> {
    if let Some(expected) = session.serve_place_id()
        && place_id != Some(expected)
    {
        tracing::warn!(
            target: "naht::server",
            expected,
            reported = ?place_id,
            "handshake rejected: place id mismatch"
        );
        return Err(Rejection::PlaceMismatch {
            expected,
            reported: place_id,
        });
    }
    session.rescan()?;
    Ok(session.info())
}

#[derive(Deserialize)]
struct CursorQuery {
    cursor: Option<u64>,
}

async fn patches(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    let notified = enrol_waiter(&state.patches_ready);
    {
        let session = state.session.lock().await;
        if session.has_patches_after(cursor) {
            return patch_batch(&session, cursor);
        }
    }
    let _ = time::timeout(state.long_poll, notified).await;

    let session = state.session.lock().await;
    patch_batch(&session, cursor)
}

async fn changes(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: ChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let outcome = state
        .with_session(move |session| apply_text_batch(session, batch))
        .await;
    let count = match outcome {
        Ok(Ok(count)) => count,
        Ok(Err(rejection)) => return rejection.into_response(),
        Err(error) => return Rejection::Blocking(error).into_response(),
    };
    tracing::info!(target: "naht::server", count, "changes applied from Studio");
    state.wake_all_long_polls();
    StatusCode::OK.into_response()
}

fn apply_text_batch(session: &mut Session, batch: ChangeBatch) -> Result<usize, Rejection> {
    for change in &batch.changes {
        let path = match change {
            protocol::Change::Upsert { path, .. } | protocol::Change::Delete { path } => path,
        };
        session.validate_path(path)?;
    }
    let count = batch.changes.len();
    session.apply_changes(batch.changes)?;
    Ok(count)
}

async fn blobs(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    let notified = enrol_waiter(&state.blobs_ready);
    {
        let session = state.session.lock().await;
        if session.has_blobs_after(cursor) {
            return blob_batch(&session, cursor);
        }
    }
    let _ = time::timeout(state.long_poll, notified).await;

    let session = state.session.lock().await;
    blob_batch(&session, cursor)
}

async fn post_blobs(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: BlobChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let outcome = state
        .with_session(move |session| apply_blob_batch(session, batch))
        .await;
    let count = match outcome {
        Ok(Ok(count)) => count,
        Ok(Err(rejection)) => return rejection.into_response(),
        Err(error) => return Rejection::Blocking(error).into_response(),
    };
    tracing::info!(target: "naht::server", count, "blob changes applied from Studio");
    state.blobs_ready.notify_waiters();
    StatusCode::OK.into_response()
}

fn apply_blob_batch(session: &mut Session, batch: BlobChangeBatch) -> Result<usize, Rejection> {
    for change in &batch.changes {
        session.validate_path(&change.path)?;
    }
    let count = batch.changes.len();
    session.apply_blob_changes(batch.changes)?;
    Ok(count)
}

async fn ack(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let ack: Ack = match protocol::from_msgpack(&body) {
        Ok(ack) => ack,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let count = ack.paths.len();
    let outcome = state
        .with_session(move |session| session.ack(&ack.paths))
        .await;
    match outcome {
        Ok(Ok(())) => {
            tracing::debug!(target: "naht::server", count, "patches acked");
            StatusCode::OK.into_response()
        }
        Ok(Err(error)) => Rejection::from(error).into_response(),
        Err(error) => Rejection::Blocking(error).into_response(),
    }
}

async fn heartbeat(State(state): State<Arc<AppState>>) -> Response {
    let session_id = state.session.lock().await.info().session_id;
    msgpack(&Pong { session_id })
}

#[derive(Deserialize)]
struct ActivityQuery {
    since: Option<u64>,
}

async fn activity(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityQuery>,
) -> Response {
    let since = query.since.unwrap_or(0);
    let session = state.session.lock().await;
    msgpack(&session.activity(since))
}

fn patch_batch(session: &Session, cursor: u64) -> Response {
    let (cursor, patches) = session.take_patches(cursor);
    msgpack(&PatchBatch { cursor, patches })
}

fn blob_batch(session: &Session, cursor: u64) -> Response {
    let (cursor, patches) = session.take_blob_patches(cursor);
    msgpack(&BlobPatchBatch { cursor, patches })
}

fn msgpack<T: serde::Serialize>(value: &T) -> Response {
    match protocol::to_msgpack(value) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/msgpack")], bytes).into_response(),
        Err(error) => Rejection::Encode(error).into_response(),
    }
}

fn enrol_waiter(notify: &Notify) -> Pin<Box<impl Future<Output = ()> + '_>> {
    let mut notified = Box::pin(notify.notified());
    notified.as_mut().enable();
    notified
}

#[derive(Debug, thiserror::Error)]
enum Rejection {
    #[error("malformed message: {0}")]
    Malformed(#[source] ProtocolError),
    #[error(transparent)]
    InvalidPath(#[from] VfsError),
    #[error("place id mismatch: project serves {expected}, plugin reported {reported:?}")]
    PlaceMismatch {
        expected: u64,
        reported: Option<u64>,
    },
    #[error("cannot encode the response")]
    Encode(#[source] ProtocolError),
    #[error("session operation failed")]
    Session(#[from] SessionError),
    #[error("blocking task failed")]
    Blocking(#[source] JoinError),
}

impl Rejection {
    fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_) | Self::InvalidPath(_) => StatusCode::BAD_REQUEST,
            Self::PlaceMismatch { .. } => StatusCode::FORBIDDEN,
            Self::Encode(_) | Self::Session(_) | Self::Blocking(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(target: "naht::server", status = status.as_u16(), error = ?self, "request failed");
            return (status, "internal server error").into_response();
        }
        (status, self.to_string()).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::Request;
    use naht_core::state::StateStore;
    use tower::ServiceExt;

    use super::*;

    fn test_state() -> Arc<AppState> {
        let store = StateStore::open_in_memory().unwrap();
        let session = Session::new(PathBuf::from("."), store, "demo", None, false);
        AppState::new(session, Duration::from_secs(1))
    }

    async fn post_blobs_status(len: usize) -> StatusCode {
        let body = Body::from(vec![0xc1_u8; len]);
        let request = Request::post("/blobs").body(body).unwrap();
        router(test_state())
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn blob_bodies_past_the_extractor_default_are_read() {
        assert_eq!(
            post_blobs_status(3 * 1024 * 1024).await,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn blob_bodies_over_the_limit_are_refused() {
        assert_eq!(
            post_blobs_status(BODY_LIMIT + 1).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
