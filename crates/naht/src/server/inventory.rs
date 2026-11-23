use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use naht_core::inventory::{InventoryBatch, InventoryEntry, InventoryReply};
use naht_core::protocol;
use naht_core::vfs::validate_source_path;

use super::cursor::SessionQuery;
use super::rejection::Rejection;
use super::{AppState, apply_within};
use crate::session::Session;

pub(super) async fn inventory(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    body: Bytes,
) -> Response {
    let batch: InventoryBatch = match protocol::from_msgpack(&body) {
        Ok(batch) => batch,
        Err(error) => return Rejection::Malformed(error).into_response(),
    };
    let count = batch.scripts.len();
    match apply_within(&state, move |session| {
        query.check(session)?;
        apply_inventory_batch(session, &batch.scripts)
    })
    .await
    {
        Ok(wanted) => {
            tracing::info!(target: "naht::server", count, "inventory reconciled from Studio");
            state.wake_all_long_polls();
            Json(InventoryReply { wanted }).into_response()
        }
        Err(rejection) => rejection.into_response(),
    }
}

fn apply_inventory_batch(
    session: &mut Session,
    scripts: &[InventoryEntry],
) -> Result<Vec<String>, Rejection> {
    let refused: Vec<String> = scripts
        .iter()
        .filter(|script| is_refused_script(session, &script.path))
        .map(|script| script.path.clone())
        .collect();
    if !refused.is_empty() {
        return Err(Rejection::RefusedPaths(refused));
    }
    session.apply_inventory(scripts).map_err(Rejection::from)
}

fn is_refused_script(session: &Session, path: &str) -> bool {
    session.validate_path(path).is_err()
        || validate_source_path(path).is_err()
        || session.is_directory(path)
}
