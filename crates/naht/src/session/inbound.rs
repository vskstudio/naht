use super::Session;
use super::transaction::{BlobsJournal, Transaction};
use super::{MASS_DELETE_WARN, SessionError};
use naht_core::binary::{BlobInstance, BlobPatch};
use naht_core::protocol::{Change, EventKind, Outcome};
use naht_core::reconciler::{self, Direction, Patch, PatchKind, TextInstance};
use naht_core::state::InstanceRecord;
use std::collections::{BTreeMap, BTreeSet};

impl Session {
    /// Apply a batch of Studio-side edits to the mirror, then reconcile against the current
    /// filesystem. Filesystem writes (and conflict markers) are performed by the reconciler.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when a change cannot be applied.
    pub fn apply_changes(&mut self, changes: Vec<Change>) -> Result<(), SessionError> {
        let mut batch = Transaction::begin(self)?;
        batch.stage_text_changes(changes)?;
        batch.reconcile_all()?;
        batch.commit();
        Ok(())
    }

    pub(super) fn stage_text_changes(&mut self, changes: Vec<Change>) -> Result<(), SessionError> {
        for change in changes {
            match change {
                Change::Upsert {
                    path,
                    class,
                    content,
                } => {
                    self.vfs.validate_path(&path)?;
                    reconciler::record_studio_side(&self.store, &path, &class, Some(&content))?;
                    let instance = TextInstance::new(path.clone(), class, content);
                    self.write_studio(path, Some(instance));
                }
                Change::Delete { path } => {
                    self.vfs.validate_path(&path)?;
                    reconciler::record_studio_deletion(&self.store, &path)?;
                    self.write_studio(path, None);
                }
            }
        }
        Ok(())
    }

    fn write_studio(&mut self, path: String, instance: Option<TextInstance>) {
        let previous = match instance {
            Some(instance) => self.studio.insert(path.clone(), instance),
            None => self.studio.remove(&path),
        };
        if let Some(journal) = self.journal.as_mut() {
            journal.studio.push((path, previous));
        }
    }

    /// Apply a batch of Studio-side binary blobs (terrain) to the blob mirror, then reconcile against
    /// the current filesystem. Blob deletions aren't modeled on the wire, terrain isn't removed in
    /// practice, so a batch carries only the blobs Studio currently holds.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when a blob change cannot be applied.
    pub fn apply_blob_changes(&mut self, changes: Vec<BlobInstance>) -> Result<(), SessionError> {
        if !self.terrain_sync {
            return Ok(());
        }
        let mut batch = Transaction::begin(self)?;
        batch.stage_blob_changes(changes)?;
        batch.reconcile_blobs()?;
        batch.commit();
        Ok(())
    }

    fn stage_blob_changes(&mut self, changes: Vec<BlobInstance>) -> Result<(), SessionError> {
        let mut snapshot = BTreeMap::new();
        for change in changes {
            self.vfs.validate_path(&change.path)?;
            snapshot.insert(change.path.clone(), change);
        }
        let previous = self.studio_blobs.replace(snapshot);
        if let Some(journal) = self.journal.as_mut()
            && matches!(journal.blobs, BlobsJournal::Untouched)
        {
            journal.blobs = BlobsJournal::Before(previous);
        }
        Ok(())
    }

