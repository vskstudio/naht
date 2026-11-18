use super::{InfoFailure, Ownership, accept_verdict, decode_info, fetch_info, ownership};
use crate::commands::error::CommandError;
use crate::daemon_port::Recorded;
use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo, to_msgpack};
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::sleep;

fn info(project: &str, place: Option<u64>, daemon_id: &str) -> ServerInfo {
    ServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "test".to_owned(),
        project_name: project.to_owned(),
        session_id: "test".to_owned(),
        daemon_id: daemon_id.to_owned(),
        serve_place_id: place,
        conflicted: Vec::new(),
        terrain_sync: false,
        limits: None,
    }
}

fn recorded(daemon_id: Option<&str>) -> Recorded {
    Recorded {
        port: 34872,
        daemon_id: daemon_id.map(str::to_owned),
    }
}

#[derive(serde::Serialize)]
struct OlderInfo {
    protocol_version: u32,
    server_version: String,
    project_name: String,
    session_id: String,
    serve_place_id: Option<u64>,
}

#[test]
fn a_daemon_recording_this_id_is_ours() {
    let verdict = ownership(
        &info("demo", None, "d-1"),
        "demo",
        None,
        Some(&recorded(Some("d-1"))),
    );
    assert_eq!(verdict, Ownership::Ours);
}

#[test]
fn a_same_named_daemon_with_another_id_is_stale() {
    let verdict = ownership(
        &info("game", None, "d-2"),
        "game",
        None,
        Some(&recorded(Some("d-1"))),
    );
    assert_eq!(
        verdict,
        Ownership::Stale {
            project: "game".to_owned()
        }
    );
}

#[test]
fn a_recorded_port_without_a_daemon_id_is_unverified() {
    let verdict = ownership(
        &info("demo", None, "d-1"),
        "demo",
        None,
        Some(&recorded(None)),
    );
    assert_eq!(verdict, Ownership::Unverified);
}

#[test]
fn a_missing_port_file_is_missing_not_unverified() {
    let verdict = ownership(&info("demo", None, "d-1"), "demo", None, None);
    assert_eq!(verdict, Ownership::Missing);
}

#[test]
fn a_missing_port_file_is_refused_as_missing_port_file() {
    let error = accept_verdict(Ownership::Missing, 34872).unwrap_err();
    assert!(matches!(
        error,
        CommandError::MissingPortFile { port: 34872 }
    ));
}

#[test]
fn a_different_project_name_is_foreign() {
    let verdict = ownership(
        &info("other", None, "d-1"),
        "demo",
        None,
        Some(&recorded(Some("d-1"))),
    );
    assert!(matches!(verdict, Ownership::Foreign { .. }));
}

#[test]
fn a_configured_place_must_match_the_daemon_place() {
    let matching = ownership(
        &info("demo", Some(1), "d-1"),
        "demo",
        Some(1),
        Some(&recorded(Some("d-1"))),
    );
    let other = ownership(
        &info("demo", Some(2), "d-1"),
        "demo",
        Some(1),
        Some(&recorded(Some("d-1"))),
    );
    assert_eq!(matching, Ownership::Ours);
    assert!(matches!(other, Ownership::Foreign { .. }));
}

#[test]
fn an_unconfigured_place_accepts_any_daemon_place() {
    let verdict = ownership(
        &info("demo", Some(2), "d-1"),
        "demo",
        None,
        Some(&recorded(Some("d-1"))),
    );
    assert_eq!(verdict, Ownership::Ours);
}

#[test]
fn a_current_info_payload_decodes() {
    let bytes = to_msgpack(&info("demo", None, "d-1")).unwrap();
    assert_eq!(decode_info(&bytes).ok(), Some(info("demo", None, "d-1")));
}

#[test]
fn an_info_payload_carries_the_frozen_paths_it_reports() {
    let frozen = ServerInfo {
        conflicted: vec!["Frozen.luau".to_owned()],
        ..info("demo", None, "d-1")
    };
    let bytes = to_msgpack(&frozen).unwrap();
    assert_eq!(decode_info(&bytes).ok(), Some(frozen));
}

