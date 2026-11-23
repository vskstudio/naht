//! The localhost HTTP server the Studio plugin talks to (architecture §7).
//!
//! All endpoints speak `MessagePack`: `GET /info` (handshake, daemon proof, `servePlaceId` guard, reconnect
//! re-diff when authenticated), `GET /patches` (text long-poll held open until a change or timeout), `POST /changes`
//! (Studio → filesystem), `GET /blobs` + `POST /blobs` (the separate terrain blob channel),
//! `POST /ack` (the plugin confirms applied patches), and `GET /heartbeat` (liveness). All sync
//! decisions are delegated to the [`Session`]; this layer is pure transport.

use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::token::Token;
use axum::Router;
use axum::body::Bytes;
use axum::error_handling::HandleErrorLayer;
use axum::extract::DefaultBodyLimit;
use axum::extract::{Extension, Query, State};
use axum::http::{StatusCode, header};
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use cursor::{CursorQuery, SessionQuery};
use naht_core::binary::{BlobInstance, TERRAIN_CLASS, terrain_voxel_count};
use naht_core::protocol::{
    self, Ack, BlobChangeBatch, BlobPatchBatch, ChangeBatch, PatchBatch, Pong,
};
use naht_core::vfs::validate_blob_path;
use rejection::Rejection;
use serde::Deserialize;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinError;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::limit::ConcurrencyLimitLayer;
use tower::load_shed::LoadShedLayer;
use tower::timeout::TimeoutLayer;
use tower::timeout::error::Elapsed;
use tower::{BoxError, ServiceBuilder};
use tower_http::limit::RequestBodyLimitLayer;

use crate::config::Limits;
use crate::session::Session;

mod guard;
mod handshake;
mod inventory;
mod listen;
mod refuse;

use guard::{Gate, PostGate, guard, limit_post_bodies};
use handshake::ServedPort;
use refuse::{refuse_directories, refused_text_paths};
use std::io;
use tokio::net::TcpListener;
use tokio::sync::MutexGuard;
use tokio::task;
use tokio::time;

const BODY_LIMIT: usize = 64 * 1024 * 1024;

const INVENTORY_ROUTE: &str = "/inventory";

const MAX_IN_FLIGHT: usize = 64;

const INFO_ROUTE: &str = "/info";

const TIMEOUT_SLACK: Duration = Duration::from_secs(10);

const POST_WAIT: Duration = Duration::from_secs(10);

/// Shared server state: the session behind a mutex and notifiers that wake parked long-polls.
///
/// Notifiers fire whenever a queue may have grown. Text patches and binary blobs ride separate
/// channels, so each has its own notifier and parks independently.
#[derive(Debug)]
pub struct AppState {
    session: Arc<Mutex<Session>>,
    serve_place_id: Option<u64>,
    bound_place: OnceLock<u64>,
    patches_ready: Notify,
    blobs_ready: Notify,
    long_poll: Duration,
    post_wait: Duration,
    limits: Limits,
    token: Token,
    shutdown: CancellationToken,
    tasks: TaskTracker,
}

impl AppState {
    fn wake_all_long_polls(&self) {
        self.patches_ready.notify_waiters();
        self.blobs_ready.notify_waiters();
    }

    /// Wrap a session, parking long-polls for at most `long_poll` before they return empty. Every
    /// route except `GET /info` requires `token` in the `x-naht-token` header.
    pub fn new(session: Session, long_poll: Duration, token: Token) -> Arc<Self> {
        Self::with_post_wait(session, long_poll, POST_WAIT, token)
    }

    /// Like [`AppState::new`], but a mutating `POST` waits at most `post_wait` for a slot or the lock.
    pub fn with_post_wait(
        session: Session,
        long_poll: Duration,
        post_wait: Duration,
        token: Token,
    ) -> Arc<Self> {
        let serve_place_id = session.serve_place_id();
        let limits = session.limits();
        let bound_place = session
            .recorded_place_id()
            .map_or_else(OnceLock::new, OnceLock::from);
        Arc::new(Self {
            session: Arc::new(Mutex::new(session)),
            serve_place_id,
            bound_place,
            patches_ready: Notify::new(),
            blobs_ready: Notify::new(),
            long_poll,
            post_wait,
            limits,
            token,
            shutdown: CancellationToken::new(),
            tasks: TaskTracker::new(),
        })
    }

