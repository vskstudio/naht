use super::Session;
use super::{SessionError, new_session_id, random_id};
use crate::config::Limits;
use crate::project_path::stays_inside_project;
use naht_core::protocol::{PROTOCOL_VERSION, ReportedLimits, ServerInfo};
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
        Self::assemble(
            RootedVfs::new(root, DiskVfs::new()),
            store,
            project_name,
            serve_place_id,
            terrain_sync,
        )
    }
}

impl<V: Vfs> Session<V> {
    pub(super) fn assemble(
        vfs: RootedVfs<V>,
        store: StateStore,
        project_name: impl Into<String>,
        serve_place_id: Option<u64>,
        terrain_sync: bool,
    ) -> Result<Self, SessionError> {
        let studio = reconciler::studio_at_base(&store)?;
        let recorded_place = store.place_id()?;
        let project_id = stored_project_id(&store)?;
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
            daemon_id: random_id()?,
            project_id,
            serve_place_id,
            recorded_place,
            terrain_sync,
            limits: Limits::default(),
        })
    }

    /// Set the limits this session enforces, as `[limits]` resolved at startup.
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// The limits this session enforces.
    pub fn limits(&self) -> Limits {
        self.limits
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
            project_id: self.project_id.clone(),
            token_required: true,
            session_id: self.id.clone(),
            daemon_id: self.daemon_id.clone(),
            serve_place_id: self.serve_place_id,
            conflicted: Vec::new(),
            terrain_sync: self.terrain_sync,
            limits: Some(ReportedLimits {
                terrain_voxels: self.limits.terrain_voxels,
            }),
            proof: None,
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

    /// The place id recorded in this project's state when the session opened, if one was bound.
    pub fn recorded_place_id(&self) -> Option<u64> {
        self.recorded_place
    }

    /// Bind this project's state to the Studio place that attaches first. A configured
    /// `[serve] place_id` governs instead, so the binding is then left alone.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::PlaceBoundElsewhere`] when the state is bound to another place, or a
    /// state error when the binding cannot be read or written.
    pub fn bind_place(&self, place_id: u64) -> Result<(), SessionError> {
        if self.serve_place_id.is_some() {
            return Ok(());
        }
        match self.store.place_id()? {
            None => self.store.set_place_id(place_id)?,
            Some(bound) if bound == place_id => {}
            Some(bound) => {
                return Err(SessionError::PlaceBoundElsewhere {
                    bound,
                    requested: place_id,
                });
            }
        }
        Ok(())
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

fn stored_project_id(store: &StateStore) -> Result<String, SessionError> {
    if let Some(project_id) = store.project_id()? {
        return Ok(project_id);
    }
    let project_id = random_id()?;
    store.set_project_id(&project_id)?;
    Ok(project_id)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use naht_core::protocol::Change;
    use naht_core::state::{InstanceRecord, StateStore};

    use crate::session::{Session, SessionError};

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
    fn info_reports_whether_terrain_sync_is_enabled() {
        let root = tempfile::tempdir().unwrap();
        let enabled = Session::new(
            root.path(),
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            true,
        )
        .unwrap();
        let disabled = Session::new(
            root.path(),
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            false,
        )
        .unwrap();
        assert!(enabled.info().terrain_sync);
        assert!(!disabled.info().terrain_sync);
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

    #[test]
    fn the_first_place_to_attach_binds_the_project_and_others_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let session = Session::new(root.path(), store, "demo", None, false).unwrap();

        session.bind_place(11).unwrap();
        session.bind_place(11).unwrap();

        assert!(matches!(
            session.bind_place(22),
            Err(SessionError::PlaceBoundElsewhere {
                bound: 11,
                requested: 22
            })
        ));
    }

    #[test]
    fn a_configured_place_id_leaves_the_binding_alone() {
        let root = tempfile::tempdir().unwrap();
        let store = StateStore::open_in_memory().unwrap();
        let session = Session::new(root.path(), store, "demo", Some(11), false).unwrap();

        session.bind_place(22).unwrap();

        assert_eq!(session.store.place_id().unwrap(), None);
    }
}