#[test]
fn an_info_payload_without_a_daemon_id_is_from_an_older_naht() {
    let bytes = to_msgpack(&OlderInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_owned(),
        project_name: "demo".to_owned(),
        session_id: "s-1".to_owned(),
        serve_place_id: None,
    })
    .unwrap();
    assert!(matches!(decode_info(&bytes), Err(InfoFailure::OlderNaht)));
}

#[test]
fn an_answer_that_is_not_naht_info_is_not_naht() {
    let bytes = to_msgpack(&"not a daemon").unwrap();
    assert!(matches!(decode_info(&bytes), Err(InfoFailure::NotNaht)));
}

#[test]
fn an_answer_nested_past_the_depth_limit_is_not_naht() {
    let mut bytes = vec![0x81, 0xa1, b'x'];
    bytes.extend(vec![0x91; 100_000]);
    bytes.push(0x00);
    assert!(matches!(decode_info(&bytes), Err(InfoFailure::NotNaht)));
}

async fn serve_info_capturing_the_head(body: Vec<u8>) -> (u16, JoinHandle<String>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let head = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        drop(socket.write_all(response.as_bytes()).await);
        drop(socket.write_all(&body).await);
        String::from_utf8_lossy(&request[..read]).to_ascii_lowercase()
    });
    (port, head)
}

#[tokio::test]
async fn fetch_info_names_the_cli_and_the_configured_place() {
    let body = to_msgpack(&info("demo", Some(7), "d-1")).unwrap();
    let (port, head) = serve_info_capturing_the_head(body).await;
    assert!(fetch_info(port, Some(7)).await.is_ok());
    let request = head.await.unwrap();
    assert!(request.contains("x-naht-client: cli"), "{request}");
    assert!(request.contains("x-naht-place-id: 7"), "{request}");
}

#[tokio::test]
async fn fetch_info_sends_no_place_when_none_is_configured() {
    let body = to_msgpack(&info("demo", None, "d-1")).unwrap();
    let (port, head) = serve_info_capturing_the_head(body).await;
    assert!(fetch_info(port, None).await.is_ok());
    let request = head.await.unwrap();
    assert!(request.contains("x-naht-client: cli"), "{request}");
    assert!(!request.contains("x-naht-place-id"), "{request}");
}

#[derive(serde::Serialize)]
struct PaddedInfo {
    protocol_version: u32,
    server_version: String,
    project_name: String,
    session_id: String,
    daemon_id: String,
    serve_place_id: Option<u64>,
    padding: String,
}

async fn serve_body_once(body: Vec<u8>) -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        drop(socket.read(&mut request).await);
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        drop(socket.write_all(head.as_bytes()).await);
        drop(socket.write_all(&body).await);
    });
    port
}

#[tokio::test]
async fn an_info_body_past_the_cap_is_not_naht() {
    let body = to_msgpack(&PaddedInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "test".to_owned(),
        project_name: "demo".to_owned(),
        session_id: "s-1".to_owned(),
        daemon_id: "d-1".to_owned(),
        serve_place_id: None,
        padding: "x".repeat(128 * 1024),
    })
    .unwrap();
    let port = serve_body_once(body).await;
    assert!(matches!(
        fetch_info(port, None).await,
        Err(InfoFailure::NotNaht)
    ));
}

async fn serve_a_declared_body_that_stalls() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        drop(socket.read(&mut request).await);
        let head = "HTTP/1.1 200 OK\r\nContent-Length: 16777216\r\nConnection: close\r\n\r\n";
        drop(socket.write_all(head.as_bytes()).await);
        let filler = vec![b'x'; 128 * 1024];
        drop(socket.write_all(&filler).await);
        sleep(Duration::from_secs(30)).await;
    });
    port
}

#[tokio::test]
async fn an_info_body_past_the_cap_is_refused_without_waiting_for_the_rest() {
    let port = serve_a_declared_body_that_stalls().await;
    assert!(matches!(
        fetch_info(port, None).await,
        Err(InfoFailure::NotNaht)
    ));
}
