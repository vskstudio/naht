use axum::body::to_bytes;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use naht_core::vfs::VfsError;

use super::Rejection;

#[tokio::test]
async fn an_invalid_path_is_answered_as_json_with_the_client_path_escaped() {
    let client_path = "Dir/\u{1b}[31m<b>\"x\"\n&.terrain";
    let response =
        Rejection::InvalidPath(VfsError::InvalidPath(client_path.to_owned())).into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(!body.contains(&0x1b), "a control byte reached the body");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["error"],
        format!("invalid project path: {client_path}")
    );
}
