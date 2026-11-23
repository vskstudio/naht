use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Extension;
use axum::Router;
use axum::routing::get;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use super::super::slot::ConnectionSlot;
use super::{ANONYMOUS_CONNECTIONS, accept_until_cancelled};

const AUTHENTICATED_CAP: usize = 64;

const PING: &[u8] = b"GET /ping HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n";

const KEEP_ALIVE_PING: &[u8] = b"GET /ping HTTP/1.1\r\nHost: test\r\n\r\n";

const PROMOTE: &[u8] = b"GET /promote HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n";

const HTTP2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

const DOCUMENTED_DEADLINE: Duration = Duration::from_secs(2);

const DEADLINE_SLACK: Duration = Duration::from_secs(1);

const SERVED_WAIT: Duration = Duration::from_secs(1);

const PROBE_MARGIN: Duration = Duration::from_millis(500);

const SEATS: usize = 16;

const MIN_OBSERVATION: Duration = Duration::from_secs(1);

const OBSERVED_FOR: Duration = DOCUMENTED_DEADLINE.saturating_sub(PROBE_MARGIN);

const HOLD: Duration = Duration::from_secs(3);

async fn read_to_close(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

async fn read_pong(stream: &mut TcpStream) {
    let mut response = Vec::new();
    while !response.ends_with(b"pong") {
        let read = stream.read_buf(&mut response).await.unwrap();
        assert!(
            read > 0,
            "the connection closed before its answer: {}",
            String::from_utf8_lossy(&response)
        );
    }
}

fn ping_router() -> Router {
    Router::new().route("/ping", get(|| async { "pong" }))
}

async fn promote_and_hold(Extension(slot): Extension<Arc<ConnectionSlot>>) -> &'static str {
    if !slot.promote().await {
        return "refused";
    }
    let holder = Arc::clone(&slot);
    tokio::spawn(async move {
        sleep(HOLD).await;
        drop(holder);
    });
    "promoted"
}

fn promoting_router() -> Router {
    Router::new().route("/promote", get(promote_and_hold))
}

async fn spawn_server(
    router: Router,
    connections: usize,
) -> (SocketAddr, CancellationToken, JoinHandle<io::Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let server = tokio::spawn(accept_until_cancelled(
        listener,
        router,
        shutdown.clone(),
        connections,
    ));
    (address, shutdown, server)
}

#[tokio::test]
async fn a_connection_past_the_anonymous_seats_waits_for_a_seat_then_is_served() {
    assert_eq!(
        ANONYMOUS_CONNECTIONS, SEATS,
        "this test pins the anonymous pool at {SEATS} seats"
    );
    let (address, shutdown, server) = spawn_server(ping_router(), AUTHENTICATED_CAP).await;

    let started = Instant::now();
    let mut seating = JoinSet::new();
    for _ in 0..SEATS {
        seating.spawn(seat(address));
    }
    let mut idle = Vec::new();
    while let Some(joined) = seating.join_next().await {
        idle.push(joined.expect("seating an anonymous connection panicked"));
    }

    let mut queued = TcpStream::connect(address).await.unwrap();
    queued.write_all(PING).await.unwrap();
    let window = (started + OBSERVED_FOR).saturating_duration_since(Instant::now());
    assert!(
        window >= MIN_OBSERVATION,
        "the {SEATS} anonymous seats took {:?} to fill, too slow to observe the connection past them before the first seat expires",
        started.elapsed()
    );
    let early = timeout(window, read_to_close(&mut queued)).await;
    assert!(
        early.is_err(),
        "the connection past the anonymous seats was answered or closed before the seats could expire"
    );

    idle.pop();
    let response = timeout(HOLD, read_to_close(&mut queued))
        .await
        .expect("the queued connection was not served once a seat freed");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("pong"), "{response}");

    idle.clear();
    shutdown.cancel();
    server.await.unwrap().unwrap();
}

async fn seat(address: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(KEEP_ALIVE_PING).await.unwrap();
    timeout(SERVED_WAIT, read_pong(&mut stream))
        .await
        .expect("a connection that should hold an anonymous seat was not answered");
    stream
}

async fn a_new_connection_is_served_once_the_held_connections_expire(opening: &[u8]) {
    let (address, shutdown, server) = spawn_server(ping_router(), AUTHENTICATED_CAP).await;

    let mut held = Vec::new();
    for _ in 0..SEATS {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(opening).await.unwrap();
        held.push(stream);
    }
    sleep(DOCUMENTED_DEADLINE.saturating_add(DEADLINE_SLACK)).await;

    let mut fresh = TcpStream::connect(address).await.unwrap();
    fresh.write_all(PING).await.unwrap();
    let response = timeout(SERVED_WAIT, read_to_close(&mut fresh))
        .await
        .expect("a new connection was not served once the held connections expired");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("pong"), "{response}");

    drop(held);
    shutdown.cancel();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_silent_connection_frees_its_seat_at_the_anonymous_deadline() {
    a_new_connection_is_served_once_the_held_connections_expire(b"").await;
}

#[tokio::test]
async fn an_http2_preface_frees_its_seat_at_the_anonymous_deadline() {
    a_new_connection_is_served_once_the_held_connections_expire(HTTP2_PREFACE).await;
}

#[tokio::test]
async fn a_promoted_connection_releases_its_seat_when_the_connection_ends() {
    let (address, shutdown, server) = spawn_server(promoting_router(), 1).await;

    let mut first = TcpStream::connect(address).await.unwrap();
    first.write_all(PROMOTE).await.unwrap();
    let first_response = read_to_close(&mut first).await;
    assert!(first_response.ends_with("promoted"), "{first_response}");

    let mut second = TcpStream::connect(address).await.unwrap();
    second.write_all(PROMOTE).await.unwrap();
    let second_response = timeout(SERVED_WAIT, read_to_close(&mut second))
        .await
        .expect("the seat stayed held by a handler after its connection ended");
    assert!(second_response.ends_with("promoted"), "{second_response}");

    shutdown.cancel();
    server.await.unwrap().unwrap();
}

#[test]
fn a_shed_is_reported_at_once_and_later_sheds_are_reported_as_one_count() {
    let start = Instant::now();
    let at = |seconds: u64| start + Duration::from_secs(seconds);
    let mut sheds = super::ShedLog::default();

    assert_eq!(sheds.record(at(0)), Some(1));
    assert_eq!(sheds.record(at(1)), None);
    assert_eq!(sheds.record(at(2)), None);
    assert_eq!(sheds.record(at(10)), Some(3));
    assert_eq!(sheds.flush(at(11)), None);
    assert_eq!(sheds.record(at(12)), None);
    assert_eq!(sheds.flush(at(20)), Some(1));
    assert_eq!(sheds.flush(at(30)), None);
}

#[test]
fn a_flush_within_ten_seconds_of_a_warning_keeps_the_count_for_the_next_due_report() {
    let start = Instant::now();
    let at = |milliseconds: u64| start + Duration::from_millis(milliseconds);
    let mut sheds = super::ShedLog::default();

    assert_eq!(sheds.record(at(0)), Some(1));
    assert_eq!(sheds.record(at(5)), None);
    assert_eq!(sheds.flush(at(10)), None);
    assert_eq!(sheds.record(at(9_990)), None);
    assert_eq!(sheds.flush(at(10_000)), Some(2));
    assert_eq!(sheds.flush(at(10_010)), None);
}
