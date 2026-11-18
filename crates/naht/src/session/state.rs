use super::Session;
use super::{SessionError, new_daemon_id, new_session_id};
use crate::project_path::stays_inside_project;
use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo};
use naht_core::reconciler;
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::{DiskVfs, EntryKind, RootedVfs, Vfs, VfsError, remove_stale_temp_files};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Component, Path, PathBuf};

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
                ?error,
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
            source_roots: BTreeSet::new(),
            project_name: project_name.into(),
            id: new_session_id(),
            daemon_id: new_daemon_id()?,
            serve_place_id,
            terrain_sync,
        })
    }

    /// The id a new daemon session mints at start, which changes on every daemon restart.
    pub fn session_id(&self) -> &str {
        &self.id
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

    /// Protects configured source directories from the emptied-directory removal a batch may trigger.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::SourceRootOutsideRoot`] when a root has a `..`, a root or a prefix
    /// component. No root of that call is protected.
    pub fn protect_source_roots(
        &mut self,
        roots: impl IntoIterator<Item = PathBuf>,
    ) -> Result<(), SessionError> {
        let keys = roots
            .into_iter()
            .map(|root| {
                source_root_key(&root)
                    .ok_or_else(|| SessionError::SourceRootOutsideRoot { path: root })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.source_roots.extend(keys);
        Ok(())
    }

    pub(super) fn may_remove_emptied_dir(&self, dir: &Path) -> bool {
        dir.components().count() > 1 && self.may_remove_init_folder(dir)
    }

    pub(super) fn may_remove_init_folder(&self, dir: &Path) -> bool {
        source_root_key(dir).is_some_and(|key| !self.source_roots.contains(&key))
    }

    /// Whether `path` is currently a directory on disk, so a file batch cannot write over it.
    pub fn is_directory(&self, path: &str) -> bool {
        matches!(self.vfs.kind(Path::new(path)), Ok(EntryKind::Dir))
    }

    /// Whether every entry under `dir`, recursively, is named by `deleted`.
    pub fn subtree_deleted_by(&self, dir: &Path, deleted: &BTreeSet<&Path>) -> bool {
        let Ok(entries) = self.vfs.list(dir) else {
            return false;
        };
        entries.iter().all(|entry| {
            deleted.contains(entry.path.as_path())
                && (entry.kind != EntryKind::Dir || self.subtree_deleted_by(&entry.path, deleted))
        })
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

fn source_root_key(path: &Path) -> Option<String> {
    stays_inside_project(path).then(|| {
        path.components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
                Component::Prefix(_)
                | Component::RootDir
                | Component::CurDir
                | Component::ParentDir => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use naht_core::protocol::Change;
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

    #[test]
    fn a_configured_source_root_keeps_its_directory_when_its_last_file_is_deleted() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();
        session
            .protect_source_roots([PathBuf::from("packages/Shared")])
            .unwrap();
        session
            .apply_changes(vec![Change::Upsert {
                path: "packages/Shared/A.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 1".to_owned(),
            }])
            .unwrap();

        session
            .apply_changes(vec![Change::Delete {
                path: "packages/Shared/A.luau".to_owned(),
            }])
            .unwrap();

        assert!(root.path().join("packages/Shared").is_dir());
    }

    #[test]
    fn a_source_root_that_leaves_the_project_is_refused_rather_than_folded() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();

        for spelling in ["a/../src", "../src", "/abs/src"] {
            assert!(
                session
                    .protect_source_roots([PathBuf::from(spelling)])
                    .is_err(),
                "`{spelling}` was accepted as a source root"
            );
        }
        assert!(session.may_remove_init_folder(Path::new("src")));
        assert!(session.may_remove_init_folder(Path::new("a/src")));
    }

    #[test]
    fn a_refused_source_root_protects_none_of_the_roots_in_its_call() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();

        let refused = session
            .protect_source_roots([PathBuf::from("packages/Shared"), PathBuf::from("a/../src")]);

        assert!(refused.is_err());
        assert!(session.may_remove_init_folder(Path::new("packages/Shared")));
    }

    #[test]
    fn a_source_root_made_only_of_dots_is_the_project_root_and_protects_no_folder() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();

        session
            .protect_source_roots([PathBuf::from("."), PathBuf::from("./")])
            .unwrap();

        assert!(session.may_remove_init_folder(Path::new("src")));
        assert!(!session.may_remove_init_folder(Path::new("")));
        assert!(!session.may_remove_emptied_dir(Path::new("")));
    }

    #[test]
    fn a_source_root_configured_in_one_case_protects_the_folder_in_another() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();
        session
            .protect_source_roots([PathBuf::from("packages/Shared")])
            .unwrap();

        assert!(!session.may_remove_emptied_dir(Path::new("packages/shared")));
    }
}
