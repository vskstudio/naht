use std::collections::BTreeMap;
use std::path::Path;

use crate::hash::content_hash;
use crate::reconciler::{
    Direction, PatchKind, ReconcileError, Side, class_of, remove_fs_copy, union_keys, write_toward,
};
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::Vfs;
use crate::walk::Unseen;

use super::model::{BlobInstance, BlobPatch, patch};
use super::persist::Base;

/// Reconciles blobs only once Studio has reported its complete blob snapshot.
///
/// # Errors
///
/// Returns an error when scanning, the state store or a write fails.
pub fn reconcile_blobs(
    vfs: &mut impl Vfs,
    store: &StateStore,
    fs: &BTreeMap<String, BlobInstance>,
    studio: Option<&BTreeMap<String, BlobInstance>>,
    unseen: &Unseen,
) -> Result<Vec<BlobPatch>, ReconcileError> {
    let Some(studio) = studio else {
        return Ok(Vec::new());
    };
    let mut base = BTreeMap::new();
    for record in store.all()? {
        if record.base_content.is_some() {
            continue;
        }
        if studio.contains_key(&record.path) || record.conflicted || unseen.contains(&record.path) {
            base.insert(record.path.clone(), record);
        } else {
            store.remove(&record.path)?;
        }
    }

    let keys = union_keys(fs, studio, &base);

    let mut pass = Pass {
        vfs,
        store,
        patches: Vec::new(),
    };
    for key in keys {
        if unseen.contains(key) {
            continue;
        }
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
        let base_state = base.map(Base::of);
        let class = class_of(fs, studio, base);

        match (fs, studio) {
            (Some(fs), Some(studio)) => {
                let fs_hash = content_hash(&fs.content);
                let studio_hash = content_hash(&studio.content);
                match &base_state {
                    Some(Base::KeepDisk) => {
                        if fs_hash != studio_hash {
                            self.patches.push(patch(
                                path,
                                &class,
                                Direction::ToStudio,
                                PatchKind::Update,
                                Some(&fs.content),
                            ));
                        }
                        advance(self.store, path, &class, &fs.content)?;
                    }
                    Some(Base::Synced(base_hash)) => {
                        let changed_fs = fs_hash != *base_hash;
                        let changed_studio = studio_hash != *base_hash;
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
                            (true, true) => self.freeze(path, &class, &studio_hash)?,
                        }
                    }
                    None if fs_hash == studio_hash => {
                        advance(self.store, path, &class, &fs.content)?;
                    }
                    None => self.freeze(path, &class, &studio_hash)?,
                }
            }
            (Some(fs), None) => {
                self.reconcile_one_side(path, &class, &fs.content, base_state.as_ref(), Side::Fs)?;
            }
            (None, Some(studio)) => {
                self.reconcile_one_side(
                    path,
                    &class,
                    &studio.content,
                    base_state.as_ref(),
                    Side::Studio,
                )?;
            }
            (None, None) => {}
        }
        Ok(())
    }

    fn reconcile_one_side(
        &mut self,
        path: &str,
        class: &str,
        content: &[u8],
        base: Option<&Base>,
        present: Side,
    ) -> Result<(), ReconcileError> {
        let (toward_missing, toward_present) = present.directions();
        match base {
            Some(Base::Synced(base_hash)) if content_hash(content) == *base_hash => {
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
