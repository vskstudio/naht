//! An unpublished place is logged once per session. Alone in its binary, no sibling test can prime tracing's callsite cache.

#![cfg(test)]

use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use naht::server::{AppState, router};
use naht::session::Session;
use naht::token;
use naht_core::protocol::{CLIENT_HEADER, PLACE_ID_HEADER, TOKEN_HEADER};
use naht_core::state::StateStore;
use tower::ServiceExt;
use tracing::subscriber::set_default;

const PORT: u16 = 34874;

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl LogBuffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

fn bound_state(root: &Path) -> (Arc<AppState>, String) {
    fs::create_dir_all(root.join(".naht")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let session = Session::new(root, store, "demo", None, false).unwrap();
    let token = token::load_or_create(root).unwrap().token;
    let text = token.as_str().to_owned();
    (AppState::new(session, Duration::from_secs(1), token), text)
}

fn info_request(place: &str, token: &str) -> Request<Body> {
    Request::get("/info")
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, token)
        .header(header::HOST, format!("127.0.0.1:{PORT}"))
        .header(PLACE_ID_HEADER, place)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn an_unpublished_place_is_logged_once_per_session() {
    let root = tempfile::tempdir().unwrap();
    let (state, token) = bound_state(root.path());
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();
    let _default = set_default(subscriber);
    let app = router(Arc::clone(&state), PORT);

    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(info_request("0", &token))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let text = logs.text();
    assert_eq!(
        text.matches("names an unpublished place").count(),
        1,
        "{text}"
    );
}
