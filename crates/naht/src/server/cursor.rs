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
        check_session(self.session.as_deref(), session)
    }
}

#[derive(Deserialize)]
pub(super) struct SessionQuery {
    pub(super) session: Option<String>,
}

impl SessionQuery {
    pub(super) fn check(&self, session: &Session) -> Result<(), Rejection> {
        check_session(self.session.as_deref(), session)
    }
}

fn check_session(requested: Option<&str>, session: &Session) -> Result<(), Rejection> {
    match requested {
        None => Err(Rejection::MissingSession),
        Some(requested) if requested != session.session_id() => Err(Rejection::SessionChanged),
        Some(_) => Ok(()),
    }
}
