use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use naht_core::state::StateStore;
use tower::ServiceExt;

use super::{CLIENT_HEADER, PLACE_ID_HEADER, TOKEN_HEADER};
use crate::server::{AppState, router};
use crate::session::Session;
use crate::token::{TEST_TOKEN, Token};

const PORT: u16 = 34873;

fn bound_state(root: &Path) -> Arc<AppState> {
    fs::create_dir_all(root.join(".naht")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let session = Session::new(root, store, "demo", None, false).unwrap();
    AppState::new(session, Duration::from_secs(1), Token::for_tests())
}

fn info_request(place: Option<&str>) -> Request<Body> {
    let request = Request::get("/info")
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, TEST_TOKEN)
        .header(header::HOST, format!("127.0.0.1:{PORT}"));
    let request = match place {
        Some(place) => request.header(PLACE_ID_HEADER, place),
        None => request,
    };
    request.body(Body::empty()).unwrap()
}

async fn info(state: &Arc<AppState>, place: Option<&str>) -> (StatusCode, String) {
    let response = router(Arc::clone(state), PORT)
        .oneshot(info_request(place))
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn an_unconfigured_project_accepts_the_place_it_was_bound_to() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());

    assert_eq!(info(&state, Some("11")).await.0, StatusCode::OK);
    assert_eq!(info(&state, Some("11")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_second_place_is_refused_with_the_reset_instructions() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());
    info(&state, Some("11")).await;

    let (status, body) = info(&state, Some("22")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("bound to place 11, not 22"), "{body}");
    assert!(body.contains("naht unbind"), "{body}");
    assert_eq!(info(&state, Some("11")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn an_unpublished_place_does_not_bind_the_project() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());

    assert_eq!(info(&state, Some("0")).await.0, StatusCode::OK);
    assert_eq!(info(&state, Some("11")).await.0, StatusCode::OK);
    assert_eq!(info(&state, Some("22")).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unpublished_place_is_refused_once_the_project_is_bound() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());
    info(&state, Some("11")).await;

    let (status, body) = info(&state, Some("0")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("unpublished"), "{body}");
    assert!(body.contains("bound to place 11"), "{body}");
}

#[tokio::test]
async fn the_binding_survives_a_restart() {
    let root = tempfile::tempdir().unwrap();
    let first = bound_state(root.path());
    info(&first, Some("11")).await;
    drop(first);

    let second = bound_state(root.path());

    assert_eq!(info(&second, Some("22")).await.0, StatusCode::FORBIDDEN);
    assert_eq!(info(&second, Some("11")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_request_without_a_place_header_binds_nothing() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());

    assert_eq!(info(&state, None).await.0, StatusCode::OK);
    assert_eq!(info(&state, Some("22")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_malformed_place_header_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());

    let (status, body) = info(&state, Some("not-a-place")).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("malformed place id header"), "{body}");
}

fn write_request(place: Option<&str>) -> Request<Body> {
    let request = Request::post("/changes")
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, TEST_TOKEN)
        .header(header::HOST, format!("127.0.0.1:{PORT}"));
    let request = match place {
        Some(place) => request.header(PLACE_ID_HEADER, place),
        None => request,
    };
    request.body(Body::empty()).unwrap()
}

async fn write(state: &Arc<AppState>, place: Option<&str>) -> (StatusCode, String) {
    let response = router(Arc::clone(state), PORT)
        .oneshot(write_request(place))
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn a_write_without_a_place_header_is_refused_once_a_place_is_bound() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());
    info(&state, Some("11")).await;

    let (status, body) = write(&state, None).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("bound to place 11"), "{body}");
    assert_ne!(write(&state, Some("11")).await.0, StatusCode::FORBIDDEN);
    assert_eq!(info(&state, None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_write_without_a_place_header_is_refused_after_a_restart() {
    let root = tempfile::tempdir().unwrap();
    let first = bound_state(root.path());
    info(&first, Some("11")).await;
    drop(first);

    let second = bound_state(root.path());

    assert_eq!(write(&second, None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_write_without_a_place_header_passes_while_nothing_is_recorded() {
    let root = tempfile::tempdir().unwrap();
    let state = bound_state(root.path());

    assert_ne!(write(&state, None).await.0, StatusCode::FORBIDDEN);
}
