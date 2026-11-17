use super::Session;
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
        for change in changes {
            match change {
                Change::Upsert {
                    path,
                    class,
                    content,
                } => {
                    self.vfs.validate_path(&path)?;
                    reconciler::record_studio_side(&self.store, &path, &class, Some(&content))?;
                    self.studio
                        .insert(path.clone(), TextInstance::new(path, class, content));
                }
                Change::Delete { path } => {
                    self.vfs.validate_path(&path)?;
                    reconciler::record_studio_deletion(&self.store, &path)?;
                    self.studio.remove(&path);
                }
            }
        }
        let pre_base = self.base_snapshot()?;
        self.reconcile_text(&pre_base)?;
        if self.terrain_sync {
            self.reconcile_blobs()?;
        }
        Ok(())
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
        let mut snapshot = BTreeMap::new();
        for change in changes {
            self.vfs.validate_path(&change.path)?;
            snapshot.insert(change.path.clone(), change);
        }
        self.studio_blobs = Some(snapshot);
        self.reconcile_blobs()
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
                    self.unacked.remove(&patch.path);
                    self.mirror(&patch);
                    self.enqueue(patch);
                }
                Direction::ToStudio => {
                    self.revert_base(&patch.path, pre_base)?;
                    self.unacked.insert(patch.path.clone(), patch.clone());
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
            PatchKind::Delete => {
                self.studio.remove(&patch.path);
            }
            PatchKind::Conflict => {}
            PatchKind::Create | PatchKind::Update => {
                if let Some(content) = &patch.content {
                    self.studio.insert(
                        patch.path.clone(),
                        TextInstance::new(patch.path.clone(), patch.class.clone(), content.clone()),
                    );
                }
            }
        }
    }
}
