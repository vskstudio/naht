use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use naht::server::AppState;
use naht::session::Session;
use naht_core::binary::BlobInstance;
use naht_core::protocol::{
    self, Ack, BlobChangeBatch, BlobPatchBatch, Change, ChangeBatch, PatchBatch, Pong, SyncActivity,
};
use naht_core::state::StateStore;

pub struct Harness {
    pub base: String,
    pub root: PathBuf,
    pub client: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Harness {
    pub async fn start(serve_place_id: Option<u64>) -> Self {
        Self::start_with_terrain(serve_place_id, true).await
    }

    pub async fn start_with_terrain(serve_place_id: Option<u64>, terrain_sync: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
        let session = Session::new(root.clone(), store, "demo", serve_place_id, terrain_sync);
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

    pub fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        std::fs::write(self.root.join(rel), content).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }

    pub fn read_bytes(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.root.join(rel)).unwrap()
    }

    pub async fn blobs(&self, cursor: u64) -> BlobPatchBatch {
        let resp = self
            .client
            .get(format!("{}/blobs", self.base))
            .query(&[("cursor", cursor.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub async fn post_blobs(&self, changes: Vec<BlobInstance>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&BlobChangeBatch { changes }).unwrap();
        self.client
            .post(format!("{}/blobs", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    pub async fn info(&self, place_id: Option<u64>) -> reqwest::Response {
        let mut req = self.client.get(format!("{}/info", self.base));
        if let Some(id) = place_id {
            req = req.query(&[("place_id", id.to_string())]);
        }
        req.send().await.unwrap()
    }

    pub async fn patches(&self, cursor: u64) -> PatchBatch {
        let resp = self
            .client
            .get(format!("{}/patches", self.base))
            .query(&[("cursor", cursor.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub async fn post_changes(&self, changes: Vec<Change>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&ChangeBatch { changes }).unwrap();
        self.client
            .post(format!("{}/changes", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    pub async fn heartbeat(&self) -> Pong {
        let resp = self
            .client
            .get(format!("{}/heartbeat", self.base))
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub async fn activity(&self, since: u64) -> SyncActivity {
        let resp = self
            .client
            .get(format!("{}/activity", self.base))
            .query(&[("since", since.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub async fn ack(&self, paths: Vec<String>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&Ack { paths }).unwrap();
        self.client
            .post(format!("{}/ack", self.base))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    pub async fn ack_all(&self, batch: &PatchBatch) {
        let paths = batch.patches.iter().map(|p| p.path.clone()).collect();
        assert!(self.ack(paths).await.is_success());
    }
}

pub async fn decode<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> T {
    let bytes = resp.bytes().await.unwrap();
    protocol::from_msgpack(&bytes).unwrap()
}
