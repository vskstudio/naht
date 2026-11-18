use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use naht_core::protocol::{self, Change, ChangeBatch};
use naht_core::reconciler::TextInstance;

use super::cursor::SessionQuery;
use super::rejection::Rejection;
use super::{AppState, apply_within, refused_text_paths};
use crate::session::Session;

pub(super) async fn inventory(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    body: Bytes,
) -> Response {
    let batch: ChangeBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let count = batch.changes.len();
    match apply_within(&state, move |session| {
        query.check(session)?;
        apply_inventory_batch(session, batch.changes)
    })
    .await
    {
        Ok(()) => {
            tracing::info!(target: "naht::server", count, "inventory reconciled from Studio");
            state.wake_all_long_polls();
            StatusCode::OK.into_response()
        }
        Err(rejection) => rejection.into_response(),
    }
}

fn apply_inventory_batch(session: &mut Session, changes: Vec<Change>) -> Result<(), Rejection> {
    let refused = refused_text_paths(session, &changes);
    if !refused.is_empty() {
        return Err(Rejection::RefusedPaths(refused));
    }
    let mut instances = Vec::with_capacity(changes.len());
    for change in changes {
        match change {
            Change::Upsert {
                path,
                class,
                content,
            } => instances.push(TextInstance::new(path, class, content)),
            Change::Delete { path } => return Err(Rejection::RefusedPaths(vec![path])),
        }
    }
    session.apply_inventory(&instances).map_err(Rejection::from)
}
