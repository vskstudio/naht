use std::fs;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use naht_core::protocol;
use naht_core::state::StateStore;
use tower::ServiceExt;

use super::tests::{TEST_PORT, fresh_state};
use super::{AppState, guard, router};
use crate::session::Session;
use crate::token::{TEST_TOKEN, Token};

const SERVE_PLACE: u64 = 7;

fn serve_place_state() -> (tempfile::TempDir, Arc<AppState>) {
    let root = tempfile::tempdir().unwrap();
    let session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        Some(SERVE_PLACE),
        false,
    )
    .unwrap();
    let state = AppState::with_post_wait(
        session,
        Duration::from_secs(1),
        Duration::from_secs(1),
        Token::for_tests(),
    );
    (root, state)
}

async fn info_status(state: &Arc<AppState>, token: Option<&str>) -> StatusCode {
    info_status_at(state, token, None).await
}

async fn info_status_at(
    state: &Arc<AppState>,
    token: Option<&str>,
    place: Option<u64>,
) -> StatusCode {
    let mut request = Request::get("/info")
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"));
    if let Some(token) = token {
        request = request.header(guard::TOKEN_HEADER, token);
    }
    if let Some(place) = place {
        request = request.header(protocol::PLACE_ID_HEADER, place.to_string());
    }
    router(Arc::clone(state), TEST_PORT)
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn a_tokenless_info_does_not_rescan_the_project() {
    let (root, state) = fresh_state(Duration::from_secs(1));
    fs::write(root.path().join("Added.luau"), "return 1").unwrap();

    assert_eq!(info_status(&state, None).await, StatusCode::OK);
    assert!(!state.session.lock().await.has_patches_after(0));

    assert_eq!(info_status(&state, Some(TEST_TOKEN)).await, StatusCode::OK);
    assert!(state.session.lock().await.has_patches_after(0));
}

#[tokio::test]
async fn a_wrong_token_on_info_is_treated_like_no_token() {
    let (root, state) = fresh_state(Duration::from_secs(1));
    fs::write(root.path().join("Added.luau"), "return 1").unwrap();

    assert_eq!(
        info_status(&state, Some("not-the-token")).await,
        StatusCode::OK
    );
    assert!(!state.session.lock().await.has_patches_after(0));
}

#[tokio::test]
async fn a_tokenless_info_under_a_serve_place_does_not_rescan_or_bind() {
    let (root, state) = serve_place_state();
    fs::write(root.path().join("Added.luau"), "return 1").unwrap();

    assert_eq!(
        info_status_at(&state, None, Some(SERVE_PLACE)).await,
        StatusCode::OK
    );
    assert!(!state.session.lock().await.has_patches_after(0));
    assert!(state.bound_place.get().is_none());

    assert_eq!(
        info_status_at(&state, Some(TEST_TOKEN), Some(SERVE_PLACE)).await,
        StatusCode::OK
    );
    assert!(state.session.lock().await.has_patches_after(0));
}

async fn info_answer(
    state: &Arc<AppState>,
    query: &str,
    token: Option<&str>,
) -> (StatusCode, Option<protocol::ServerInfo>) {
    let mut request = Request::get(format!("/info{query}"))
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"));
    if let Some(token) = token {
        request = request.header(guard::TOKEN_HEADER, token);
    }
    let response = router(Arc::clone(state), TEST_PORT)
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, protocol::from_msgpack(&body).ok())
}

#[tokio::test]
async fn info_proves_the_nonce_with_the_token_and_the_served_port() {
    let (_root, state) = fresh_state(Duration::from_secs(1));
    let expected = Token::for_tests().prove(TEST_PORT, "abc123");

    let (status, info) = info_answer(&state, "?nonce=abc123", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(info.unwrap().proof, expected);
}

#[tokio::test]
async fn an_authenticated_info_proves_the_nonce_too() {
    let (_root, state) = fresh_state(Duration::from_secs(1));
    let expected = Token::for_tests().prove(TEST_PORT, "abc123");

    let (_, info) = info_answer(&state, "?nonce=abc123", Some(TEST_TOKEN)).await;

    assert_eq!(info.unwrap().proof, expected);
}

#[tokio::test]
async fn info_without_a_nonce_carries_no_proof() {
    let (_root, state) = fresh_state(Duration::from_secs(1));

    let (status, info) = info_answer(&state, "", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(info.unwrap().proof, None);
}

#[tokio::test]
async fn a_malformed_nonce_is_refused() {
    let (_root, state) = fresh_state(Duration::from_secs(1));
    let oversized = "a".repeat(protocol::NONCE_MAX_LEN + 1);

    for query in ["?nonce=", "?nonce=a%0Ab", &format!("?nonce={oversized}")] {
        let (status, _) = info_answer(&state, query, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
    }
}
