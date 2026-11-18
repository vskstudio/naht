use std::time::Duration;

use axum::Router;
use axum::routing::get;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use super::{MAX_CONNECTIONS, accept_until_cancelled};

const PING: &[u8] = b"GET /ping HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n";

async fn read_to_close(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test]
async fn a_connection_past_the_cap_waits_until_a_slot_frees_then_is_served() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let router = Router::new().route("/ping", get(|| async { "pong" }));
    let server = tokio::spawn(accept_until_cancelled(listener, router, shutdown.clone()));

    let mut idle = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        idle.push(TcpStream::connect(address).await.unwrap());
    }
    sleep(Duration::from_millis(200)).await;

    let mut waiting = TcpStream::connect(address).await.unwrap();
    waiting.write_all(PING).await.unwrap();
    let early = timeout(Duration::from_millis(500), read_to_close(&mut waiting)).await;
    assert!(
        early.is_err(),
        "a connection past the {MAX_CONNECTIONS} cap was served while the cap was full"
    );

    idle.pop();
    let response = timeout(Duration::from_secs(5), read_to_close(&mut waiting))
        .await
        .expect("the waiting connection was not served once a slot freed");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("pong"), "{response}");

    idle.clear();
    shutdown.cancel();
    server.await.unwrap().unwrap();
}

const HTTP2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

async fn a_connection_past_the_cap_is_served_once_the_held_connections_time_out(opening: &[u8]) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let router = Router::new().route("/ping", get(|| async { "pong" }));
    let server = tokio::spawn(accept_until_cancelled(listener, router, shutdown.clone()));

    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(opening).await.unwrap();
        held.push(stream);
    }

    let mut waiting = TcpStream::connect(address).await.unwrap();
    waiting.write_all(PING).await.unwrap();
    let response = timeout(Duration::from_secs(20), read_to_close(&mut waiting))
        .await
        .expect("a connection past the cap was not served once the held connections timed out");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("pong"), "{response}");

    drop(held);
    shutdown.cancel();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_silent_connection_frees_its_slot_at_the_header_timeout() {
    a_connection_past_the_cap_is_served_once_the_held_connections_time_out(b"").await;
}

#[tokio::test]
async fn an_http2_preface_frees_its_slot_at_the_header_timeout() {
    a_connection_past_the_cap_is_served_once_the_held_connections_time_out(HTTP2_PREFACE).await;
}
