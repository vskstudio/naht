use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use naht::server;
use naht::server::AppState;
use naht::session::Session;
use naht_core::binary::BlobInstance;
use naht_core::protocol::{
    self, BlobChangeBatch, BlobPatchBatch, Change, ChangeBatch, PatchBatch, Pong, SyncActivity,
};
use naht_core::state::StateStore;
use std::fs;
use std::io;
use std::iter::repeat_n;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time;

use crate::wire;
pub(crate) use crate::wire::decode;

pub(crate) const CLIENT_HEADER: &str = "x-naht-client";
pub(crate) const PLACE_ID_HEADER: &str = "x-naht-place-id";

pub(crate) struct Harness {
    pub base: String,
    pub root: PathBuf,
    pub client: reqwest::Client,
    pub state: Arc<AppState>,
    place_id: Option<u64>,
    _dir: tempfile::TempDir,
}

impl Harness {
    pub(crate) async fn start(serve_place_id: Option<u64>) -> Self {
        Self::start_with_terrain(serve_place_id, true).await
    }

    pub(crate) async fn start_with_terrain(
        serve_place_id: Option<u64>,
        terrain_sync: bool,
    ) -> Self {
        Self::start_configured(serve_place_id, terrain_sync, Duration::from_millis(300)).await
    }

    pub(crate) async fn start_configured(
        serve_place_id: Option<u64>,
        terrain_sync: bool,
        long_poll: Duration,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
        let session =
            Session::new(root.clone(), store, "demo", serve_place_id, terrain_sync).unwrap();
        let state = AppState::new(session, long_poll);

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(server::serve(listener, Arc::clone(&state)));

        Self {
            base: format!("http://{addr}"),
            root,
            client: reqwest::Client::new(),
            state,
            place_id: serve_place_id,
            _dir: dir,
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn authed(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let request = request.header(CLIENT_HEADER, "plugin");
        match self.place_id {
            Some(id) => request.header(PLACE_ID_HEADER, id.to_string()),
            None => request,
        }
    }

    pub(crate) fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        fs::write(self.root.join(rel), content).unwrap();
    }

    pub(crate) fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap()
    }

    pub(crate) fn read_bytes(&self, rel: &str) -> Vec<u8> {
        fs::read(self.root.join(rel)).unwrap()
    }

    async fn session_id(&self) -> String {
        self.heartbeat().await.session_id
    }

    pub(crate) async fn blobs(&self, cursor: u64) -> BlobPatchBatch {
        let session = self.session_id().await;
        let resp = self
            .authed(self.client.get(format!("{}/blobs", self.base)))
            .query(&[("cursor", cursor.to_string()), ("session", session)])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub(crate) async fn post_blobs(&self, changes: Vec<BlobInstance>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&BlobChangeBatch { changes }).unwrap();
        self.authed(self.client.post(format!("{}/blobs", self.base)))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    pub(crate) async fn info(&self, place_id: Option<u64>) -> reqwest::Response {
        wire::info(&self.client, &self.base, place_id).await
    }

    pub(crate) async fn raw_reply(&self, request: &str) -> io::Result<String> {
        let addr = self.base.trim_start_matches("http://");
        let mut stream = TcpStream::connect(addr).await?;
        stream.write_all(request.as_bytes()).await?;
        let mut reply = Vec::new();
        time::timeout(Duration::from_secs(5), stream.read_to_end(&mut reply))
            .await
            .map_err(io::Error::other)??;
        Ok(String::from_utf8_lossy(&reply).into_owned())
    }

    pub(crate) async fn patches(&self, cursor: u64) -> PatchBatch {
        let session = self.session_id().await;
        let resp = self
            .authed(self.client.get(format!("{}/patches", self.base)))
            .query(&[("cursor", cursor.to_string()), ("session", session)])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub(crate) async fn post_changes(&self, changes: Vec<Change>) -> reqwest::StatusCode {
        let body = protocol::to_msgpack(&ChangeBatch { changes }).unwrap();
        self.authed(self.client.post(format!("{}/changes", self.base)))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
    }

    pub(crate) async fn post_changes_nested(&self, depth: usize) -> reqwest::StatusCode {
        self.authed(self.client.post(format!("{}/changes", self.base)))
            .body(nested_map(depth))
            .send()
            .await
            .unwrap()
            .status()
    }

    pub(crate) async fn post_changes_refused(&self, changes: Vec<Change>) -> Vec<String> {
        let body = protocol::to_msgpack(&ChangeBatch { changes }).unwrap();
        let resp = self
            .authed(self.client.post(format!("{}/changes", self.base)))
            .body(body)
            .send()
            .await
            .unwrap();
        Self::refused_paths_of(resp).await
    }

    pub(crate) async fn post_blobs_refused(&self, changes: Vec<BlobInstance>) -> Vec<String> {
        let body = protocol::to_msgpack(&BlobChangeBatch { changes }).unwrap();
        let resp = self
            .authed(self.client.post(format!("{}/blobs", self.base)))
            .body(body)
            .send()
            .await
            .unwrap();
        Self::refused_paths_of(resp).await
    }

    async fn refused_paths_of(resp: reqwest::Response) -> Vec<String> {
        assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
        let refused: serde_json::Value =
            serde_json::from_slice(&resp.bytes().await.unwrap()).unwrap();
        serde_json::from_value(refused["refused_paths"].clone()).unwrap()
    }

    pub(crate) async fn heartbeat(&self) -> Pong {
        decode(wire::heartbeat(&self.client, &self.base).await.unwrap()).await
    }

    pub(crate) async fn activity(&self, since: u64) -> SyncActivity {
        let resp = self
            .authed(self.client.get(format!("{}/activity", self.base)))
            .query(&[("since", since.to_string())])
            .send()
            .await
            .unwrap();
        decode(resp).await
    }

    pub(crate) async fn ack(&self, seqs: Vec<u64>) -> reqwest::StatusCode {
        let session = self.session_id().await;
        wire::ack(&self.client, &self.base, &session, seqs).await
    }

    pub(crate) async fn ack_all(&self, batch: &PatchBatch) {
        let seqs = batch.patches.iter().map(|patch| patch.seq).collect();
        assert!(self.ack(seqs).await.is_success());
    }
}

fn nested_map(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x81, 0xa1, b'x'];
    bytes.extend(repeat_n(0x91, depth));
    bytes.push(0x00);
    bytes
}
