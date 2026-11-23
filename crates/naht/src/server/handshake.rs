use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::response::{IntoResponse, Response};
use naht_core::protocol::{self, ServerInfo};
use serde::Deserialize;

use super::guard::Authenticated;
use super::rejection::Rejection;
use super::{AppState, msgpack};
use crate::session::Session;

#[derive(Clone, Copy, Debug)]
pub(super) struct ServedPort(pub(super) u16);

#[derive(Debug, Deserialize)]
pub(super) struct InfoQuery {
    nonce: Option<String>,
}

pub(super) async fn info(
    State(state): State<Arc<AppState>>,
    Extension(ServedPort(port)): Extension<ServedPort>,
    authenticated: Option<Extension<Authenticated>>,
    Query(query): Query<InfoQuery>,
) -> Response {
    let proof = match prove(&state, port, query.nonce.as_deref()) {
        Ok(proof) => proof,
        Err(rejection) => return rejection.into_response(),
    };
    let authenticated = authenticated.is_some();
    let outcome = state
        .with_session(move |session| handshake(session, authenticated))
        .await;
    let mut info = match outcome {
        Ok(Ok(info)) => info,
        Ok(Err(rejection)) => return rejection.into_response(),
        Err(error) => return Rejection::Blocking(error).into_response(),
    };
    info.proof = proof;
    if authenticated {
        tracing::info!(target: "naht::server", "handshake ok; re-diffed on connect");
        state.wake_all_long_polls();
    }
    msgpack(&info)
}

fn prove(state: &AppState, port: u16, nonce: Option<&str>) -> Result<Option<String>, Rejection> {
    let Some(nonce) = nonce else {
        return Ok(None);
    };
    if !protocol::is_valid_nonce(nonce) {
        return Err(Rejection::MalformedNonce);
    }
    state
        .token()
        .prove(port, nonce)
        .map(Some)
        .ok_or(Rejection::ProofUnavailable)
}

fn handshake(session: &mut Session, authenticated: bool) -> Result<ServerInfo, Rejection> {
    if authenticated {
        session.rescan()?;
    }
    Ok(session.info())
}
