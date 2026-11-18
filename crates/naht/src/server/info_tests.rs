use std::fs;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use super::tests::{TEST_PORT, fresh_state};
use super::{AppState, guard, router};
use crate::token::TEST_TOKEN;

async fn info_status(state: &Arc<AppState>, token: Option<&str>) -> StatusCode {
    let mut request = Request::get("/info")
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"));
    if let Some(token) = token {
        request = request.header(guard::TOKEN_HEADER, token);
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
