//! End-to-end daemon tests driven by a fake Studio HTTP client (the `fake-studio` pattern from
//! architecture §10). Each test starts a real `axum` server over a temp project directory and speaks
//! the MessagePack protocol to it, exercising the Stage 4 acceptance criteria.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use naht::server::AppState;
use naht::session::Session;
use naht_core::binary::BlobInstance;
use naht_core::protocol::{
    self, Ack, BlobChangeBatch, BlobPatchBatch, Change, ChangeBatch, PatchBatch, Pong, ServerInfo,
};
use naht_core::reconciler::{Direction, PatchKind};
use naht_core::state::StateStore;

/// A running daemon plus the fake client and the project directory it serves.
struct Harness {
    base: String,
    root: PathBuf,
    client: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn start(serve_place_id: Option<u64>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
        let session = Session::new(root.clone(), store, "demo", serve_place_id);
        // A short long-poll keeps "no new patches" requests from hanging the test.
        let state = AppState::new(session, Duration::from_millis(300));

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = naht::server::serve(listener, state).await;
        });

        Self {
            base: format!("http://{addr}"),
            root,
            client: reqwest::Client::new(),
            _dir: dir,
        }
    }

    fn write(&self, rel: &str, content: &str) {
        std::fs::write(self.root.join(rel), content).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }

    fn write_bytes(&self, rel: &str, content: &[u8]) {
        std::fs::write(self.root.join(rel), content).unwrap();
    }

    fn read_bytes(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.root.join(rel)).unwrap()
    }

    async fn blobs(&self, cursor: u64) -> BlobPatchBatch {
        let resp = self
            .client
            .get(format!("{}/blobs", self.base))
            .query(&[("cursor", cursor.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    async fn post_blobs(&self, changes: Vec<BlobInstance>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&BlobChangeBatch { changes }).unwrap();
        self.client
            .post(format!("{}/blobs", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn info(&self, place_id: Option<u64>) -> reqwest::Response {
        let mut req = self.client.get(format!("{}/info", self.base));
        if let Some(id) = place_id {
            req = req.query(&[("place_id", id.to_string())]);
        }
        req.send().await.unwrap()
    }

    async fn patches(&self, cursor: u64) -> PatchBatch {
        let resp = self
            .client
            .get(format!("{}/patches", self.base))
            .query(&[("cursor", cursor.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    async fn post_changes(&self, changes: Vec<Change>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&ChangeBatch { changes }).unwrap();
        self.client
            .post(format!("{}/changes", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn heartbeat(&self) -> Pong {
        let resp = self
            .client
            .get(format!("{}/heartbeat", self.base))
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    async fn ack(&self, paths: Vec<String>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&Ack { paths }).unwrap();
        self.client
            .post(format!("{}/ack", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    async fn ack_all(&self, batch: &PatchBatch) {
        let paths = batch.patches.iter().map(|p| p.path.clone()).collect();
        assert!(self.ack(paths).await.is_success());
    }
}

async fn decode<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> T {
    let bytes = resp.bytes().await.unwrap();
    protocol::from_msgpack(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fs_change_appears_via_patches() {
    let h = Harness::start(None).await;
    h.write("Greeter.luau", "return 1");

    // The reconnect re-diff on /info turns the new file into a queued FS → Studio patch.
    assert!(h.info(None).await.status().is_success());

    let batch = h.patches(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Greeter.luau");
    assert_eq!(patch.class, "ModuleScript");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.kind, PatchKind::Create);
    assert_eq!(patch.content.as_deref(), Some("return 1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_writes_the_file() {
    let h = Harness::start(None).await;

    let status = h
        .post_changes(vec![Change::Upsert {
            path: "New.luau".to_string(),
            class: "ModuleScript".to_string(),
            content: "return 5".to_string(),
        }])
        .await;

    assert!(status.is_success());
    assert_eq!(h.read("New.luau"), "return 5");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflicting_change_over_the_wire_writes_markers_and_freezes() {
    let h = Harness::start(None).await;

    // Establish a shared base of "local a = 1".
    h.write("m.luau", "local a = 1\n");
    h.info(None).await;
    let _drain = h.patches(0).await;

    // Both sides edit the same line before either reconcile lands: FS to 11, Studio to 22.
    h.write("m.luau", "local a = 11\n");
    let status = h
        .post_changes(vec![Change::Upsert {
            path: "m.luau".to_string(),
            class: "ModuleScript".to_string(),
            content: "local a = 22\n".to_string(),
        }])
        .await;
    assert!(status.is_success());

    let on_disk = h.read("m.luau");
    assert!(
        on_disk.contains("<<<<<<<"),
        "expected markers, got: {on_disk}"
    );
    assert!(on_disk.contains("local a = 11"));
    assert!(on_disk.contains("local a = 22"));

    // The path is frozen: a further edit produces no new patch until it is resolved.
    let cursor = h.patches(0).await.cursor;
    h.write("m.luau", "local a = 33\n");
    h.info(None).await;
    assert!(h.patches(cursor).await.patches.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_rediffs_without_clobbering() {
    let h = Harness::start(None).await;
    h.write("a.luau", "v1");

    // Initial sync, acked by the (fake) plugin so the base advances.
    h.info(None).await;
    let first = h.patches(0).await;
    assert_eq!(first.patches.len(), 1);
    h.ack_all(&first).await;

    // A reconnect re-diffs against the persisted base: nothing changed, so no spurious patch and the
    // file is untouched (no blind re-push).
    h.info(None).await;
    assert!(h.patches(first.cursor).await.patches.is_empty());
    assert_eq!(h.read("a.luau"), "v1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_half_acked_batch_re_emits_only_the_unacked_path() {
    let h = Harness::start(None).await;
    h.write("A.luau", "return 1");
    h.write("B.luau", "return 2");

    // Re-diff surfaces both new files as patches.
    h.info(None).await;
    let first = h.patches(0).await;
    assert_eq!(first.patches.len(), 2);

    // The plugin applies only A and acks it; B fails (no ack).
    assert!(h.ack(vec!["A.luau".to_string()]).await.is_success());

    // The next re-diff re-emits only B — A's base advanced on its ack, B's did not.
    h.info(None).await;
    let again = h.patches(first.cursor).await;
    let paths: Vec<_> = again.patches.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, vec!["B.luau"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handshake_enforces_the_serve_place_guard() {
    let h = Harness::start(Some(999)).await;

    assert_eq!(
        h.info(Some(123)).await.status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(h.info(None).await.status(), reqwest::StatusCode::FORBIDDEN);

    let ok = h.info(Some(999)).await;
    assert!(ok.status().is_success());
    let info: ServerInfo = decode(ok).await;
    assert_eq!(info.serve_place_id, Some(999));
    assert_eq!(info.project_name, "demo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heartbeat_reports_the_session_id() {
    let h = Harness::start(None).await;
    let info: ServerInfo = decode(h.info(None).await).await;
    assert_eq!(h.heartbeat().await.session_id, info.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_blob_syncs_both_ways_byte_for_byte() {
    let h = Harness::start(None).await;
    // Bytes that would corrupt under any text encoding: NUL, high bytes, the full 0..256 range.
    let blob: Vec<u8> = (0u16..400).map(|i| (i % 256) as u8).collect();

    // FS → Studio: a `.terrain` blob on disk surfaces on the blob channel with its exact bytes.
    h.write_bytes("Terrain.terrain", &blob);
    assert!(h.info(None).await.status().is_success());
    let batch = h.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Terrain.terrain");
    assert_eq!(patch.class, "Terrain");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.content.as_deref(), Some(blob.as_slice()));

    // Studio → FS: a blob POST writes a new `.terrain` file byte-for-byte.
    let other: Vec<u8> = (0u16..256).rev().map(|i| (i % 256) as u8).collect();
    let status = h
        .post_blobs(vec![BlobInstance::new(
            "World.terrain",
            "Terrain",
            other.clone(),
        )])
        .await;
    assert!(status.is_success());
    assert_eq!(h.read_bytes("World.terrain"), other);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_both_sides_blob_change_freezes_through_the_live_daemon() {
    let h = Harness::start(None).await;

    // Establish a shared base: a blob on disk, synced to Studio, both sides now agree on "base".
    h.write_bytes("t.terrain", b"base");
    h.info(None).await;
    let drained = h.blobs(0).await.cursor;

    // Both sides edit before either reconcile lands: FS to "fs-edit", Studio pushes "studio-edit".
    // The Studio push triggers the reconcile that sees both changed and freezes the path.
    h.write_bytes("t.terrain", b"fs-edit");
    let status = h
        .post_blobs(vec![BlobInstance::new(
            "t.terrain",
            "Terrain",
            b"studio-edit".to_vec(),
        )])
        .await;
    assert!(status.is_success());

    // Neither side was overwritten: the file on disk keeps the FS edit, and no blob patch flows.
    assert_eq!(h.read_bytes("t.terrain"), b"fs-edit");
    assert!(h.blobs(drained).await.patches.is_empty());

    // The path stays frozen: a further FS edit produces no new patch until it is resolved.
    h.write_bytes("t.terrain", b"fs-2");
    h.info(None).await;
    assert!(h.blobs(drained).await.patches.is_empty());
}