    /// The token every request but `GET /info` must present.
    pub fn token(&self) -> &Token {
        &self.token
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
    ///
    /// # Errors
    ///
    /// Returns [`JoinError`] when the blocking task panics or is cancelled.
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

/// Build the router over `state`, serving the loopback port `port`.
pub fn router(state: Arc<AppState>, port: u16) -> Router {
    let gate = Arc::new(Gate::new(port, Arc::clone(&state)));
    let posts = Arc::new(PostGate::new(
        state.post_wait,
        state.tasks.clone(),
        state.limits.inventory_body_bytes,
    ));
    let body_ceiling = BODY_LIMIT.max(state.limits.inventory_body_bytes);
    let read_deadline = ServiceBuilder::new()
        .layer(HandleErrorLayer::<_, ()>::new(overload))
        .layer(TimeoutLayer::new(
            state.long_poll.saturating_add(TIMEOUT_SLACK),
        ));
    Router::new()
        .route(
            INFO_ROUTE,
            get(handshake::info).layer(read_deadline.clone()),
        )
        .route("/patches", get(patches).layer(read_deadline.clone()))
        .route("/changes", post(changes))
        .route(INVENTORY_ROUTE, post(inventory::inventory))
        .route(
            "/blobs",
            get(blobs).layer(read_deadline.clone()).post(post_blobs),
        )
        .route("/ack", post(ack))
        .route("/heartbeat", get(heartbeat).layer(read_deadline.clone()))
        .route("/activity", get(activity).layer(read_deadline))
        .layer(from_fn_with_state(posts, limit_post_bodies))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::<_, ()>::new(overload))
                .layer(LoadShedLayer::new())
                .layer(ConcurrencyLimitLayer::new(MAX_IN_FLIGHT))
                .layer(RequestBodyLimitLayer::new(body_ceiling))
                .layer(DefaultBodyLimit::max(body_ceiling)),
        )
        .layer(from_fn_with_state(gate, guard))
        .layer(Extension(ServedPort(port)))
        .with_state(state)
}

