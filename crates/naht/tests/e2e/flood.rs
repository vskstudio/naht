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

const QUEUED_NEWCOMER_BOUND: Duration = Duration::from_secs(20);

const SEATS: usize = 16;

const QUEUE: usize = 256;

const SLACK: Duration = Duration::from_secs(5);

const SEAT_DEADLINE: Duration = Duration::from_secs(2);

const QUEUE_WAIT: Duration = Duration::from_secs(20);

const REFUSAL_BOUND: Duration = Duration::from_secs(1);

const WAIT_CAP_LOWER: Duration = Duration::from_secs(18);

const WAIT_CAP_UPPER: Duration = Duration::from_secs(23);

const WAIT_CAP_POSITION: usize = 224;

const WAIT_CAP_GIVE_UP: Duration = Duration::from_secs(40);

async fn started(root: &tempfile::TempDir) -> Daemon {
    init(root.path());
    Daemon::start(root.path()).await
}

async fn hold_silent(address: &str, count: usize) -> Vec<TcpStream> {
    let mut held = Vec::with_capacity(count);
    for _ in 0..count {
        held.push(TcpStream::connect(address).await.unwrap());
    }
    held
}

fn write_report(line: &str) {
    if let Some(path) = env::var_os(REPORT_VARIABLE) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("the flood report could not be opened");
        file.write_all(line.as_bytes())
            .expect("the flood report could not be written");
    }
}

#[tokio::test]
async fn a_newcomer_is_refused_at_accept_once_the_queue_is_full() {
    let root = tempfile::tempdir().unwrap();
    let daemon = started(&root).await;
    let address_of_daemon = address(&daemon);
    let held = hold_silent(&address_of_daemon, SEATS + QUEUE + 28).await;
    sleep(SETTLE).await;

    let (refused_in, newcomer) = timed(timeout(
        QUEUE_WAIT + SLACK,
        attempt(
            &address_of_daemon,
            &authenticated_request(&daemon, "/heartbeat"),
        ),
    ))
    .await;
    drop(held);

    let mut report = String::new();
    writeln!(
        report,
        "queue_refusal_ms={} queue_refusal_served={}",
        refused_in.as_millis(),
        matches!(newcomer, Ok(Some(_))),
    )
    .unwrap();
    write_report(&report);

    assert!(
        matches!(newcomer, Ok(None)) && refused_in < REFUSAL_BOUND,
        "a newcomer behind a full queue was not refused at accept (answered after {refused_in:?})"
    );
}

#[tokio::test]
async fn a_newcomer_behind_silent_seats_is_served_when_a_seat_frees_at_the_deadline() {
    let root = tempfile::tempdir().unwrap();
    let daemon = started(&root).await;
    let address_of_daemon = address(&daemon);
    let held = hold_silent(&address_of_daemon, SEATS).await;
    sleep(SETTLE).await;

    let (waited, newcomer) = timed(timeout(
        QUEUE_WAIT,
        attempt(
            &address_of_daemon,
            &authenticated_request(&daemon, "/heartbeat"),
        ),
    ))
    .await;
    drop(held);

    let mut report = String::new();
    writeln!(
        report,
        "seat_deadline_ms={} seat_deadline_served={}",
        waited.as_millis(),
        matches!(newcomer, Ok(Some(_))),
    )
    .unwrap();
    write_report(&report);

    assert!(
        matches!(newcomer, Ok(Some(_))) && waited < SEAT_DEADLINE + SLACK,
        "a newcomer behind silent seats was not served at the {SEAT_DEADLINE:?} deadline (answered after {waited:?})"
    );
}

#[tokio::test]
async fn a_newcomer_queued_behind_silent_connections_is_cut_at_the_wait_cap() {
    let root = tempfile::tempdir().unwrap();
    let daemon = started(&root).await;
    let address_of_daemon = address(&daemon);
    let held = hold_silent(&address_of_daemon, SEATS + WAIT_CAP_POSITION).await;

    let (waited, newcomer) = timed(timeout(
        WAIT_CAP_GIVE_UP,
        attempt(
            &address_of_daemon,
            &authenticated_request(&daemon, "/heartbeat"),
        ),
    ))
    .await;
    drop(held);

    let mut report = String::new();
    writeln!(
        report,
        "wait_cap_ms={} wait_cap_served={}",
        waited.as_millis(),
        matches!(newcomer, Ok(Some(_))),
    )
    .unwrap();
    write_report(&report);

    assert!(
        matches!(newcomer, Ok(None)) && (WAIT_CAP_LOWER..WAIT_CAP_UPPER).contains(&waited),
        "a newcomer queued behind silent connections was not cut at the {QUEUE_WAIT:?} wait cap (answered after {waited:?})"
    );
}
