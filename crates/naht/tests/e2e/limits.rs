use std::fs;
use std::path::Path;
use std::time::Duration;

use naht_core::binary::{BlobInstance, TERRAIN_CLASS};
use naht_core::protocol::{
    self, BlobChangeBatch, CLIENT_HEADER, ReportedLimits, ServerInfo, TERRAIN_PATH, TOKEN_HEADER,
};
use reqwest::{Client, RequestBuilder, StatusCode};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use super::{Daemon, init, wire};

const PROBE_WAIT: Duration = Duration::from_secs(1);
const RELEASE_WAIT: Duration = Duration::from_secs(5);
const WARNING: &str = "large deletion reconciled";

fn project(limits: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    let config =
        format!("[project]\nname = \"demo\"\n\n[serve]\nterrain_sync = true\n\n[limits]\n{limits}");
    fs::write(root.path().join("naht.toml"), config).unwrap();
    root
}

fn authed(request: RequestBuilder, token: &str) -> RequestBuilder {
    request
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, token)
}

async fn info(daemon: &Daemon) -> ServerInfo {
    let response = authed(
        daemon.client.get(format!("{}/info", daemon.base)),
        &daemon.token,
    )
    .send()
    .await
    .unwrap();
    assert!(
        response.status().is_success(),
        "GET /info: {}",
        response.status()
    );
    wire::decode(response).await
}

async fn post_bytes(daemon: &Daemon, path: &str, body: Vec<u8>) -> StatusCode {
    authed(
        daemon.client.post(format!("{}{path}", daemon.base)),
        &daemon.token,
    )
    .body(body)
    .send()
    .await
    .unwrap()
    .status()
}

async fn post_blobs(daemon: &Daemon, changes: Vec<BlobInstance>) -> (StatusCode, String) {
    let body = protocol::to_msgpack(&BlobChangeBatch { changes }).unwrap();
    let response = authed(
        daemon.client.post(format!("{}/blobs", daemon.base)),
        &daemon.token,
    )
    .body(body)
    .send()
    .await
    .unwrap();
    let status = response.status();
    (status, response.text().await.unwrap())
}

fn terrain_blob(nx: i32, ny: i32, nz: i32) -> BlobInstance {
    let mut content = Vec::new();
    for dimension in [nx, ny, nz] {
        content.extend_from_slice(&dimension.to_le_bytes());
    }
    content.resize(40, 0);
    BlobInstance::new(TERRAIN_PATH, TERRAIN_CLASS, content)
}

async fn stall(daemon: &Daemon) -> TcpStream {
    let addr = daemon.base.trim_start_matches("http://");
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(format!("GET /info HTTP/1.1\r\nHost: {addr}\r\n").as_bytes())
        .await
        .unwrap();
    stream
}

async fn probe(daemon: &Daemon, wait: Duration) -> Result<reqwest::Response, reqwest::Error> {
    let client = Client::builder()
        .timeout(wait)
        .pool_max_idle_per_host(0)
        .build()
        .unwrap();
    authed(client.get(format!("{}/info", daemon.base)), &daemon.token)
        .send()
        .await
}

async fn settle(daemon: &Daemon) {
    info(daemon).await;
    let seqs: Vec<u64> = daemon
        .pending_patches()
        .await
        .sequenced()
        .iter()
        .map(|sequenced| sequenced.seq)
        .collect();
    if !seqs.is_empty() {
        daemon.ack(seqs).await;
    }
}

fn write_files(dir: &Path, names: &[&str]) {
    for name in names {
        fs::write(dir.join(name), "return {}\n").unwrap();
    }
}

fn remove_files(dir: &Path, names: &[&str]) {
    for name in names {
        fs::remove_file(dir.join(name)).unwrap();
    }
}

#[tokio::test]
async fn the_configured_terrain_cap_is_reported_and_enforced_at_its_boundary() {
    let root = project("terrain_voxels = 8\n");
    let daemon = Daemon::start(root.path()).await;

    assert_eq!(
        info(&daemon).await.limits,
        Some(ReportedLimits { terrain_voxels: 8 })
    );
    assert_eq!(
        post_blobs(&daemon, vec![terrain_blob(2, 2, 2)]).await.0,
        StatusCode::OK,
        "exactly the configured voxel cap was refused"
    );
    let (status, body) = post_blobs(&daemon, vec![terrain_blob(3, 3, 1)]).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert!(body.contains("over the limit of 8"), "{body}");
}

#[tokio::test]
async fn the_configured_inventory_body_limit_is_enforced_at_its_boundary() {
    let root = project("inventory_body_bytes = 1024\n");
    let daemon = Daemon::start(root.path()).await;

    assert_ne!(
        post_bytes(&daemon, "/inventory", vec![0_u8; 1024]).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a body of exactly the configured limit was refused as too large"
    );
    assert_eq!(
        post_bytes(&daemon, "/inventory", vec![0_u8; 1025]).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn the_configured_connection_cap_holds_back_the_connection_past_it() {
    let root = project("connections = 2\n");
    let daemon = Daemon::start(root.path()).await;

    let first = stall(&daemon).await;
    assert!(
        probe(&daemon, PROBE_WAIT).await.is_ok(),
        "one stalled connection blocked a cap of two"
    );
    let second = stall(&daemon).await;
    assert!(
        probe(&daemon, PROBE_WAIT)
            .await
            .is_err_and(|error| error.is_timeout()),
        "a third connection was served past the cap of two"
    );

    drop(first);
    assert!(
        probe(&daemon, RELEASE_WAIT).await.is_ok(),
        "the held connection did not release its slot"
    );
    drop(second);
}

#[tokio::test]
async fn the_configured_mass_delete_threshold_warns_at_exactly_that_many_deletions() {
    let root = project("mass_delete_warn = 3\n");
    let logs = tempfile::tempdir().unwrap();
    let log = logs.path().join("serve.log");
    let daemon = Daemon::start_logged(root.path(), Some(&log)).await;
    let src = root.path().join("src");

    let two = ["Two_a.luau", "Two_b.luau"];
    write_files(&src, &two);
    settle(&daemon).await;
    remove_files(&src, &two);
    settle(&daemon).await;
    assert_eq!(
        fs::read_to_string(&log).unwrap().matches(WARNING).count(),
        0,
        "two deletions warned under a threshold of three"
    );

    let three = ["Three_a.luau", "Three_b.luau", "Three_c.luau"];
    write_files(&src, &three);
    settle(&daemon).await;
    remove_files(&src, &three);
    settle(&daemon).await;
    assert_eq!(
        fs::read_to_string(&log).unwrap().matches(WARNING).count(),
        1,
        "three deletions did not warn at the configured threshold of three"
    );
}
