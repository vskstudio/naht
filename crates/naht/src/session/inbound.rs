use super::Session;
use super::transaction::{BlobsJournal, Journal, Journaled, Transaction};
use super::{MASS_DELETE_WARN, SessionError};
use naht_core::binary::{BlobInstance, BlobPatch};
use naht_core::mapper::{flat_init_path, init_flat_path, init_folder};
use naht_core::protocol::{Change, EventKind, Outcome};
use naht_core::reconciler::{self, Direction, Patch, PatchKind, TextInstance};
use naht_core::state::InstanceRecord;
use naht_core::vfs::Vfs;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn remove_emptied_dirs_in<V: Vfs>(vfs: &mut V, journal: &mut Option<Journal>, dirs: &[&Path]) {
    for dir in dirs {
        let mut journaled = Journaled {
            inner: &mut *vfs,
            journal: &mut *journal,
        };
        if let Err(error) = journaled.remove_if_emptied(dir) {
            tracing::warn!(target: "naht::session", path = ?dir, error = ?error, "kept a directory the batch emptied: it could not be removed");
        }
    }
}

fn is_case_only_rename(from: &str, to: &str) -> bool {
    from != to
        && from.to_lowercase() == to.to_lowercase()
        && Path::new(from).parent() == Path::new(to).parent()
}

impl<V: Vfs> Session<V> {
    /// Apply a batch of Studio-side edits to the mirror, then reconcile against the current
    /// filesystem. Filesystem writes (and conflict markers) are performed by the reconciler.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when a change cannot be applied.
    pub fn apply_changes(&mut self, changes: Vec<Change>) -> Result<(), SessionError> {
        let mut batch = Transaction::begin(self)?;
        let deleted = batch.stage_text_changes(changes)?;
        batch.reconcile_all()?;
        batch.remove_emptied_dirs(&deleted);
        batch.commit();
        Ok(())
    }

    pub(super) fn remove_emptied_dirs(&mut self, deleted: &[String]) {
        let session = &*self;
        let candidates: BTreeSet<&Path> = deleted
            .iter()
            .flat_map(|path| {
                let own_folder = init_folder(path).map(Path::new);
                Path::new(path).ancestors().filter(move |candidate| {
                    session.may_remove_emptied_dir(candidate)
                        || (Some(*candidate) == own_folder
                            && session.may_remove_init_folder(candidate))
                })
            })
            .collect();
        let mut dirs: Vec<&Path> = candidates.into_iter().collect();
        dirs.sort_by_key(|dir| Reverse(dir.components().count()));
        remove_emptied_dirs_in(&mut self.vfs, &mut self.journal, &dirs);
    }

    pub(super) fn stage_text_changes(
        &mut self,
        changes: Vec<Change>,
    ) -> Result<Vec<String>, SessionError> {
        let mut upserts = Vec::new();
        let mut removals = Vec::new();
        for change in changes {
            match change {
                Change::Upsert {
                    path,
                    class,
                    content,
                } => {
                    self.vfs.validate_path(&path)?;
                    upserts.push((path, class, content));
                }
                Change::Delete { path } => {
                    self.vfs.validate_path(&path)?;
                    removals.push(path);
                }
            }
        }
        let renamed = self.rename_case_only_pairs(&removals, &upserts)?;
        removals.retain(|path| !renamed.contains(path));
        let upserted: BTreeSet<String> = upserts.iter().map(|(path, _, _)| path.clone()).collect();
        let deleted: BTreeSet<String> = removals.iter().cloned().collect();
        let mut resolved_upserts = BTreeSet::new();
        for (path, class, content) in upserts {
            let path = self.batch_target(&path, &upserted, &deleted);
            reconciler::record_studio_side(&self.store, &path, &class, Some(&content))?;
            resolved_upserts.insert(path.clone());
            let instance = TextInstance::new(path.clone(), class, content);
            self.write_studio(path, Some(instance));
        }
        let mut emptied = Vec::with_capacity(removals.len());
        for path in removals {
            let path = self.batch_target(&path, &resolved_upserts, &deleted);
            if resolved_upserts.contains(&path) {
                continue;
            }
            reconciler::record_studio_deletion(&self.store, &path)?;
            self.write_studio(path.clone(), None);
            emptied.push(path);
        }
        Ok(emptied)
    }

    fn rename_case_only_pairs(
        &mut self,
        removals: &[String],
        upserts: &[(String, String, String)],
    ) -> Result<BTreeSet<String>, SessionError> {
        let mut renamed = BTreeSet::new();
        for old in removals {
            let target = upserts
                .iter()
                .map(|(path, _, _)| path.as_str())
                .find(|new| is_case_only_rename(old, new));
            if let Some(new) = target
                && self.rename_case_only(old, new)?
            {
                renamed.insert(old.clone());
            }
        }
        Ok(renamed)
    }

    fn rename_case_only(&mut self, from: &str, to: &str) -> Result<bool, SessionError> {
        let frozen = self
            .store
            .get(from)?
            .is_some_and(|record| record.conflicted);
        if frozen || !self.vfs.exists(Path::new(from)) || self.has_entry_named(Path::new(to))? {
            return Ok(false);
        }
        Journaled {
            inner: &mut self.vfs,
            journal: &mut self.journal,
        }
        .rename(Path::new(from), Path::new(to))?;
        reconciler::move_base(&self.store, from, to)?;
        self.write_studio(from.to_owned(), None);
        Ok(true)
    }

    fn has_entry_named(&self, path: &Path) -> Result<bool, SessionError> {
        let parent = path.parent().unwrap_or(Path::new(""));
        Ok(self
            .vfs
            .list(parent)?
            .iter()
            .any(|entry| entry.path.as_path() == path))
    }

    fn batch_target(
        &self,
        path: &str,
        upserted: &BTreeSet<String>,
        deleted: &BTreeSet<String>,
    ) -> String {
        let unclaimed =
            |candidate: &str| !upserted.contains(candidate) && !deleted.contains(candidate);
        if let (Some(flat), Some(folder)) = (init_flat_path(path), init_folder(path)) {
            let folder_prefix = format!("{folder}/");
            let folder_is_shared = upserted
                .iter()
                .any(|other| other.as_str() != path && other.starts_with(&folder_prefix));
            if self.vfs.exists(Path::new(&flat))
                && !self.vfs.exists(Path::new(folder))
                && !folder_is_shared
                && unclaimed(&flat)
            {
                return flat;
            }
        } else if let Some(init) = flat_init_path(path)
            && self.vfs.exists(Path::new(&init))
            && !self.vfs.exists(Path::new(path))
            && unclaimed(&init)
        {
            return init;
        }
        path.to_owned()
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
                tracing::warn!(target: "naht::sync", path = ?patch.path, "terrain blob conflict frozen");
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
                tracing::warn!(target: "naht::sync", path = ?patch.path, "conflict frozen");
            }
            match patch.direction {
                Direction::ToFs => {
                    self.mirror(&patch);
                    tracing::debug!(
                        target: "naht::sync",
                        path = ?patch.path,
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
mod tests;
