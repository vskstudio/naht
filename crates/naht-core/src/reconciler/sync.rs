use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::merge::{self, Merge};
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::{Vfs, VfsError};

use super::model::{Direction, Patch, PatchKind, ReconcileError, TextInstance, patch};
use super::persist::{advance_base, freeze};
use super::scan::decode;

/// Reconcile the filesystem and Studio sides against the persisted base.
///
/// Filesystem-bound effects (writing files, markers, deletions) are applied to `vfs`; Studio-bound
/// patches are returned for the daemon to forward. Both directions appear in the returned list so a
/// caller can see exactly what changed and where.
pub fn reconcile(
    vfs: &mut impl Vfs,
    store: &StateStore,
    fs: &BTreeMap<String, TextInstance>,
    studio: &BTreeMap<String, TextInstance>,
) -> Result<Vec<Patch>, ReconcileError> {
    let base: BTreeMap<String, InstanceRecord> = store
        .all()?
        .into_iter()
        .map(|record| (record.path.clone(), record))
        .collect();

    let mut keys: BTreeSet<&String> = BTreeSet::new();
    keys.extend(fs.keys());
    keys.extend(studio.keys());
    keys.extend(base.keys());

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
    patches: Vec<Patch>,
}

impl<V: Vfs> Pass<'_, V> {
    fn reconcile_one(
        &mut self,
        path: &str,
        fs: Option<&TextInstance>,
        studio: Option<&TextInstance>,
        base: Option<&InstanceRecord>,
    ) -> Result<(), ReconcileError> {
        if let Some(record) = base {
            if record.conflicted {
                return Ok(());
            }
            if record.base_content.is_none() {
                return Ok(());
            }
        }

        let base_text = match base {
            Some(record) => Some(decode(
                path,
                record.base_content.as_deref().unwrap_or_default(),
            )?),
            None => None,
        };
        let class = class_of(fs, studio, base);

        match (fs, studio) {
            (Some(fs), Some(studio)) => self.reconcile_both_present(
                path,
                &class,
                &fs.content,
                &studio.content,
                base_text.as_deref(),
            )?,
            (Some(fs), None) => {
                self.reconcile_one_side(path, &class, &fs.content, base_text.as_deref(), Side::Fs)?
            }
            (None, Some(studio)) => self.reconcile_one_side(
                path,
                &class,
                &studio.content,
                base_text.as_deref(),
                Side::Studio,
            )?,
            (None, None) => {
                if base.is_some() {
                    self.store.remove(path)?;
                }
            }
        }
        Ok(())
    }

    fn reconcile_both_present(
        &mut self,
        path: &str,
        class: &str,
        fs: &str,
        studio: &str,
        base: Option<&str>,
    ) -> Result<(), ReconcileError> {
        let kind_for_existing = if base.is_some() {
            PatchKind::Update
        } else {
            PatchKind::Create
        };

        if let Some(base) = base {
            let changed_fs = fs != base;
            let changed_studio = studio != base;
            match (changed_fs, changed_studio) {
                (false, false) => return Ok(()),
                (true, false) => {
                    self.patches.push(patch(
                        path,
                        class,
                        Direction::ToStudio,
                        PatchKind::Update,
                        Some(fs),
                    ));
                    advance_base(self.store, path, class, fs)?;
                    return Ok(());
                }
                (false, true) => {
                    self.vfs.write(Path::new(path), studio.as_bytes())?;
                    self.patches.push(patch(
                        path,
                        class,
                        Direction::ToFs,
                        PatchKind::Update,
                        Some(studio),
                    ));
                    advance_base(self.store, path, class, studio)?;
                    return Ok(());
                }
                (true, true) => {}
            }
        }

        match merge::merge3(base.unwrap_or_default(), fs, studio) {
            Merge::Clean(merged) => {
                if merged != fs {
                    self.vfs.write(Path::new(path), merged.as_bytes())?;
                    self.patches.push(patch(
                        path,
                        class,
                        Direction::ToFs,
                        kind_for_existing,
                        Some(&merged),
                    ));
                }
                if merged != studio {
                    self.patches.push(patch(
                        path,
                        class,
                        Direction::ToStudio,
                        kind_for_existing,
                        Some(&merged),
                    ));
                }
                advance_base(self.store, path, class, &merged)?;
            }
            Merge::Conflict(marked) => {
                self.vfs.write(Path::new(path), marked.as_bytes())?;
                freeze(self.store, path, class, base.unwrap_or_default())?;
                self.patches.push(patch(
                    path,
                    class,
                    Direction::ToFs,
                    PatchKind::Conflict,
                    Some(&marked),
                ));
            }
        }
        Ok(())
    }

    fn reconcile_one_side(
        &mut self,
        path: &str,
        class: &str,
        content: &str,
        base: Option<&str>,
        present: Side,
    ) -> Result<(), ReconcileError> {
        let (toward_missing, toward_present) = match present {
            Side::Fs => (Direction::ToStudio, Direction::ToFs),
            Side::Studio => (Direction::ToFs, Direction::ToStudio),
        };

        match base {
            None => {
                self.materialize(path, content, toward_missing)?;
                self.patches.push(patch(
                    path,
                    class,
                    toward_missing,
                    PatchKind::Create,
                    Some(content),
                ));
                advance_base(self.store, path, class, content)?;
            }
            Some(base) if content == base => {
                if matches!(present, Side::Fs) {
                    self.vfs.remove(Path::new(path))?;
                }
                self.patches
                    .push(patch(path, class, toward_present, PatchKind::Delete, None));
                self.store.remove(path)?;
            }
            Some(_) => {
                self.materialize(path, content, toward_missing)?;
                self.patches.push(patch(
                    path,
                    class,
                    toward_missing,
                    PatchKind::Create,
                    Some(content),
                ));
                advance_base(self.store, path, class, content)?;
            }
        }
        Ok(())
    }

    fn materialize(
        &mut self,
        path: &str,
        content: &str,
        toward: Direction,
    ) -> Result<(), VfsError> {
        if toward == Direction::ToFs {
            self.vfs.write(Path::new(path), content.as_bytes())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Side {
    Fs,
    Studio,
}

fn class_of(
    fs: Option<&TextInstance>,
    studio: Option<&TextInstance>,
    base: Option<&InstanceRecord>,
) -> String {
    fs.or(studio)
        .map(|current| current.class.clone())
        .or_else(|| base.map(|base_entry| base_entry.class.clone()))
        .unwrap_or_default()
}