    pub(super) fn absorb_blobs(&mut self, patches: Vec<BlobPatch>) {
        for patch in patches {
            if patch.kind == PatchKind::Conflict {
                tracing::warn!(target: "naht::sync", path = %patch.path, "terrain blob conflict frozen");
                continue;
            }
            match patch.direction {
                Direction::ToFs => {
                    self.record_event(
                        patch.path.clone(),
                        Direction::ToFs,
                        EventKind::Blob,
                        Outcome::Ok,
                    );
                }
                Direction::ToStudio => {
                    if let Some(mirror) = self.studio_blobs.as_mut() {
                        match (&patch.kind, &patch.content) {
                            (PatchKind::Delete, _) => {
                                mirror.remove(&patch.path);
                            }
                            (_, Some(content)) => {
                                mirror.insert(
                                    patch.path.clone(),
                                    BlobInstance::new(
                                        patch.path.clone(),
                                        patch.class.clone(),
                                        content.clone(),
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                    self.enqueue_blob(patch);
                }
            }
        }
    }

    pub(super) fn absorb(
        &mut self,
        patches: Vec<Patch>,
        pre_base: &BTreeMap<String, InstanceRecord>,
    ) -> Result<(), SessionError> {
        let fs_bound: BTreeSet<String> = patches
            .iter()
            .filter(|patch| patch.direction == Direction::ToFs)
            .map(|patch| patch.path.clone())
            .collect();

        let deletions = patches
            .iter()
            .filter(|patch| patch.kind == PatchKind::Delete)
            .count();
        if deletions >= MASS_DELETE_WARN {
            tracing::warn!(
                target: "naht::sync",
                count = deletions,
                "large deletion reconciled, verify this was intended"
            );
        }

        for patch in patches {
            if patch.kind == PatchKind::Conflict {
                tracing::warn!(target: "naht::sync", path = %patch.path, "conflict frozen");
            }
            match patch.direction {
                Direction::ToFs => {
                    self.mirror(&patch);
                    tracing::debug!(
                        target: "naht::sync",
                        path = %patch.path,
                        kind = ?patch.kind,
                        "patch applied to filesystem"
                    );
                    let outcome = if patch.kind == PatchKind::Conflict {
                        Outcome::Conflict
                    } else {
                        Outcome::Ok
                    };
                    self.record_event(
                        patch.path.clone(),
                        Direction::ToFs,
                        EventKind::Text,
                        outcome,
                    );
                }
                Direction::ToStudio if fs_bound.contains(&patch.path) => {
                    self.set_unacked(&patch.path, None);
                    self.mirror(&patch);
                    self.enqueue(patch);
                }
                Direction::ToStudio => {
                    self.revert_base(&patch.path, pre_base)?;
                    self.set_unacked(&patch.path, Some(patch.clone()));
                    self.enqueue(patch);
                }
            }
        }
        Ok(())
    }

    fn revert_base(
        &mut self,
        path: &str,
        pre_base: &BTreeMap<String, InstanceRecord>,
    ) -> Result<(), SessionError> {
        match pre_base.get(path) {
            Some(record) => self.store.upsert(record)?,
            None => self.store.remove(path)?,
        }
        Ok(())
    }

    pub(super) fn mirror(&mut self, patch: &Patch) {
        match patch.kind {
            PatchKind::Delete => self.write_studio(patch.path.clone(), None),
            PatchKind::Conflict => {}
            PatchKind::Create | PatchKind::Update => {
                if let Some(content) = &patch.content {
                    let instance =
                        TextInstance::new(patch.path.clone(), patch.class.clone(), content.clone());
                    self.write_studio(patch.path.clone(), Some(instance));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use naht_core::binary::{BlobInstance, TERRAIN_CLASS};
    use naht_core::protocol::Change;
    use naht_core::state::{InstanceRecord, StateStore};

    use crate::session::{Session, SessionError};

    fn upsert(path: &str, content: &str) -> Change {
        Change::Upsert {
            path: path.to_owned(),
            class: "ModuleScript".to_owned(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn a_failed_batch_restores_the_entry_it_overwrote() {
        let root = tempfile::tempdir().unwrap();
        let mut session = Session::new(
            root.path(),
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            false,
        )
        .unwrap();
        session
            .apply_changes(vec![upsert("Good.luau", "A")])
            .unwrap();

        let outcome = session.apply_changes(vec![
            upsert("Good.luau", "B"),
            Change::Delete {
                path: "../escape.luau".to_owned(),
            },
        ]);

        assert!(outcome.is_err());
        assert_eq!(session.studio["Good.luau"].content, "A");
    }

    fn session_in(root: &Path) -> Session {
        Session::new(
            root,
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            false,
        )
        .unwrap()
    }

    fn base_records(session: &Session) -> BTreeMap<String, InstanceRecord> {
        session
            .store
            .all()
            .unwrap()
            .into_iter()
            .map(|record| (record.guid.clone(), record))
            .collect()
    }

    fn batch_ending_in_read_only_dir(
        session: &mut Session,
        root: &Path,
        changes: Vec<Change>,
    ) -> Result<(), SessionError> {
        let read_only = root.join("ro");
        fs::create_dir_all(&read_only).unwrap();
        fs::set_permissions(&read_only, fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = session.apply_changes(changes);
        fs::set_permissions(&read_only, fs::Permissions::from_mode(0o755)).unwrap();
        outcome
    }

    #[test]
    fn a_failed_batch_leaves_no_disk_write_behind() {
        let root = tempfile::tempdir().unwrap();
        let mut session = session_in(root.path());
        let before = base_records(&session);

        let outcome = batch_ending_in_read_only_dir(
            &mut session,
            root.path(),
            vec![upsert("Aok.luau", "X"), upsert("ro/B.luau", "Y")],
        );

        assert!(outcome.is_err());
        assert!(!root.path().join("Aok.luau").exists());
        assert!(!session.studio.contains_key("Aok.luau"));
        assert_eq!(base_records(&session), before);
        session.rescan().unwrap();
        assert!(!root.path().join("Aok.luau").exists());
        assert_eq!(base_records(&session), before);
    }

    #[test]
    fn a_failed_batch_restores_the_file_it_updated() {
        let root = tempfile::tempdir().unwrap();
        let mut session = session_in(root.path());
        session
            .apply_changes(vec![upsert("Aok.luau", "A")])
            .unwrap();
        let before = base_records(&session);

        let outcome = batch_ending_in_read_only_dir(
            &mut session,
            root.path(),
            vec![upsert("Aok.luau", "B"), upsert("ro/B.luau", "Y")],
        );

        assert!(outcome.is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("Aok.luau")).unwrap(),
            "A"
        );
        assert_eq!(session.studio["Aok.luau"].content, "A");
        assert_eq!(base_records(&session), before);
        session.rescan().unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("Aok.luau")).unwrap(),
            "A"
        );
        assert_eq!(base_records(&session), before);
    }

    fn blob(path: &str) -> BlobInstance {
        BlobInstance::new(path, TERRAIN_CLASS, vec![1_u8, 2, 3])
    }

    #[test]
    fn a_failed_blob_batch_leaves_no_blob_on_disk_or_in_the_mirror() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Blocker"), "a file").unwrap();
        let mut session = Session::new(
            root.path(),
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            true,
        )
        .unwrap();

        let outcome =
            session.apply_blob_changes(vec![blob("A_Good.terrain"), blob("Blocker/B_Bad.terrain")]);

        assert!(outcome.is_err());
        assert!(!root.path().join("A_Good.terrain").exists());
        assert!(
            session
                .studio_blobs
                .as_ref()
                .is_none_or(|blobs| !blobs.contains_key("A_Good.terrain"))
        );
        session.rescan().unwrap();
        assert!(!root.path().join("A_Good.terrain").exists());
    }

    #[test]
    fn a_failed_batch_leaves_the_activity_log_and_patch_queue_unchanged() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Blocker"), "a file").unwrap();
        fs::write(root.path().join("Other.luau"), "from disk").unwrap();
        let mut session = Session::new(
            root.path(),
            StateStore::open_in_memory().unwrap(),
            "demo",
            None,
            true,
        )
        .unwrap();
        session.studio_blobs = Some(BTreeMap::from([(
            "Blocker/B_Bad.terrain".to_owned(),
            blob("Blocker/B_Bad.terrain"),
        )]));
        let before = session.activity(0);
        let queued_before = session.take_patches(0).1.len();

        let outcome = session.apply_changes(vec![upsert("Aok.luau", "A")]);

        assert!(outcome.is_err());
        let after = session.activity(0);
        assert_eq!(after.events.len(), before.events.len());
        assert_eq!(after.pending, before.pending);
        assert_eq!(session.take_patches(0).1.len(), queued_before);
    }
}
