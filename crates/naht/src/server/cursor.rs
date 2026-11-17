use serde::Deserialize;

use crate::session::Session;

use super::rejection::Rejection;

#[derive(Deserialize)]
pub(super) struct CursorQuery {
    pub(super) cursor: Option<u64>,
    pub(super) session: Option<String>,
}

impl CursorQuery {
    pub(super) fn check(&self, session: &Session) -> Result<(), Rejection> {
        match &self.session {
            Some(requested) if requested != session.session_id() => Err(Rejection::SessionChanged),
            _ => Ok(()),
        }
    }
}
