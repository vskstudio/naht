use naht::commands;
use naht::config::Config;
use naht_core::protocol::{self, Change, ChangeBatch, ServerInfo};
use reqwest::{Method, StatusCode};
use std::fs;

use super::support::{CLIENT_HEADER, Harness, PLACE_ID_HEADER, TOKEN_HEADER, decode};

const ROUTES: [(Method, &str); 8] = [
    (Method::GET, "/info"),
    (Method::GET, "/patches?cursor=0"),
    (Method::POST, "/changes"),
    (Method::GET, "/blobs?cursor=0"),
    (Method::POST, "/blobs"),
    (Method::POST, "/ack"),
    (Method::GET, "/heartbeat"),
    (Method::GET, "/activity?since=0"),
];

fn change_body(path: &str) -> Result<Vec<u8>, protocol::ProtocolError> {
    protocol::to_msgpack(&ChangeBatch {
        changes: vec![Change::Upsert {
            path: path.to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }],
    })
}

fn port_of(harness: &Harness) -> &str {
    harness.base.rsplit(':').next().unwrap_or_default()
}

async fn status_of(
    harness: &Harness,
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
) -> Result<StatusCode, reqwest::Error> {
    let mut request = reqwest::Client::new().request(method, harness.url(path));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    Ok(request.send().await?.status())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_without_the_client_header_are_refused_on_every_route() {
    let harness = Harness::start(None).await;
    for (method, path) in &ROUTES {
        let status = status_of(&harness, method.clone(), path, &[])
            .await
            .unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_without_the_client_header_carries_the_outdated_plugin_token() {
    let harness = Harness::start(None).await;
    let body = reqwest::Client::new()
        .get(harness.url("/info"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("outdated-plugin"), "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_origin_requests_are_refused_on_every_route() {
    let harness = Harness::start(None).await;
    for (method, path) in &ROUTES {
        let headers = [
            (CLIENT_HEADER, "plugin"),
            ("origin", "https://evil.example"),
        ];
        let status = status_of(&harness, method.clone(), path, &headers)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_foreign_host_is_refused_on_every_route() {
    let harness = Harness::start(None).await;
    let host = format!("evil.example:{}", port_of(&harness));
    for (method, path) in &ROUTES {
        let headers = [(CLIENT_HEADER, "plugin"), ("host", host.as_str())];
        let status = status_of(&harness, method.clone(), path, &headers)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_place_id_is_refused_on_every_route() {
    let harness = Harness::start(Some(999)).await;
    for (method, path) in &ROUTES {
        let headers = [(CLIENT_HEADER, "plugin"), (TOKEN_HEADER, harness.token())];
        let status = status_of(&harness, method.clone(), path, &headers)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cors_preflight_is_refused_without_an_allow_origin_header() {
    let harness = Harness::start(None).await;
    let response = reqwest::Client::new()
        .request(Method::OPTIONS, harness.url("/changes"))
        .header("origin", "https://evil.example")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", CLIENT_HEADER)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_origin_text_post_is_refused_and_writes_nothing() {
    let harness = Harness::start(None).await;
    let bare = reqwest::Client::new();

    let refused = bare
        .post(harness.url("/changes"))
        .header(CLIENT_HEADER, "plugin")
        .header("origin", "https://evil.example")
        .header("content-type", "text/plain")
        .body(change_body("Evil.luau").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert!(!harness.root.join("Evil.luau").exists());

    let accepted = bare
        .post(harness.url("/changes"))
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, harness.token())
        .body(change_body("Ok.luau").unwrap())
        .send()
        .await
        .unwrap();
    assert!(accepted.status().is_success());
    assert_eq!(harness.read("Ok.luau"), "return 1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_foreign_host_cannot_read_the_source_through_patches() {
    let harness = Harness::start(None).await;
    harness.write("Secret.luau", "return 'source'");
    let bare = reqwest::Client::new();

    let response = bare
        .get(harness.url("/patches?cursor=0"))
        .header(CLIENT_HEADER, "plugin")
        .header("host", "evil.example:34872")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response.bytes().await.unwrap();
    assert!(!String::from_utf8_lossy(&body).contains("return 'source'"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_loopback_host_names_are_accepted() {
    let harness = Harness::start(None).await;
    let port = port_of(&harness).to_owned();
    let bare = reqwest::Client::new();

    for host in [
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ] {
        let response = bare
            .get(harness.url("/heartbeat"))
            .header(CLIENT_HEADER, "plugin")
            .header(TOKEN_HEADER, harness.token())
            .header("host", host.as_str())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{host}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_without_a_host_header_is_refused() {
    let harness = Harness::start(None).await;
    let reply = harness
        .raw_reply("GET /heartbeat HTTP/1.1\r\nx-naht-client: plugin\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_with_two_host_headers_is_refused() {
    let harness = Harness::start(None).await;
    let request = format!(
        "GET /heartbeat HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nHost: evil.example\r\nx-naht-client: plugin\r\nConnection: close\r\n\r\n",
        port_of(&harness)
    );
    let reply = harness.raw_reply(&request).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_place_id_on_changes_is_refused_and_writes_nothing() {
    let harness = Harness::start(Some(999)).await;
    let bare = reqwest::Client::new();

    let wrong = bare
        .post(harness.url("/changes"))
        .header(CLIENT_HEADER, "plugin")
        .header(PLACE_ID_HEADER, "123")
        .header(TOKEN_HEADER, harness.token())
        .body(change_body("Wrong.luau").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);

    let missing = bare
        .post(harness.url("/changes"))
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, harness.token())
        .body(change_body("Missing.luau").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);

    assert!(!harness.root.join("Wrong.luau").exists());
    assert!(!harness.root.join("Missing.luau").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn naht_pull_passes_the_place_guard_of_a_guarded_daemon() {
    let harness = Harness::start(Some(999)).await;
    let mut config = Config::default();
    config.serve.port = Some(port_of(&harness).parse().unwrap());
    config.serve.place_id = Some(999);
    config.project.name = Some("demo".to_owned());
    let info: ServerInfo = decode(harness.info(Some(999)).await).await;
    fs::write(
        harness.root.join(".naht").join("daemon.port"),
        format!("{}\n{}\n", port_of(&harness), info.daemon_id),
    )
    .unwrap();
    commands::pull(&harness.root, &config).await.unwrap();

    config.serve.place_id = Some(123);
    assert!(commands::pull(&harness.root, &config).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gated_route_without_the_token_is_refused_with_401() {
    let harness = Harness::start(None).await;
    for (method, path) in ROUTES.iter().filter(|(_, path)| *path != "/info") {
        let response = reqwest::Client::new()
            .request(method.clone(), harness.url(path))
            .header(CLIENT_HEADER, "plugin")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
        let body: serde_json::Value =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(body["error"], "token required", "{method} {path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_token_is_refused_with_401_and_writes_nothing() {
    let harness = Harness::start(None).await;
    let response = reqwest::Client::new()
        .post(harness.url("/changes"))
        .header(CLIENT_HEADER, "plugin")
        .header(TOKEN_HEADER, "0".repeat(64))
        .body(change_body("Forged.luau").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(body["error"], "token rejected");
    assert!(!harness.root.join("Forged.luau").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_stays_open_and_reports_that_a_token_is_required() {
    let harness = Harness::start(None).await;
    let response = reqwest::Client::new()
        .get(harness.url("/info"))
        .header(CLIENT_HEADER, "plugin")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let info: ServerInfo = decode(response).await;
    assert!(info.token_required);
    assert_ne!(info.project_id, "");
}
