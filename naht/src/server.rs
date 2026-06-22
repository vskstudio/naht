//! The localhost HTTP server the Studio plugin talks to (architecture §7).
//!
//! Four endpoints, all MessagePack: `GET /info` (handshake + `servePlaceId` guard + reconnect
//! re-diff), `GET /patches` (long-poll held open until a change or timeout), `POST /changes`
//! (Studio → filesystem), and `GET /heartbeat` (liveness). All sync decisions are delegated to the
//! [`Session`]; this layer is pure transport.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use naht_core::protocol::{
    self, Ack, BlobChangeBatch, BlobPatchBatch, ChangeBatch, PatchBatch, Pong,
};
use serde::Deserialize;
use tokio::sync::{Mutex, Notify};

use crate::session::Session;

/// Shared server state: the session behind a mutex, plus notifiers that wake parked long-polls
/// whenever a queue may have grown. Text patches and binary blobs ride separate channels, so each
/// has its own notifier and parks independently.
pub struct AppState {
    session: Mutex<Session>,
    patches_ready: Notify,
    blobs_ready: Notify,
    long_poll: Duration,
}

impl AppState {
    /// Wrap a session, parking long-polls for at most `long_poll` before they return empty.
    pub fn new(session: Session, long_poll: Duration) -> Arc<Self> {
        Arc::new(Self {
            session: Mutex::new(session),
            patches_ready: Notify::new(),
            blobs_ready: Notify::new(),
            long_poll,
        })
    }

    /// Lock the session for exclusive access. Used by the watcher to reconcile on a filesystem event.
    pub async fn session_lock(&self) -> tokio::sync::MutexGuard<'_, Session> {
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
        .with_state(state)
}

/// Serve `state` on an already-bound `listener` until the process ends.
pub async fn serve(listener: tokio::net::TcpListener, state: Arc<AppState>) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

#[derive(Deserialize)]
struct InfoQuery {
    place_id: Option<u64>,
}

async fn info(State(state): State<Arc<AppState>>, Query(query): Query<InfoQuery>) -> Response {
    let mut session = state.session.lock().await;
    if let Some(expected) = session.serve_place_id() {
        if query.place_id != Some(expected) {
            tracing::warn!(
                target: "naht::server",
                expected,
                reported = ?query.place_id,
                "handshake rejected: place id mismatch"
            );
            return (
                StatusCode::FORBIDDEN,
                format!(
                    "place id mismatch: project serves {expected}, plugin reported {:?}",
                    query.place_id
                ),
            )
                .into_response();
        }
    }
    if let Err(error) = session.rescan() {
        return internal_error(&error);
    }
    let info = session.info();
    drop(session);
    tracing::info!(target: "naht::server", "handshake ok; re-diffed on connect");
    state.patches_ready.notify_waiters();
    state.blobs_ready.notify_waiters();
    msgpack(&info)
}

/// The long-poll cursor query, shared by the `/patches` and `/blobs` channels.
#[derive(Deserialize)]
struct CursorQuery {
    cursor: Option<u64>,
}

async fn patches(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    // Enrol as a waiter *before* checking the queue. `enable()` registers now, so a change that
    // enqueues and notifies between our check and our await is captured, not lost — otherwise the
    // future only registers when first polled and the wakeup would slip through, parking us for the
    // full timeout despite a patch being ready.
    let notified = state.patches_ready.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    {
        let session = state.session.lock().await;
        if session.has_patches_after(cursor) {
            return patch_batch(&session, cursor);
        }
    }
    let _ = tokio::time::timeout(state.long_poll, notified).await;

    let session = state.session.lock().await;
    patch_batch(&session, cursor)
}

async fn changes(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: ChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let count = batch.changes.len();
    let mut session = state.session.lock().await;
    if let Err(error) = session.apply_changes(batch.changes) {
        return internal_error(&error);
    }
    drop(session);
    tracing::info!(target: "naht::server", count, "changes applied from Studio");
    state.patches_ready.notify_waiters();
    StatusCode::OK.into_response()
}

async fn blobs(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    // Same parked-waiter discipline as `/patches`: enrol before checking the queue so a blob that
    // enqueues between the check and the await is captured, not lost.
    let notified = state.blobs_ready.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    {
        let session = state.session.lock().await;
        if session.has_blobs_after(cursor) {
            return blob_batch(&session, cursor);
        }
    }
    let _ = tokio::time::timeout(state.long_poll, notified).await;

    let session = state.session.lock().await;
    blob_batch(&session, cursor)
}

async fn post_blobs(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: BlobChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let count = batch.changes.len();
    let mut session = state.session.lock().await;
    if let Err(error) = session.apply_blob_changes(batch.changes) {
        return internal_error(&error);
    }
    drop(session);
    tracing::info!(target: "naht::server", count, "blob changes applied from Studio");
    state.blobs_ready.notify_waiters();
    StatusCode::OK.into_response()
}

async fn ack(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let ack: Ack = match protocol::from_msgpack(&body) {
        Ok(ack) => ack,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let mut session = state.session.lock().await;
    if let Err(error) = session.ack(&ack.paths) {
        return internal_error(&error);
    }
    tracing::debug!(target: "naht::server", count = ack.paths.len(), "patches acked");
    StatusCode::OK.into_response()
}

async fn heartbeat(State(state): State<Arc<AppState>>) -> Response {
    let session_id = state.session.lock().await.info().session_id;
    msgpack(&Pong { session_id })
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
        Err(error) => internal_error(&error),
    }
}

fn internal_error(error: &impl std::fmt::Display) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
}
