use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use naht_core::protocol::ProtocolError;
use naht_core::vfs::{ProjectPathError, VfsError};
use tokio::task::JoinError;

use crate::session::SessionError;

#[derive(Debug, thiserror::Error)]
pub(super) enum Rejection {
    #[error("malformed message: {0}")]
    Malformed(#[source] ProtocolError),
    #[error(transparent)]
    InvalidPath(#[from] VfsError),
    #[error("path not allowed: {0}")]
    Disallowed(#[from] ProjectPathError),
    #[error("cannot encode the response")]
    Encode(#[source] ProtocolError),
    #[error("session operation failed")]
    Session(#[from] SessionError),
    #[error("blocking task failed")]
    Blocking(#[source] JoinError),
    #[error("session busy, retry the request")]
    Busy,
    #[error("session changed: the daemon restarted, re-handshake with /info")]
    SessionChanged,
    #[error("refused project paths: {0:?}")]
    RefusedPaths(Vec<String>),
}

impl Rejection {
    fn status(&self) -> StatusCode {
        match self {
            Self::Malformed(_)
            | Self::InvalidPath(_)
            | Self::Disallowed(_)
            | Self::RefusedPaths(_) => StatusCode::BAD_REQUEST,
            Self::Busy => StatusCode::SERVICE_UNAVAILABLE,
            Self::SessionChanged => StatusCode::CONFLICT,
            Self::Encode(_) | Self::Session(_) | Self::Blocking(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        let status = self.status();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(target: "naht::server", status = status.as_u16(), error = ?self, "request failed");
            return (status, "internal server error").into_response();
        }
        if let Self::RefusedPaths(refused_paths) = &self {
            let body = serde_json::json!({ "refused_paths": refused_paths });
            return (
                status,
                [(header::CONTENT_TYPE, "application/json")],
                body.to_string(),
            )
                .into_response();
        }
        (status, self.to_string()).into_response()
    }
}
