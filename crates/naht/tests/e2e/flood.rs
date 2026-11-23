use std::env;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
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

const RETRY_PAUSE: Duration = Duration::from_secs(1);

const REPORT_VARIABLE: &str = "NAHT_FLOOD_REPORT";

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

pub(super) async fn try_read_response(stream: &mut TcpStream) -> Option<(u16, Vec<u8>)> {
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

pub(super) async fn attempt(address: &str, request: &str) -> Option<TcpStream> {
    let mut stream = TcpStream::connect(address).await.ok()?;
    stream.write_all(request.as_bytes()).await.ok()?;
    let (status, _) = try_read_response(&mut stream).await?;
    (status == 200).then_some(stream)
}

pub(super) async fn served_within(
    address: &str,
    request: &str,
    within: Duration,
) -> (u32, Option<TcpStream>) {
    let mut attempts: u32 = 0;
    let served = timeout(within, async {
        loop {
            attempts = attempts.saturating_add(1);
            if let Some(stream) = attempt(address, request).await {
                return stream;
            }
            sleep(RETRY_PAUSE).await;
        }
    })
    .await;
    (attempts, served.ok())
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

async fn silent_flood(address: String, stop: Arc<AtomicBool>) {
    let mut buffer = [0_u8; 64];
    while !stop.load(Ordering::Relaxed) {
        let Ok(mut stream) = TcpStream::connect(&address).await else {
            continue;
        };
        while stream.read(&mut buffer).await.is_ok_and(|read| read > 0) {}
    }
}

pub(super) async fn a_newcomer_resolves_under_a_silent_flood(flooders: usize) {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    let daemon = Daemon::start(root.path()).await;
    let address_of_daemon = address(&daemon);
    let stop = Arc::new(AtomicBool::new(false));
    let floods: Vec<JoinHandle<()>> = (0..flooders)
        .map(|_| tokio::spawn(silent_flood(address_of_daemon.clone(), Arc::clone(&stop))))
        .collect();
    sleep(SETTLE).await;

    let authenticated = authenticated_request(&daemon, "/heartbeat");
    let (on_newcomer, newcomer) = timed(timeout(
        SILENT_NEWCOMER_BOUND,
        attempt(&address_of_daemon, &authenticated),
    ))
    .await;

    stop.store(true, Ordering::Relaxed);
    for flood in floods {
        flood.abort();
    }

    let served = matches!(newcomer, Ok(Some(_)));
    let mut report = String::new();
    writeln!(
        report,
        "silent_flooders={flooders} silent_newcomer_ms={} silent_newcomer_served={served} silent_newcomer_resolved={}",
        on_newcomer.as_millis(),
        newcomer.is_ok(),
    )
    .unwrap();
    if let Some(path) = env::var_os(REPORT_VARIABLE) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the flood report could not be opened");
        file.write_all(report.as_bytes())
            .expect("the flood report could not be written");
    }

    assert!(
        newcomer.is_ok(),
        "a new authenticated connection was neither served nor closed within {SILENT_NEWCOMER_BOUND:?} under {flooders} silent connections"
    );
}

#[tokio::test]
async fn a_newcomer_resolves_under_a_silent_flood_of_two_hundred_and_fifty_six_connections() {
    a_newcomer_resolves_under_a_silent_flood(256).await;
}

#[tokio::test]
async fn a_newcomer_resolves_under_a_silent_flood_of_three_hundred_connections() {
    a_newcomer_resolves_under_a_silent_flood(300).await;
}

pub(super) async fn timed<T: Future>(work: T) -> (Duration, T::Output) {
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
    let address_of_daemon = address(&daemon);
    let mut established = hold_authenticated(&daemon).await;
    let stop = Arc::new(AtomicBool::new(false));
    let floods: Vec<JoinHandle<()>> = (0..flooders)
        .map(|_| tokio::spawn(flood(address_of_daemon.clone(), Arc::clone(&stop))))
        .collect();
    sleep(SETTLE).await;

    let authenticated = authenticated_request(&daemon, "/heartbeat");
    let (on_established, established_status) =
        timed(request_on_open(&mut established, &authenticated)).await;
    let (on_newcomer, (newcomer_attempts, newcomer)) = timed(served_within(
        &address_of_daemon,
        &authenticated,
        newcomer_bound,
    ))
    .await;
    let (on_proof, (proof_attempts, proof)) = timed(served_within(
        &address_of_daemon,
        &tokenless_request(&daemon, "/info"),
        newcomer_bound,
    ))
    .await;

    stop.store(true, Ordering::Relaxed);
    for flood in floods {
        flood.abort();
    }

    let mut report = String::new();
    writeln!(
        report,
        "flooders={flooders} established_ms={} newcomer_ms={} newcomer_attempts={newcomer_attempts} newcomer_served={} proof_ms={} proof_attempts={proof_attempts} proof_served={}",
        on_established.as_millis(),
        on_newcomer.as_millis(),
        newcomer.is_some(),
        on_proof.as_millis(),
        proof.is_some(),
    )
    .unwrap();
    if let Some(path) = env::var_os(REPORT_VARIABLE) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the flood report could not be opened");
        file.write_all(report.as_bytes())
            .expect("the flood report could not be written");
    }

    assert_eq!(
        established_status, 200,
        "the established connection was refused"
    );
    assert!(
        on_established < FLOOD_READ_WAIT,
        "an established authenticated request took {on_established:?} under the flood"
    );
    assert!(
        newcomer.is_some(),
        "a new authenticated connection was not served within {newcomer_bound:?} under the flood ({newcomer_attempts} attempts)"
    );
    assert!(
        proof.is_some(),
        "the tokenless /info was not served within {newcomer_bound:?} under the flood ({proof_attempts} attempts)"
    );
}

#[tokio::test]
async fn an_authenticated_request_is_served_under_a_flood_of_sixty_four_tokenless_connections() {
    an_authenticated_request_is_served_under_a_tokenless_flood(64, NEWCOMER_BOUND).await;
}

#[tokio::test]
async fn an_authenticated_request_is_served_under_a_flood_of_two_hundred_tokenless_connections() {
    an_authenticated_request_is_served_under_a_tokenless_flood(200, QUEUED_NEWCOMER_BOUND).await;
}

const NEWCOMER_BOUND: Duration = Duration::from_secs(20);

const QUEUED_NEWCOMER_BOUND: Duration = Duration::from_secs(30);

const SILENT_NEWCOMER_BOUND: Duration = Duration::from_secs(35);
