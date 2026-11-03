use std::collections::BTreeMap;
use std::path::Path;

use crate::hash::content_hash;
use crate::reconciler::{
    Direction, PatchKind, ReconcileError, Side, class_of, remove_fs_copy, union_keys, write_toward,
};
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::Vfs;

use super::model::{BlobInstance, BlobPatch, patch};

/// Reconcile the filesystem and Studio blob sides against the persisted (hash-only) base.
///
/// Filesystem writes and deletions are applied to `vfs`; Studio-bound patches are returned. A blob
/// changed on both sides freezes that path as a binary conflict: nothing is overwritten.
///
/// # Errors
///
/// Returns an error when scanning, the state store or a write fails.
pub fn reconcile_blobs(
    vfs: &mut impl Vfs,
    store: &StateStore,
    fs: &BTreeMap<String, BlobInstance>,
    studio: &BTreeMap<String, BlobInstance>,
) -> Result<Vec<BlobPatch>, ReconcileError> {
    let base: BTreeMap<String, InstanceRecord> = store
        .all()?
        .into_iter()
        .filter(|record| record.base_content.is_none())
        .map(|record| (record.path.clone(), record))
        .collect();

    let keys = union_keys(fs, studio, &base);

    let mut pass = Pass {
        vfs,
        store,
        patches: Vec::new(),
    };
    for key in keys {
        pass.reconcile_one(key, fs.get(key), studio.get(key), base.get(key))?;
    }
    Ok(pass.patches)
}

struct Pass<'a, V: Vfs> {
    vfs: &'a mut V,
    store: &'a StateStore,
    patches: Vec<BlobPatch>,
}

impl<V: Vfs> Pass<'_, V> {
    fn reconcile_one(
        &mut self,
        path: &str,
        fs: Option<&BlobInstance>,
        studio: Option<&BlobInstance>,
        base: Option<&InstanceRecord>,
    ) -> Result<(), ReconcileError> {
        if is_frozen(base) {
            return Ok(());
        }
        let base_hash = base.map(|record| record.content_hash.as_str());
        let class = class_of(fs, studio, base);

        match (fs, studio) {
            (Some(fs), Some(studio)) => {
                let fs_hash = content_hash(&fs.content);
                let studio_hash = content_hash(&studio.content);
                match base_hash {
                    Some(base_hash) => {
                        let changed_fs = fs_hash != base_hash;
                        let changed_studio = studio_hash != base_hash;
                        match (changed_fs, changed_studio) {
                            (false, false) => {}
                            (true, false) => {
                                self.patches.push(patch(
                                    path,
                                    &class,
                                    Direction::ToStudio,
                                    PatchKind::Update,
                                    Some(&fs.content),
                                ));
                                advance(self.store, path, &class, &fs.content)?;
                            }
                            (false, true) => {
                                self.vfs.write(Path::new(path), &studio.content)?;
                                self.patches.push(patch(
                                    path,
                                    &class,
                                    Direction::ToFs,
                                    PatchKind::Update,
                                    Some(&studio.content),
                                ));
                                advance(self.store, path, &class, &studio.content)?;
                            }
                            (true, true) => self.freeze(path, &class, base_hash)?,
                        }
                    }
                    None if fs_hash == studio_hash => {
                        advance(self.store, path, &class, &fs.content)?;
                    }
                    None => self.freeze(path, &class, &fs_hash)?,
                }
            }
            (Some(fs), None) => {
                self.reconcile_one_side(path, &class, &fs.content, base_hash, Side::Fs)?;
            }
            (None, Some(studio)) => {
                self.reconcile_one_side(path, &class, &studio.content, base_hash, Side::Studio)?;
            }
            (None, None) => {
                if base.is_some() {
                    self.store.remove(path)?;
                }
            }
        }
        Ok(())
    }

    fn reconcile_one_side(
        &mut self,
        path: &str,
        class: &str,
        content: &[u8],
        base_hash: Option<&str>,
        present: Side,
    ) -> Result<(), ReconcileError> {
        let (toward_missing, toward_present) = present.directions();
        match base_hash {
            Some(base_hash) if content_hash(content) == base_hash => {
                remove_fs_copy(self.vfs, path, present)?;
                self.patches
                    .push(patch(path, class, toward_present, PatchKind::Delete, None));
                self.store.remove(path)?;
            }
            None | Some(_) => {
                write_toward(self.vfs, path, content, toward_missing)?;
                self.patches.push(patch(
                    path,
                    class,
                    toward_missing,
                    PatchKind::Create,
                    Some(content),
                ));
                advance(self.store, path, class, content)?;
            }
        }
        Ok(())
    }

    fn freeze(&mut self, path: &str, class: &str, base_hash: &str) -> Result<(), ReconcileError> {
        self.store.upsert(&record(path, class, base_hash, true))?;
        self.patches.push(patch(
            path,
            class,
            Direction::ToFs,
            PatchKind::Conflict,
            None,
        ));
        Ok(())
    }
}

fn advance(
    store: &StateStore,
    path: &str,
    class: &str,
    content: &[u8],
) -> Result<(), ReconcileError> {
    store.upsert(&record(path, class, &content_hash(content), false))?;
    Ok(())
}

fn record(path: &str, class: &str, hash: &str, conflicted: bool) -> InstanceRecord {
    InstanceRecord {
        guid: path.to_owned(),
        path: path.to_owned(),
        class: class.to_owned(),
        content_hash: hash.to_owned(),
        base_content: None,
        mtime: 0,
        conflicted,
    }
}

fn is_frozen(base: Option<&InstanceRecord>) -> bool {
    base.is_some_and(|record| record.conflicted)
}
