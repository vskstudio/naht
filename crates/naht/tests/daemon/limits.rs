use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use naht::config::Limits;
use naht_core::binary::{BlobInstance, TERRAIN_CLASS};
use naht_core::protocol::TERRAIN_PATH;
use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net;
use tokio::task;
use tokio::time;

use super::support::{CLIENT_HEADER, Harness};

fn terrain_blob(nx: i32, ny: i32, nz: i32) -> BlobInstance {
    let mut content = Vec::new();
    for dimension in [nx, ny, nz] {
        content.extend_from_slice(&dimension.to_le_bytes());
    }
    content.resize(40, 0);
    BlobInstance::new(TERRAIN_PATH, TERRAIN_CLASS, content)
}

const PRODUCTION_LIKE_LONG_POLL: Duration = Duration::from_secs(25);

const BODY_LIMIT: usize = 64 * 1024 * 1024;

fn read_until_server_closes(addr: &str) -> io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.write_all(b"GET /heartbeat HTTP/1.1\r\nHost: 127.0.0.1\r\n")?;
    let mut received = Vec::new();
    stream.read_to_end(&mut received)?;
    Ok(received)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stalled_request_head_is_dropped_by_the_server() {
    let harness = Harness::start(None).await;
    let addr = harness.base.trim_start_matches("http://").to_owned();

    let outcome = task::spawn_blocking(move || read_until_server_closes(&addr)).await;

    assert!(
        matches!(outcome, Ok(Ok(_))),
        "the server kept a stalled request head open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_post_bodies_do_not_hold_the_body_slots_forever() {
    let harness = Harness::start_configured(None, true, PRODUCTION_LIKE_LONG_POLL).await;
    let addr = harness.base.trim_start_matches("http://").to_owned();
    let stalled_head = format!(
        "POST /changes HTTP/1.1\r\nHost: {addr}\r\n{CLIENT_HEADER}: plugin\r\nContent-Length: 100000\r\n\r\n"
    );

    let mut stalled = Vec::new();
    for _ in 0..2 {
        let mut stream = net::TcpStream::connect(&addr).await.unwrap();
        stream.write_all(stalled_head.as_bytes()).await.unwrap();
        stalled.push(stream);
    }

    let shed = time::timeout(Duration::from_secs(12), harness.ack(Vec::new())).await;
    assert!(
        matches!(shed, Ok(status) if status == StatusCode::SERVICE_UNAVAILABLE),
        "a POST behind two stalled bodies was not answered 503 within the post wait"
    );

    let reply = time::timeout(Duration::from_secs(20), first_line(&mut stalled[0]))
        .await
        .unwrap()
        .unwrap();
    assert!(reply.starts_with("HTTP/1.1 408"), "{reply}");

    assert!(harness.ack(Vec::new()).await.is_success());
}

async fn first_line(stream: &mut net::TcpStream) -> io::Result<String> {
    let mut received = Vec::new();
    let mut chunk = [0_u8; 256];
    while !received.windows(2).any(|pair| pair == b"\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        received.extend_from_slice(chunk.get(..read).unwrap_or_default());
    }
    Ok(String::from_utf8_lossy(&received).into_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_over_64_mib_is_refused_with_413() {
    let harness = Harness::start(None).await;
    let addr = harness.base.trim_start_matches("http://");
    let request = format!(
        "POST /changes HTTP/1.1\r\nHost: {addr}\r\n{CLIENT_HEADER}: plugin\r\nExpect: 100-continue\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        BODY_LIMIT + 1
    );

    let reply = harness.raw_reply(&request).await.unwrap();

    assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_truncated_post_body_is_refused_with_400() {
    let harness = Harness::start(None).await;
    let addr = harness.base.trim_start_matches("http://").to_owned();
    let truncated = format!(
        "POST /changes HTTP/1.1\r\nHost: {addr}\r\n{CLIENT_HEADER}: plugin\r\nContent-Length: 1000\r\nConnection: close\r\n\r\nshort"
    );

    let mut stream = net::TcpStream::connect(&addr).await.unwrap();
    stream.write_all(truncated.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
    let reply = time::timeout(Duration::from_secs(20), first_line(&mut stream))
        .await
        .unwrap()
        .unwrap();

    assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_inventory_body_over_the_configured_limit_is_refused_with_413() {
    let limits = Limits {
        inventory_body_bytes: 1024,
        ..Limits::default()
    };
    let harness = Harness::start_limited(None, true, Duration::from_millis(300), limits).await;

    assert_eq!(
        harness.post_bytes("/inventory", vec![0_u8; 2048]).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_ne!(
        harness.post_bytes("/inventory", vec![0_u8; 512]).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "a body within the configured limit was refused as too large"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terrain_blob_over_the_configured_voxel_cap_is_refused_with_413_and_json() {
    let limits = Limits {
        terrain_voxels: 10,
        ..Limits::default()
    };
    let harness = Harness::start_limited(None, true, Duration::from_millis(300), limits).await;

    let (status, body) = harness.post_blobs_reply(vec![terrain_blob(4, 4, 4)]).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    let reply: serde_json::Value = serde_json::from_str(&body).expect("the refusal is JSON");
    assert!(
        reply["error"]
            .as_str()
            .unwrap_or_default()
            .contains("64 voxels"),
        "{body}"
    );

    assert_eq!(
        harness.post_blobs(vec![terrain_blob(2, 2, 2)]).await,
        StatusCode::OK,
        "a terrain within the cap was refused"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_terrain_blob_without_a_readable_header_is_refused_with_400() {
    let harness = Harness::start_with_terrain(None, true).await;
    let truncated = BlobInstance::new(TERRAIN_PATH, TERRAIN_CLASS, vec![1_u8; 12]);

    let (status, body) = harness.post_blobs_reply(vec![truncated]).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("voxel header"), "{body}");
}
