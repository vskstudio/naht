use super::Session;
use super::{SessionError, new_daemon_id, new_session_id};
use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo};
use naht_core::reconciler;
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::{DiskVfs, EntryKind, RootedVfs, Vfs, VfsError, remove_stale_temp_files};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

impl Session {
    /// Open a session rooted at `root`, owning `store`. `serve_place_id` guards which Studio place
    /// may connect (`None` leaves it unguarded).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the persisted bases or the terrain blobs cannot be read.
    pub fn new(
        root: impl Into<PathBuf>,
        store: StateStore,
        project_name: impl Into<String>,
        serve_place_id: Option<u64>,
        terrain_sync: bool,
    ) -> Result<Self, SessionError> {
        let root = root.into();
        if let Err(error) = remove_stale_temp_files(&root) {
            tracing::warn!(
                target: "naht::session",
                path = %root.display(),
                %error,
                "could not remove stale temp files"
            );
        }
        let vfs = RootedVfs::new(root, DiskVfs::new());
        let studio = reconciler::studio_at_base(&store)?;
        Ok(Self {
            vfs,
            store,
            studio,
            journal: None,
            queue: BTreeMap::new(),
            unacked: BTreeMap::new(),
            next_seq: 0,
            studio_blobs: None,
            blob_queue: BTreeMap::new(),
            blob_next_seq: 0,
            activity: VecDeque::new(),
            activity_next_seq: 1,
            project_name: project_name.into(),
            id: new_session_id(),
            daemon_id: new_daemon_id()?,
            serve_place_id,
            terrain_sync,
        })
    }

    /// The identity payload used by heartbeats, without the conflicted list.
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: naht_core::version().to_owned(),
            project_name: self.project_name.clone(),
            session_id: self.id.clone(),
            daemon_id: self.daemon_id.clone(),
            serve_place_id: self.serve_place_id,
            conflicted: Vec::new(),
        }
    }

    /// The handshake payload for `GET /info`, carrying every path still frozen in the store.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the conflicted records cannot be read.
    pub fn handshake_info(&self) -> Result<ServerInfo, SessionError> {
        let mut info = self.info();
        info.conflicted = self
            .store
            .conflicted()?
            .into_iter()
            .map(|record| record.path)
            .collect();
        Ok(info)
    }

    /// The random id of this daemon run, recorded in the port file.
    pub fn daemon_id(&self) -> &str {
        &self.daemon_id
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

    /// Whether `path` is currently a directory on disk, so a file batch cannot write over it.
    pub fn is_directory(&self, path: &str) -> bool {
        matches!(self.vfs.kind(Path::new(path)), Ok(EntryKind::Dir))
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

#[cfg(test)]
mod tests {
    use naht_core::state::{InstanceRecord, StateStore};

    use crate::session::Session;

    fn record(path: &str, conflicted: bool) -> InstanceRecord {
        InstanceRecord {
            guid: format!("guid-{path}"),
            path: path.to_owned(),
            class: "ModuleScript".to_owned(),
            content_hash: "hash".to_owned(),
            base_content: Some(b"base".to_vec()),
            mtime: 0,
            conflicted,
        }
    }

    #[test]
    fn handshake_info_lists_the_frozen_paths_after_a_restart() {
        let store = StateStore::open_in_memory().unwrap();
        store.upsert(&record("Frozen.luau", true)).unwrap();
        store.upsert(&record("Clean.luau", false)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let session = Session::new(root.path(), store, "demo", None, false).unwrap();

        assert_eq!(
            session.handshake_info().unwrap().conflicted,
            vec!["Frozen.luau".to_owned()]
        );
        assert_eq!(session.info().conflicted, Vec::<String>::new());
    }
}
