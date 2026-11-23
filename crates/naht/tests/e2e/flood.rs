use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use naht_core::protocol::{CLIENT_HEADER, TOKEN_HEADER};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use super::{Daemon, init};

const FLOOD_READ_WAIT: Duration = Duration::from_secs(5);

const SETTLE: Duration = Duration::from_secs(1);

const REQUEST_CEILING: Duration = Duration::from_secs(60);

pub(super) fn address(daemon: &Daemon) -> String {
    daemon.base.trim_start_matches("http://").to_owned()
}

pub(super) fn authenticated_request(daemon: &Daemon, path: &str) -> String {
    let host = address(daemon);
    let token = &daemon.token;
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{CLIENT_HEADER}: plugin\r\n{TOKEN_HEADER}: {token}\r\n\r\n"
    )
}

pub(super) fn tokenless_request(daemon: &Daemon, path: &str) -> String {
    let host = address(daemon);
    format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{CLIENT_HEADER}: plugin\r\n\r\n")
}

pub(super) async fn read_response(stream: &mut TcpStream) -> (u16, Vec<u8>) {
    try_read_response(stream)
        .await
        .expect("the response ended early")
}

async fn try_read_response(stream: &mut TcpStream) -> Option<(u16, Vec<u8>)> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(stream.read_u8().await.ok()?);
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let status = text.split_whitespace().nth(1)?.parse::<u16>().ok()?;
    let length = text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.ok()?;
    Some((status, body))
}

pub(super) async fn hold_authenticated(daemon: &Daemon) -> TcpStream {
    let mut stream = TcpStream::connect(address(daemon)).await.unwrap();
    stream
        .write_all(authenticated_request(daemon, "/heartbeat").as_bytes())
        .await
        .unwrap();
    let (status, _) = read_response(&mut stream).await;
    assert_eq!(status, 200, "the authenticated connection was not served");
    stream
}

pub(super) async fn request_on_open(stream: &mut TcpStream, request: &str) -> u16 {
    stream.write_all(request.as_bytes()).await.unwrap();
    read_response(stream).await.0
}

async fn request_on_new_connection(address: &str, request: &str) -> u16 {
    let mut stream = TcpStream::connect(address).await.unwrap();
    request_on_open(&mut stream, request).await
}

async fn flood(address: String, stop: Arc<AtomicBool>) {
    let request =
        format!("GET /heartbeat HTTP/1.1\r\nHost: {address}\r\n{CLIENT_HEADER}: flood\r\n\r\n");
    while !stop.load(Ordering::Relaxed) {
        let Ok(mut stream) = TcpStream::connect(&address).await else {
            continue;
        };
        while !stop.load(Ordering::Relaxed) {
            if stream.write_all(request.as_bytes()).await.is_err() {
                break;
            }
            let answered = timeout(FLOOD_READ_WAIT, try_read_response(&mut stream)).await;
            if !matches!(answered, Ok(Some(_))) {
                break;
            }
        }
    }
}

async fn timed<T: Future>(work: T) -> (Duration, T::Output) {
    let started = Instant::now();
    let output = work.await;
    (started.elapsed(), output)
}

pub(super) async fn an_authenticated_request_is_served_under_a_tokenless_flood(
    flooders: usize,
    newcomer_bound: Duration,
) {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    let daemon = Daemon::start(root.path()).await;
    let mut established = hold_authenticated(&daemon).await;
    let stop = Arc::new(AtomicBool::new(false));
    let floods: Vec<JoinHandle<()>> = (0..flooders)
        .map(|_| tokio::spawn(flood(address(&daemon), Arc::clone(&stop))))
        .collect();
    sleep(SETTLE).await;

    let authenticated = authenticated_request(&daemon, "/heartbeat");
    let (on_established, established_status) =
        timed(request_on_open(&mut established, &authenticated)).await;

    let (on_new_connection, new_status) = timed(timeout(
        REQUEST_CEILING,
        request_on_new_connection(&address(&daemon), &authenticated),
    ))
    .await;

    let (on_proof, proof_status) = timed(timeout(
        REQUEST_CEILING,
        request_on_new_connection(&address(&daemon), &tokenless_request(&daemon, "/info")),
    ))
    .await;

    stop.store(true, Ordering::Relaxed);
    for flood in floods {
        flood.abort();
    }

    let new_status = new_status.unwrap_or(0);
    let proof_status = proof_status.unwrap_or(0);
    assert_eq!(
        established_status, 200,
        "the established connection was refused"
    );
    assert!(
        on_established < FLOOD_READ_WAIT,
        "an established authenticated request took {on_established:?} under the flood"
    );
    assert_eq!(
        new_status, 200,
        "a new authenticated connection was not served"
    );
    assert!(
        on_new_connection <= newcomer_bound,
        "a new authenticated connection took {on_new_connection:?} under the flood, past {newcomer_bound:?}"
    );
    assert_eq!(
        proof_status, 200,
        "the tokenless /info was not served under the flood"
    );
    assert!(
        on_proof <= newcomer_bound,
        "the tokenless /info took {on_proof:?} under the flood, past {newcomer_bound:?}"
    );
}

#[tokio::test]
async fn an_authenticated_request_is_served_under_a_flood_of_sixty_four_tokenless_connections() {
    an_authenticated_request_is_served_under_a_tokenless_flood(64, NEWCOMER_BOUND).await;
}

const NEWCOMER_BOUND: Duration = Duration::from_secs(20);
