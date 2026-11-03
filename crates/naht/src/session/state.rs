use super::Session;
use super::{SessionError, new_session_id};
use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo};
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::{DiskVfs, RootedVfs, VfsError};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

impl Session {
    /// Open a session rooted at `root`, owning `store`. `serve_place_id` guards which Studio place
    /// may connect (`None` leaves it unguarded).
    pub fn new(
        root: impl Into<PathBuf>,
        store: StateStore,
        project_name: impl Into<String>,
        serve_place_id: Option<u64>,
        terrain_sync: bool,
    ) -> Self {
        Self {
            vfs: RootedVfs::new(root.into(), DiskVfs::new()),
            store,
            studio: BTreeMap::new(),
            queue: BTreeMap::new(),
            unacked: BTreeMap::new(),
            next_seq: 0,
            studio_blobs: BTreeMap::new(),
            blob_queue: BTreeMap::new(),
            blob_next_seq: 0,
            activity: VecDeque::new(),
            activity_next_seq: 1,
            project_name: project_name.into(),
            id: new_session_id(),
            serve_place_id,
            terrain_sync,
        }
    }

    /// The handshake payload for `GET /info`.
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: naht_core::version().to_owned(),
            project_name: self.project_name.clone(),
            session_id: self.id.clone(),
            serve_place_id: self.serve_place_id,
        }
    }

    /// The place id this session is bound to, if any.
    pub fn serve_place_id(&self) -> Option<u64> {
        self.serve_place_id
    }

    /// Whether this session is allowed to synchronize terrain blobs.
    pub fn terrain_sync(&self) -> bool {
        self.terrain_sync
    }

    /// Validate a Studio-originated path against the project root before mutating session state.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when `path` escapes the root or cannot be checked.
    pub fn validate_path(&self, path: &str) -> Result<(), VfsError> {
        self.vfs.validate_path(path)
    }

    pub(super) fn base_snapshot(&self) -> Result<BTreeMap<String, InstanceRecord>, SessionError> {
        Ok(self
            .store
            .all()?
            .into_iter()
            .map(|record| (record.path.clone(), record))
            .collect())
    }
}
