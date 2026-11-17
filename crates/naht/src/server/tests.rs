use std::fs;
use std::path::PathBuf;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use naht_core::binary::{BlobInstance, TERRAIN_CLASS};
use naht_core::state::StateStore;
use tokio::io::{DuplexStream, duplex};
use tokio_util::io::ReaderStream;
use tower::ServiceExt;

use super::*;

const TEST_PORT: u16 = 34872;

fn test_state() -> Arc<AppState> {
    let store = StateStore::open_in_memory().unwrap();
    let session = Session::new(PathBuf::from("."), store, "demo", None, false).unwrap();
    AppState::new(session, Duration::from_secs(1))
}

fn fresh_state(post_wait: Duration) -> (tempfile::TempDir, Arc<AppState>) {
    let root = tempfile::tempdir().unwrap();
    let session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        false,
    )
    .unwrap();
    let state = AppState::with_post_wait(session, Duration::from_secs(1), post_wait);
    (root, state)
}

fn change_request(body: Body) -> Request<Body> {
    Request::post("/changes")
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(body)
        .unwrap()
}

fn upsert_batch(path: &str) -> Body {
    let batch = protocol::to_msgpack(&ChangeBatch {
        changes: vec![Change::Upsert {
            path: path.to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }],
    })
    .unwrap();
    Body::from(batch)
}

fn stuck_body() -> (Body, DuplexStream) {
    let (reader, keep_open) = duplex(64);
    (Body::from_stream(ReaderStream::new(reader)), keep_open)
}

async fn post_blobs_status(len: usize) -> StatusCode {
    let body = Body::from(vec![0xc1_u8; len]);
    let request = Request::post("/blobs")
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(body)
        .unwrap();
    router(test_state(), TEST_PORT)
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

async fn refused_paths_of(response: Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn a_text_batch_naming_a_path_that_is_now_a_directory_is_refused_400_naming_it() {
    let (root, state) = fresh_state(Duration::from_secs(1));
    fs::create_dir(root.path().join("Tree")).unwrap();

    let response = router(state, TEST_PORT)
        .oneshot(change_request(upsert_batch("Tree")))
        .await
        .unwrap();
    let (status, refused) = refused_paths_of(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused["refused_paths"], serde_json::json!(["Tree"]));
}

#[tokio::test]
async fn a_blob_batch_naming_a_path_that_is_now_a_directory_is_refused_400_naming_it() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Land.terrain")).unwrap();
    let session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        true,
    )
    .unwrap();
    let state = AppState::new(session, Duration::from_secs(1));
    let batch = protocol::to_msgpack(&BlobChangeBatch {
        changes: vec![BlobInstance::new(
            "Land.terrain",
            TERRAIN_CLASS,
            vec![1_u8, 2, 3],
        )],
    })
    .unwrap();
    let request = Request::post("/blobs")
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(Body::from(batch))
        .unwrap();

    let response = router(state, TEST_PORT).oneshot(request).await.unwrap();
    let (status, refused) = refused_paths_of(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refused["refused_paths"],
        serde_json::json!(["Land.terrain"])
    );
}

#[tokio::test]
async fn text_batches_name_every_refused_path_in_the_body() {
    use naht_core::protocol::{Change, ChangeBatch};

    let batch = ChangeBatch {
        changes: vec![
            Change::Upsert {
                path: "Good.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 1".to_owned(),
            },
            Change::Upsert {
                path: "Config: v2/Leaf.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 2".to_owned(),
            },
            Change::Delete {
                path: "a:b.luau".to_owned(),
            },
        ],
    };
    let body = protocol::to_msgpack(&batch).unwrap();
    let response = router(test_state(), TEST_PORT)
        .oneshot(change_request(Body::from(body)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let refused: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        refused["refused_paths"],
        serde_json::json!(["Config: v2/Leaf.luau", "a:b.luau"])
    );
}

fn nested_map(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x81, 0xa1, b'x'];
    bytes.extend(vec![0x91; depth]);
    bytes.push(0x00);
    bytes
}