async fn overload(error: BoxError) -> StatusCode {
    if error.is::<Elapsed>() {
        StatusCode::REQUEST_TIMEOUT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn apply_within<T, F>(state: &AppState, work: F) -> Result<T, Rejection>
where
    T: Send + 'static,
    F: FnOnce(&mut Session) -> Result<T, Rejection> + Send + 'static,
{
    let session = Arc::clone(&state.session);
    let mut locked = time::timeout(state.post_wait, session.lock_owned())
        .await
        .map_err(|elapsed| {
            tracing::warn!(target: "naht::server", %elapsed, "session busy past the post wait");
            Rejection::Busy
        })?;
    task::spawn_blocking(move || work(&mut locked))
        .await
        .map_err(Rejection::Blocking)?
}

/// Serve `state` on an already-bound `listener` until the shutdown token is cancelled, then let
/// in-flight requests finish.
///
/// # Errors
///
/// Returns an error when the listener fails.
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> io::Result<()> {
    let port = listener.local_addr()?.port();
    let shutdown = state.shutdown.clone();
    let connections = state.limits.connections;
    listen::accept_until_cancelled(listener, router(state, port), shutdown, connections).await
}

async fn patches(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    let notified = enrol_waiter(&state.patches_ready);
    {
        let session = state.session.lock().await;
        if let Err(rejection) = query.check(&session) {
            return rejection.into_response();
        }
        if session.has_patches_after(cursor) {
            return patch_batch(&session, cursor);
        }
    }
    let _parked = time::timeout(state.long_poll, notified).await;

    let session = state.session.lock().await;
    patch_batch(&session, cursor)
}

async fn changes(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: ChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let count = match apply_within(&state, move |session| apply_text_batch(session, batch)).await {
        Ok(count) => count,
        Err(rejection) => return rejection.into_response(),
    };
    tracing::info!(target: "naht::server", count, "changes applied from Studio");
    state.wake_all_long_polls();
    StatusCode::OK.into_response()
}

fn apply_text_batch(session: &mut Session, batch: ChangeBatch) -> Result<usize, Rejection> {
    let refused = refused_text_paths(session, &batch.changes);
    if !refused.is_empty() {
        return Err(Rejection::RefusedPaths(refused));
    }
    let count = batch.changes.len();
    session.apply_changes(batch.changes)?;
    Ok(count)
}

async fn blobs(State(state): State<Arc<AppState>>, Query(query): Query<CursorQuery>) -> Response {
    let cursor = query.cursor.unwrap_or(0);

    let notified = enrol_waiter(&state.blobs_ready);
    {
        let mut session = state.session.lock().await;
        if let Err(rejection) = query.check(&session) {
            return rejection.into_response();
        }
        if session.has_blobs_after(cursor) {
            return blob_batch(&mut session, cursor);
        }
    }
    let _parked = time::timeout(state.long_poll, notified).await;

    let mut session = state.session.lock().await;
    blob_batch(&mut session, cursor)
}

async fn post_blobs(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let batch: BlobChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let terrain_voxels = state.limits.terrain_voxels;
    let count = match apply_within(&state, move |session| {
        apply_blob_batch(session, batch, terrain_voxels)
    })
    .await
    {
        Ok(count) => count,
        Err(rejection) => return rejection.into_response(),
    };
    tracing::info!(target: "naht::server", count, "blob changes applied from Studio");
    state.blobs_ready.notify_waiters();
    StatusCode::OK.into_response()
}

fn apply_blob_batch(
    session: &mut Session,
    batch: BlobChangeBatch,
    terrain_voxels: usize,
) -> Result<usize, Rejection> {
    refuse_other_blobs(&batch.changes)?;
    refuse_oversized_terrain(&batch.changes, terrain_voxels)?;
    for change in &batch.changes {
        session.validate_path(&change.path)?;
    }
    refuse_directories(
        session,
        batch.changes.iter().map(|change| change.path.as_str()),
    )?;
    let count = batch.changes.len();
    session.apply_blob_changes(batch.changes)?;
    Ok(count)
}

fn refuse_oversized_terrain(changes: &[BlobInstance], limit: usize) -> Result<(), Rejection> {
    for change in changes
        .iter()
        .filter(|change| change.class == TERRAIN_CLASS)
    {
        let voxels = terrain_voxel_count(&change.content)
            .ok_or_else(|| Rejection::UnreadableTerrain(change.path.clone()))?;
        if voxels > limit {
            return Err(Rejection::TerrainTooLarge { voxels, limit });
        }
    }
    Ok(())
}

fn refuse_other_blobs(changes: &[BlobInstance]) -> Result<(), Rejection> {
    let refused: Vec<String> = changes
        .iter()
        .filter(|change| validate_blob_path(&change.path).is_err())
        .map(|change| change.path.clone())
        .collect();
    if refused.is_empty() {
        Ok(())
    } else {
        Err(Rejection::RefusedPaths(refused))
    }
}

async fn ack(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    body: Bytes,
) -> Response {
    let ack: Ack = match protocol::from_msgpack(&body) {
        Ok(ack) => ack,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let count = ack.seqs.len();
    match apply_within(&state, move |session| {
        query.check(session)?;
        session.ack(&ack.seqs).map_err(Rejection::from)
    })
    .await
    {
        Ok(()) => {
            tracing::debug!(target: "naht::server", count, "patches acked");
            StatusCode::OK.into_response()
        }
        Err(rejection) => rejection.into_response(),
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

fn blob_batch(session: &mut Session, cursor: u64) -> Response {
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

mod cursor;
#[cfg(test)]
mod info_tests;
mod rejection;
#[cfg(test)]
mod tests;
