use std::time::Duration;

use naht::config::Limits;
use naht_core::protocol::{ReportedLimits, ServerInfo};

use super::support::{Harness, decode};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_reports_the_terrain_cap_the_plugin_must_respect() {
    let limits = Limits {
        terrain_voxels: 10,
        ..Limits::default()
    };
    let harness = Harness::start_limited(None, true, Duration::from_millis(300), limits).await;
    let info: ServerInfo = decode(harness.info(None).await).await;
    assert_eq!(info.limits, Some(ReportedLimits { terrain_voxels: 10 }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handshake_enforces_the_serve_place_guard() {
    let harness = Harness::start(Some(999)).await;

    assert_eq!(
        harness.info(Some(123)).await.status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        harness.info(None).await.status(),
        reqwest::StatusCode::FORBIDDEN
    );

    let ok = harness.info(Some(999)).await;
    assert!(ok.status().is_success());
    let info: ServerInfo = decode(ok).await;
    assert_eq!(info.serve_place_id, Some(999));
    assert_eq!(info.project_name, "demo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heartbeat_reports_the_session_id() {
    let harness = Harness::start(None).await;
    let info: ServerInfo = decode(harness.info(None).await).await;
    assert_eq!(harness.heartbeat().await.session_id, info.session_id);
}