async fn deep_body_status(path: &str) -> StatusCode {
    let request = Request::post(path)
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(Body::from(nested_map(100_000)))
        .unwrap();
    router(test_state(), TEST_PORT)
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn a_changes_body_nested_past_the_depth_limit_is_400() {
    assert_eq!(deep_body_status("/changes").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_blobs_body_nested_past_the_depth_limit_is_400() {
    assert_eq!(deep_body_status("/blobs").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_ack_body_nested_past_the_depth_limit_is_400() {
    assert_eq!(deep_body_status("/ack").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn blob_bodies_past_the_extractor_default_are_read() {
    assert_eq!(
        post_blobs_status(3 * 1024 * 1024).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn blob_bodies_over_the_limit_are_refused() {
    assert_eq!(
        post_blobs_status(BODY_LIMIT + 1).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn a_post_changes_slower_than_the_read_timeout_still_answers_200() {
    let (root, state) = fresh_state(Duration::from_secs(30));
    let held = state.session_lock().await;
    let app = router(Arc::clone(&state), TEST_PORT);
    let call = tokio::spawn(app.oneshot(change_request(upsert_batch("Slow.luau"))));

    time::sleep(Duration::from_secs(11)).await;
    drop(held);

    let status = call.await.unwrap().unwrap().status();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        fs::read_to_string(root.path().join("Slow.luau")).unwrap(),
        "return 1"
    );
}

#[tokio::test]
async fn a_post_beyond_the_concurrency_limit_is_shed_with_503() {
    let (_root, state) = fresh_state(Duration::from_secs(30));
    let app = router(state, TEST_PORT);
    let mut keep_open = Vec::new();
    let mut calls = Vec::new();
    for _ in 0..MAX_IN_FLIGHT {
        let (body, keep) = stuck_body();
        keep_open.push(keep);
        let app = app.clone();
        calls.push(tokio::spawn(app.oneshot(change_request(body))));
    }
    time::sleep(Duration::from_millis(200)).await;

    let shed = time::timeout(
        Duration::from_secs(2),
        app.clone()
            .oneshot(change_request(upsert_batch("Shed.luau"))),
    )
    .await
    .expect("the 65th POST is refused without waiting")
    .unwrap();

    assert_eq!(shed.status(), StatusCode::SERVICE_UNAVAILABLE);
    for call in &calls {
        call.abort();
    }
}

#[tokio::test]
async fn a_post_waiting_longer_than_the_bound_for_a_permit_gets_503_and_applies_nothing() {
    let (root, state) = fresh_state(Duration::from_millis(300));
    let app = router(state, TEST_PORT);
    let mut keep_open = Vec::new();
    let mut calls = Vec::new();
    for _ in 0..guard::POST_BODY_SLOTS {
        let (body, keep) = stuck_body();
        keep_open.push(keep);
        let app = app.clone();
        calls.push(tokio::spawn(app.oneshot(change_request(body))));
    }
    time::sleep(Duration::from_millis(200)).await;

    let waited = time::timeout(
        Duration::from_secs(5),
        app.clone()
            .oneshot(change_request(upsert_batch("Queued.luau"))),
    )
    .await
    .expect("the queued POST gives up at the bound")
    .unwrap();

    assert_eq!(waited.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!root.path().join("Queued.luau").exists());
    for call in &calls {
        call.abort();
    }
}

#[tokio::test]
async fn a_post_waiting_longer_than_the_bound_for_the_session_lock_gets_503_and_applies_nothing() {
    let (root, state) = fresh_state(Duration::from_millis(300));
    let held = state.session_lock().await;
    let app = router(Arc::clone(&state), TEST_PORT);

    let waited = time::timeout(
        Duration::from_secs(5),
        app.oneshot(change_request(upsert_batch("Locked.luau"))),
    )
    .await
    .expect("the locked POST gives up at the bound")
    .unwrap();
    drop(held);

    assert_eq!(waited.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!root.path().join("Locked.luau").exists());
}

#[tokio::test]
async fn a_batch_still_waiting_when_the_client_leaves_is_drained_before_shutdown_completes() {
    let (root, state) = fresh_state(Duration::from_secs(30));
    let held = state.session_lock().await;
    let app = router(Arc::clone(&state), TEST_PORT);
    let client = tokio::spawn(app.oneshot(change_request(upsert_batch("Drained.luau"))));
    time::sleep(Duration::from_millis(200)).await;

    client.abort();
    state.tasks().close();
    assert!(
        time::timeout(Duration::from_millis(300), state.tasks().wait())
            .await
            .is_err(),
        "shutdown drained before the batch finished"
    );

    drop(held);
    time::timeout(Duration::from_secs(5), state.tasks().wait())
        .await
        .expect("the batch finishes and shutdown drains it");
    assert_eq!(
        fs::read_to_string(root.path().join("Drained.luau")).unwrap(),
        "return 1"
    );
}

fn terrain_with_a_queued_blob() -> (tempfile::TempDir, Arc<AppState>) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    let mut session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        true,
    )
    .unwrap();
    session
        .apply_blob_changes(vec![BlobInstance::new(
            "src/Map.terrain",
            TERRAIN_CLASS,
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    fs::write(root.path().join("src").join("Map.terrain"), [4_u8, 5, 6]).unwrap();
    session.rescan().unwrap();
    session.rescan().unwrap();
    (root, AppState::new(session, Duration::from_secs(1)))
}

async fn get_status_and_body(state: &Arc<AppState>, uri: &str) -> (StatusCode, Bytes) {
    let request = Request::get(uri)
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(Body::empty())
        .unwrap();
    let response = router(Arc::clone(state), TEST_PORT)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        to_bytes(response.into_body(), usize::MAX).await.unwrap(),
    )
}

#[tokio::test]
async fn a_blob_poll_from_a_previous_session_is_refused_409_and_prunes_nothing() {
    let (_root, state) = terrain_with_a_queued_blob();

    let (rejected, _) = get_status_and_body(&state, "/blobs?cursor=5&session=previous").await;
    assert_eq!(rejected, StatusCode::CONFLICT);

    let current = state.session.lock().await.session_id().to_owned();
    let (status, body) =
        get_status_and_body(&state, &format!("/blobs?cursor=0&session={current}")).await;
    assert_eq!(status, StatusCode::OK);
    let batch: BlobPatchBatch = protocol::from_msgpack(&body).unwrap();
    assert_eq!(batch.patches.len(), 1);
}

#[tokio::test]
async fn a_text_poll_from_a_previous_session_is_refused_409() {
    let (_root, state) = terrain_with_a_queued_blob();

    let (status, _) = get_status_and_body(&state, "/patches?cursor=0&session=previous").await;

    assert_eq!(status, StatusCode::CONFLICT);
}

async fn post_ack_status(state: &Arc<AppState>, uri: &str, seqs: Vec<u64>) -> StatusCode {
    let body = protocol::to_msgpack(&Ack { seqs }).unwrap();
    let request = Request::post(uri)
        .header(guard::CLIENT_HEADER, "plugin")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(Body::from(body))
        .unwrap();
    router(Arc::clone(state), TEST_PORT)
        .oneshot(request)
        .await
        .unwrap()
        .status()
}

async fn state_with_a_queued_text_patch() -> (tempfile::TempDir, Arc<AppState>, u64) {
    let (root, state) = fresh_state(Duration::from_secs(1));
    fs::write(root.path().join("A.luau"), "return 1").unwrap();
    state.session.lock().await.rescan().unwrap();
    let seq = state.session.lock().await.take_patches(0).1[0].seq;
    (root, state, seq)
}

#[tokio::test]
async fn an_ack_from_a_previous_daemon_session_is_refused_409_and_settles_nothing() {
    let (_root, state, seq) = state_with_a_queued_text_patch().await;

    let status = post_ack_status(&state, "/ack?session=previous", vec![seq]).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let current = state.session.lock().await.session_id().to_owned();
    let (_, body) =
        get_status_and_body(&state, &format!("/patches?cursor=0&session={current}")).await;
    let batch: PatchBatch = protocol::from_msgpack(&body).unwrap();
    assert_eq!(
        batch.patches.len(),
        1,
        "a stale ack settled a patch the plugin never received"
    );
}

#[tokio::test]
async fn an_ack_naming_the_current_session_settles_the_patch() {
    let (_root, state, seq) = state_with_a_queued_text_patch().await;
    let current = state.session.lock().await.session_id().to_owned();

    let status = post_ack_status(&state, &format!("/ack?session={current}"), vec![seq]).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(state.session.lock().await.take_patches(0).1.len(), 0);
}

#[tokio::test]
async fn an_ack_without_a_session_is_refused_400_and_settles_nothing() {
    let (_root, state, seq) = state_with_a_queued_text_patch().await;

    let status = post_ack_status(&state, "/ack", vec![seq]).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(state.session.lock().await.take_patches(0).1.len(), 1);
}

#[tokio::test]
async fn a_text_poll_without_a_session_is_refused_400() {
    let (_root, state, _) = state_with_a_queued_text_patch().await;

    let (status, _) = get_status_and_body(&state, "/patches?cursor=0").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_blob_poll_without_a_session_is_refused_400() {
    let (_root, state) = terrain_with_a_queued_blob();

    let (status, _) = get_status_and_body(&state, "/blobs?cursor=0").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
