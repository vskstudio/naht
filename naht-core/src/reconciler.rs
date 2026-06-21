//! Diff the current sides against the persisted base and produce directional patches, resolving
//! concurrent edits with a 3-way merge.
//!
//! This is the brain's decision layer, still with no network: given the current filesystem side,
//! the current Studio side (supplied directly here; over the wire in Stage 4), and the persisted
//! base, it decides per instance what to push where, advances the base on a clean result, and — on
//! an unmergeable conflict — writes git-style markers, freezes the path, and loses nothing.
//!
//! Stage 3 operates on text instances keyed by path; the daemon maps stable GUIDs onto these in
//! later stages. Binary bases (no `base_content`) are skipped here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::hash::content_hash;
use crate::merge::{self, Merge};
use crate::state::{InstanceRecord, StateError, StateStore};
use crate::vfs::{Vfs, VfsError};

/// Which way a patch flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Filesystem → Studio (carried to the plugin on its next long-poll).
    ToStudio,
    /// Studio → filesystem (written to disk by the daemon).
    ToFs,
}

/// What a patch does to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchKind {
    /// The instance is new on the target side.
    Create,
    /// The instance's content changed.
    Update,
    /// The instance was removed.
    Delete,
    /// Markers were written to the filesystem and the path frozen.
    Conflict,
}

/// One reconciliation action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The instance's filesystem path (its stable key in Stage 3).
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// Which way the patch flows.
    pub direction: Direction,
    /// What the patch does.
    pub kind: PatchKind,
    /// The new content, or `None` for a delete.
    pub content: Option<String>,
}

/// A text instance on one side of the sync, keyed by its filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInstance {
    /// The instance's filesystem path.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// The current text content.
    pub content: String,
}

impl TextInstance {
    /// Convenience constructor.
    pub fn new(
        path: impl Into<String>,
        class: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            class: class.into(),
            content: content.into(),
        }
    }
}

/// The set of frozen (conflicted) paths awaiting resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Paths currently frozen pending `resolve`.
    pub conflicted: Vec<String>,
}

/// Errors from reconciliation.
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    /// The state store failed.
    #[error(transparent)]
    State(#[from] StateError),
    /// A filesystem operation failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// A side's content was not valid UTF-8.
    #[error("non-UTF-8 content at {0}")]
    NonUtf8(String),
    /// `resolve` was asked about a path that is not conflicted.
    #[error("no conflict to resolve at {0}")]
    NotConflicted(String),
    /// `resolve` was refused because the file still has conflict markers.
    #[error("conflict markers remain at {0}")]
    MarkersRemain(String),
}

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

    let mut patches = Vec::new();
    for key in keys {
        reconcile_one(
            vfs,
            store,
            key,
            fs.get(key),
            studio.get(key),
            base.get(key),
            &mut patches,
        )?;
    }
    Ok(patches)
}

