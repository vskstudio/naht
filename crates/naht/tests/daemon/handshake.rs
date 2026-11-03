use naht_core::protocol::ServerInfo;

use super::support::{Harness, decode};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handshake_enforces_the_serve_place_guard() {
    let h = Harness::start(Some(999)).await;

    assert_eq!(
        h.info(Some(123)).await.status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(h.info(None).await.status(), reqwest::StatusCode::FORBIDDEN);

    let ok = h.info(Some(999)).await;
    assert!(ok.status().is_success());
    let info: ServerInfo = decode(ok).await;
    assert_eq!(info.serve_place_id, Some(999));
    assert_eq!(info.project_name, "demo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heartbeat_reports_the_session_id() {
    let h = Harness::start(None).await;
    let info: ServerInfo = decode(h.info(None).await).await;
    assert_eq!(h.heartbeat().await.session_id, info.session_id);
}