fn reconcile_one(
    vfs: &mut impl Vfs,
    store: &StateStore,
    path: &str,
    fs: Option<&TextInstance>,
    studio: Option<&TextInstance>,
    base: Option<&InstanceRecord>,
    patches: &mut Vec<Patch>,
) -> Result<(), ReconcileError> {
    if let Some(record) = base {
        // A frozen path stays frozen until `resolve` clears it.
        if record.conflicted {
            return Ok(());
        }
        // A binary base has no text to merge; text reconcile leaves it alone.
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
        (Some(fs), Some(studio)) => reconcile_both_present(
            vfs,
            store,
            path,
            &class,
            &fs.content,
            &studio.content,
            base_text.as_deref(),
            patches,
        )?,
        (Some(fs), None) => {
            reconcile_one_side(
                vfs,
                store,
                path,
                &class,
                &fs.content,
                base_text.as_deref(),
                Side::Fs,
                patches,
            )?;
        }
        (None, Some(studio)) => {
            reconcile_one_side(
                vfs,
                store,
                path,
                &class,
                &studio.content,
                base_text.as_deref(),
                Side::Studio,
                patches,
            )?;
        }
        (None, None) => {
            // Gone from both sides: drop the base.
            if base.is_some() {
                store.remove(path)?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn reconcile_both_present(
    vfs: &mut impl Vfs,
    store: &StateStore,
    path: &str,
    class: &str,
    fs: &str,
    studio: &str,
    base: Option<&str>,
    patches: &mut Vec<Patch>,
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
                patches.push(patch(
                    path,
                    class,
                    Direction::ToStudio,
                    PatchKind::Update,
                    Some(fs),
                ));
                advance_base(store, path, class, fs)?;
                return Ok(());
            }
            (false, true) => {
                vfs.write(Path::new(path), studio.as_bytes())?;
                patches.push(patch(
                    path,
                    class,
                    Direction::ToFs,
                    PatchKind::Update,
                    Some(studio),
                ));
                advance_base(store, path, class, studio)?;
                return Ok(());
            }
            (true, true) => {} // fall through to a 3-way merge
        }
    }

    match merge::merge3(base.unwrap_or_default(), fs, studio) {
        Merge::Clean(merged) => {
            if merged != fs {
                vfs.write(Path::new(path), merged.as_bytes())?;
                patches.push(patch(
                    path,
                    class,
                    Direction::ToFs,
                    kind_for_existing,
                    Some(&merged),
                ));
            }
            if merged != studio {
                patches.push(patch(
                    path,
                    class,
                    Direction::ToStudio,
                    kind_for_existing,
                    Some(&merged),
                ));
            }
            advance_base(store, path, class, &merged)?;
        }
        Merge::Conflict(marked) => {
            vfs.write(Path::new(path), marked.as_bytes())?;
            freeze(store, path, class, base.unwrap_or_default())?;
            patches.push(patch(
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

#[derive(Clone, Copy)]
enum Side {
    Fs,
    Studio,
}

/// Handle an instance present on only one side: a creation, a deletion, or an edit racing a
/// deletion (where the edit wins — Naht never destroys an edit).
#[allow(clippy::too_many_arguments)]
fn reconcile_one_side(
    vfs: &mut impl Vfs,
    store: &StateStore,
    path: &str,
    class: &str,
    content: &str,
    base: Option<&str>,
    present: Side,
    patches: &mut Vec<Patch>,
) -> Result<(), ReconcileError> {
    // A create/edit flows toward the side missing the instance; a delete flows toward the side that
    // still has it (to remove it there too).
    let (toward_missing, toward_present) = match present {
        Side::Fs => (Direction::ToStudio, Direction::ToFs),
        Side::Studio => (Direction::ToFs, Direction::ToStudio),
    };

    match base {
        // New on the present side: create it on the other side.
        None => {
            materialize(vfs, path, content, toward_missing)?;
            patches.push(patch(
                path,
                class,
                toward_missing,
                PatchKind::Create,
                Some(content),
            ));
            advance_base(store, path, class, content)?;
        }
        Some(base) if content == base => {
            // The present side is unchanged and the other side deleted it: propagate the delete to
            // the present side. If that side is the filesystem, remove the file.
            if matches!(present, Side::Fs) {
                vfs.remove(Path::new(path))?;
            }
            patches.push(patch(path, class, toward_present, PatchKind::Delete, None));
            store.remove(path)?;
        }
        Some(_) => {
            // The present side edited while the other side deleted: the edit wins, so re-create it
            // on the deleting side rather than lose the work.
            materialize(vfs, path, content, toward_missing)?;
            patches.push(patch(
                path,
                class,
                toward_missing,
                PatchKind::Create,
                Some(content),
            ));
            advance_base(store, path, class, content)?;
        }
    }
    Ok(())
}

/// Apply a patch's filesystem effect, if it flows toward the filesystem.
fn materialize(
    vfs: &mut impl Vfs,
    path: &str,
    content: &str,
    toward: Direction,
) -> Result<(), VfsError> {
    if toward == Direction::ToFs {
        vfs.write(Path::new(path), content.as_bytes())?;
    }
    Ok(())
}

/// List the frozen (conflicted) paths.
pub fn status(store: &StateStore) -> Result<Status, ReconcileError> {
    Ok(Status {
        conflicted: store.conflicted()?.into_iter().map(|r| r.path).collect(),
    })
}

/// Clear a conflict once the user has removed the markers from the file.
///
/// Refuses while any marker remains; on success the resolved file content becomes the new base and
/// the path unfreezes.
pub fn resolve(vfs: &impl Vfs, store: &StateStore, path: &str) -> Result<(), ReconcileError> {
    let record = store
        .get(path)?
        .ok_or_else(|| ReconcileError::NotConflicted(path.to_string()))?;
    if !record.conflicted {
        return Err(ReconcileError::NotConflicted(path.to_string()));
    }
    let text = decode(path, &vfs.read(Path::new(&record.path))?)?;
    if merge::has_conflict_markers(&text) {
        return Err(ReconcileError::MarkersRemain(path.to_string()));
    }
    advance_base(store, &record.path, &record.class, &text)?;
    Ok(())
}

fn advance_base(
    store: &StateStore,
    path: &str,
    class: &str,
    content: &str,
) -> Result<(), ReconcileError> {
    store.upsert(&base_record(path, class, content, false))?;
    Ok(())
}

fn freeze(store: &StateStore, path: &str, class: &str, base: &str) -> Result<(), ReconcileError> {
    // Keep the original base content so a future resolve still has a merge ancestor.
    store.upsert(&base_record(path, class, base, true))?;
    Ok(())
}

fn base_record(path: &str, class: &str, content: &str, conflicted: bool) -> InstanceRecord {
    InstanceRecord {
        guid: path.to_string(),
        path: path.to_string(),
        class: class.to_string(),
        content_hash: content_hash(content.as_bytes()),
        base_content: Some(content.as_bytes().to_vec()),
        mtime: 0,
        conflicted,
    }
}

fn patch(
    path: &str,
    class: &str,
    direction: Direction,
    kind: PatchKind,
    content: Option<&str>,
) -> Patch {
    Patch {
        path: path.to_string(),
        class: class.to_string(),
        direction,
        kind,
        content: content.map(ToString::to_string),
    }
}

fn class_of(
    fs: Option<&TextInstance>,
    studio: Option<&TextInstance>,
    base: Option<&InstanceRecord>,
) -> String {
    fs.or(studio)
        .map(|t| t.class.clone())
        .or_else(|| base.map(|r| r.class.clone()))
        .unwrap_or_default()
}

fn decode(path: &str, bytes: &[u8]) -> Result<String, ReconcileError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| ReconcileError::NonUtf8(path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemoryVfs;

    const CLASS: &str = "ModuleScript";

    fn sides(items: &[(&str, &str)]) -> BTreeMap<String, TextInstance> {
        items
            .iter()
            .map(|(path, content)| (path.to_string(), TextInstance::new(*path, CLASS, *content)))
            .collect()
    }

    fn store_with_base(entries: &[(&str, &str)]) -> StateStore {
        let store = StateStore::open_in_memory().unwrap();
        for (path, content) in entries {
            advance_base(&store, path, CLASS, content).unwrap();
        }
        store
    }

    #[test]
    fn one_sided_fs_change_pushes_only_to_studio_and_advances_base() {
        let mut vfs = MemoryVfs::new().with_file("m", "v1");
        let store = store_with_base(&[("m", "v1")]);
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "v2")]),
            &sides(&[("m", "v1")]),
        )
        .unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].content.as_deref(), Some("v2"));
        assert_eq!(
            store.get("m").unwrap().unwrap().base_content,
            Some(b"v2".to_vec())
        );
    }

    #[test]
    fn one_sided_studio_change_writes_the_file() {
        let mut vfs = MemoryVfs::new().with_file("m", "v1");
        let store = store_with_base(&[("m", "v1")]);
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "v1")]),
            &sides(&[("m", "v2")]),
        )
        .unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToFs);
        assert_eq!(vfs.read(Path::new("m")).unwrap(), b"v2");
        assert_eq!(
            store.get("m").unwrap().unwrap().base_content,
            Some(b"v2".to_vec())
        );
    }

    #[test]
    fn no_change_produces_no_patches() {
        let mut vfs = MemoryVfs::new().with_file("m", "v1");
        let store = store_with_base(&[("m", "v1")]);
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "v1")]),
            &sides(&[("m", "v1")]),
        )
        .unwrap();
        assert!(patches.is_empty());
    }

    #[test]
    fn non_overlapping_both_changed_auto_merges() {
        let base = "local a = 1\nlocal b = 2\nlocal c = 3\n";
        let fs = "local a = 10\nlocal b = 2\nlocal c = 3\n";
        let studio = "local a = 1\nlocal b = 2\nlocal c = 30\n";
        let merged = "local a = 10\nlocal b = 2\nlocal c = 30\n";

        let mut vfs = MemoryVfs::new().with_file("m", base);
        let store = store_with_base(&[("m", base)]);
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", fs)]),
            &sides(&[("m", studio)]),
        )
        .unwrap();

        assert!(patches.iter().any(|p| p.direction == Direction::ToFs));
        assert!(patches.iter().any(|p| p.direction == Direction::ToStudio));
        assert!(patches.iter().all(|p| p.content.as_deref() == Some(merged)));
        assert_eq!(vfs.read(Path::new("m")).unwrap(), merged.as_bytes());
        assert_eq!(
            store.get("m").unwrap().unwrap().base_content,
            Some(merged.as_bytes().to_vec())
        );
    }

    #[test]
    fn overlapping_both_changed_freezes_with_markers_and_loses_nothing() {
        let base = "local a = 1\n";
        let mut vfs = MemoryVfs::new().with_file("m", base);
        let store = store_with_base(&[("m", base)]);
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "local a = 11\n")]),
            &sides(&[("m", "local a = 22\n")]),
        )
        .unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].kind, PatchKind::Conflict);

        let on_disk = String::from_utf8(vfs.read(Path::new("m")).unwrap()).unwrap();
        assert!(merge::has_conflict_markers(&on_disk));
        assert!(on_disk.contains("local a = 11"));
        assert!(on_disk.contains("local a = 22"));

        let record = store.get("m").unwrap().unwrap();
        assert!(record.conflicted);
        // The base is preserved (not advanced), so a later resolve still has its ancestor.
        assert_eq!(record.base_content, Some(base.as_bytes().to_vec()));
    }

    #[test]
    fn a_frozen_path_is_skipped_on_the_next_reconcile() {
        let base = "local a = 1\n";
        let mut vfs = MemoryVfs::new().with_file("m", base);
        let store = store_with_base(&[("m", base)]);
        reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "local a = 11\n")]),
            &sides(&[("m", "local a = 22\n")]),
        )
        .unwrap();

        // A second pass with fresh edits must not touch the frozen path.
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "local a = 33\n")]),
            &sides(&[("m", "local a = 44\n")]),
        )
        .unwrap();
        assert!(patches.is_empty());
        assert_eq!(status(&store).unwrap().conflicted, vec!["m".to_string()]);
    }

    #[test]
    fn resolve_refuses_while_markers_remain_then_succeeds() {
        let base = "local a = 1\n";
        let mut vfs = MemoryVfs::new().with_file("m", base);
        let store = store_with_base(&[("m", base)]);
        reconcile(
            &mut vfs,
            &store,
            &sides(&[("m", "local a = 11\n")]),
            &sides(&[("m", "local a = 22\n")]),
        )
        .unwrap();

        // Markers still present: resolve is refused.
        assert!(matches!(
            resolve(&vfs, &store, "m"),
            Err(ReconcileError::MarkersRemain(_))
        ));

        // User resolves the file by hand, then resolve succeeds and unfreezes.
        vfs.write(Path::new("m"), b"local a = 99\n").unwrap();
        resolve(&vfs, &store, "m").unwrap();
        assert!(status(&store).unwrap().conflicted.is_empty());
        assert_eq!(
            store.get("m").unwrap().unwrap().base_content,
            Some(b"local a = 99\n".to_vec())
        );
    }

    #[test]
    fn new_fs_file_creates_in_studio() {
        let mut vfs = MemoryVfs::new().with_file("n", "fresh");
        let store = StateStore::open_in_memory().unwrap();
        let patches = reconcile(
            &mut vfs,
            &store,
            &sides(&[("n", "fresh")]),
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].kind, PatchKind::Create);
        assert_eq!(
            store.get("n").unwrap().unwrap().base_content,
            Some(b"fresh".to_vec())
        );
    }

    #[test]
    fn deletion_on_one_side_propagates_when_the_other_is_unchanged() {
        let mut vfs = MemoryVfs::new().with_file("m", "v1");
        let store = store_with_base(&[("m", "v1")]);
        // FS deleted the file; Studio still has the unchanged content.
        let patches =
            reconcile(&mut vfs, &store, &BTreeMap::new(), &sides(&[("m", "v1")])).unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].kind, PatchKind::Delete);
        assert_eq!(store.get("m").unwrap(), None);
    }
}
