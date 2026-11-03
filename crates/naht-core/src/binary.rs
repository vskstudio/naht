//! Opaque binary-blob sync — the path that lifts terrain (and any file-level binary) from "deferred"
//! to a conflict-safe, byte-exact sync (architecture §9).
//!
//! Unlike scripts, a blob can't be diffed or merged: it is compared by hash and synced whole,
//! last-writer-wins, and a change on *both* sides since the base **freezes** the path instead of
//! silently overwriting either copy. The persisted base stores only the hash (no `base_content`, per
//! architecture §5's binary rule), so a restart still detects change without keeping the bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hash::content_hash;
use crate::reconciler::{Direction, PatchKind, ReconcileError};
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::{EntryKind, Vfs};

/// The filesystem extension a binary blob lives behind. A `*.terrain` file is the opaque voxel blob
/// the daemon syncs (architecture §9); the convention mirrors the script-suffix conventions in the
/// mapper, kept here because only the binary path consumes it.
pub const BLOB_EXTENSION: &str = "terrain";

/// The Roblox class a `*.terrain` blob maps to.
pub const TERRAIN_CLASS: &str = "Terrain";

/// One binary instance on a side of the sync, keyed by its filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobInstance {
    /// The blob's filesystem path (its stable key).
    pub path: String,
    /// The Roblox class name (e.g. `Terrain`).
    pub class: String,
    /// The raw bytes (encoded as a MessagePack binary blob, not an integer array).
    #[serde(with = "serde_bytes")]
    pub content: Vec<u8>,
}

impl BlobInstance {
    /// Convenience constructor.
    pub fn new(
        path: impl Into<String>,
        class: impl Into<String>,
        content: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            path: path.into(),
            class: class.into(),
            content: content.into(),
        }
    }
}

/// One binary reconciliation action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobPatch {
    /// The blob's filesystem path.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// Which way the patch flows.
    pub direction: Direction,
    /// What the patch does. A `Conflict` froze the path; no bytes were written.
    pub kind: PatchKind,
    /// The blob bytes, or `None` for a delete or a conflict freeze.
    #[serde(with = "serde_bytes")]
    pub content: Option<Vec<u8>>,
}

/// Walk `dir` in `vfs` and collect every binary blob file (`*.terrain`) into the flat, path-keyed
/// map the blob reconciler consumes — the binary counterpart of [`crate::reconciler::scan_text`].
/// Non-blob files are skipped, not lost; the text scan picks them up instead.
pub fn scan_blobs(
    vfs: &impl Vfs,
    dir: &Path,
) -> Result<BTreeMap<String, BlobInstance>, ReconcileError> {
    let mut out = BTreeMap::new();
    scan_blobs_into(vfs, dir, &mut out)?;
    Ok(out)
}

fn scan_blobs_into(
    vfs: &impl Vfs,
    dir: &Path,
    out: &mut BTreeMap<String, BlobInstance>,
) -> Result<(), ReconcileError> {
    for entry in vfs.list(dir)? {
        match entry.kind {
            EntryKind::Dir => scan_blobs_into(vfs, &entry.path, out)?,
            EntryKind::File => {
                let is_blob = entry
                    .path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == BLOB_EXTENSION);
                if !is_blob {
                    continue;
                }
                let key = entry.path.to_string_lossy().into_owned();
                let content = vfs.read(&entry.path)?;
                out.insert(key.clone(), BlobInstance::new(key, TERRAIN_CLASS, content));
            }
        }
    }
    Ok(())
}

/// Reconcile the filesystem and Studio blob sides against the persisted (hash-only) base.
///
/// Filesystem writes and deletions are applied to `vfs`; Studio-bound patches are returned. A blob
/// changed on both sides freezes that path as a binary conflict — nothing is overwritten.
pub fn reconcile_blobs(
    vfs: &mut impl Vfs,
    store: &StateStore,
    fs: &BTreeMap<String, BlobInstance>,
    studio: &BTreeMap<String, BlobInstance>,
) -> Result<Vec<BlobPatch>, ReconcileError> {
    // Only binary base records (hash-only) take part; text bases are left to the text reconcile.
    let base: BTreeMap<String, InstanceRecord> = store
        .all()?
        .into_iter()
        .filter(|record| record.base_content.is_none())
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
    fs: Option<&BlobInstance>,
    studio: Option<&BlobInstance>,
    base: Option<&InstanceRecord>,
    patches: &mut Vec<BlobPatch>,
) -> Result<(), ReconcileError> {
    if base.is_some_and(|record| record.conflicted) {
        return Ok(()); // frozen until resolved
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
                            patches.push(patch(
                                path,
                                &class,
                                Direction::ToStudio,
                                PatchKind::Update,
                                Some(&fs.content),
                            ));
                            advance(store, path, &class, &fs.content)?;
                        }
                        (false, true) => {
                            vfs.write(Path::new(path), &studio.content)?;
                            patches.push(patch(
                                path,
                                &class,
                                Direction::ToFs,
                                PatchKind::Update,
                                Some(&studio.content),
                            ));
                            advance(store, path, &class, &studio.content)?;
                        }
                        (true, true) => freeze(store, path, &class, base_hash, patches)?,
                    }
                }
                // New on both sides: adopt if identical, otherwise it's an unmergeable conflict.
                None if fs_hash == studio_hash => advance(store, path, &class, &fs.content)?,
                None => freeze(store, path, &class, &fs_hash, patches)?,
            }
        }
        (Some(fs), None) => reconcile_one_side(
            vfs,
            store,
            path,
            &class,
            &fs.content,
            base_hash,
            Side::Fs,
            patches,
        )?,
        (None, Some(studio)) => {
            reconcile_one_side(
                vfs,
                store,
                path,
                &class,
                &studio.content,
                base_hash,
                Side::Studio,
                patches,
            )?;
        }
        (None, None) => {
            if base.is_some() {
                store.remove(path)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Side {
    Fs,
    Studio,
}

// Eight parameters mirror the text reconciler's one-sided handler; threading them through a struct
// would obscure more than it clarifies.
#[allow(clippy::too_many_arguments)]
fn reconcile_one_side(
    vfs: &mut impl Vfs,
    store: &StateStore,
    path: &str,
    class: &str,
    content: &[u8],
    base_hash: Option<&str>,
    present: Side,
    patches: &mut Vec<BlobPatch>,
) -> Result<(), ReconcileError> {
    let (toward_missing, toward_present) = match present {
        Side::Fs => (Direction::ToStudio, Direction::ToFs),
        Side::Studio => (Direction::ToFs, Direction::ToStudio),
    };
    match base_hash {
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
            advance(store, path, class, content)?;
        }
        // Unchanged on the present side, deleted on the other: propagate the delete.
        Some(base_hash) if content_hash(content) == base_hash => {
            if matches!(present, Side::Fs) {
                vfs.remove(Path::new(path))?;
            }
            patches.push(patch(path, class, toward_present, PatchKind::Delete, None));
            store.remove(path)?;
        }
        // Edited on the present side while deleted on the other: the edit wins, re-create it.
        Some(_) => {
            materialize(vfs, path, content, toward_missing)?;
            patches.push(patch(
                path,
                class,
                toward_missing,
                PatchKind::Create,
                Some(content),
            ));
            advance(store, path, class, content)?;
        }
    }
    Ok(())
}

fn materialize(
    vfs: &mut impl Vfs,
    path: &str,
    content: &[u8],
    toward: Direction,
) -> Result<(), ReconcileError> {
    if toward == Direction::ToFs {
        vfs.write(Path::new(path), content)?;
    }
    Ok(())
}

fn freeze(
    store: &StateStore,
    path: &str,
    class: &str,
    base_hash: &str,
    patches: &mut Vec<BlobPatch>,
) -> Result<(), ReconcileError> {
    store.upsert(&record(path, class, base_hash, true))?;
    patches.push(patch(
        path,
        class,
        Direction::ToFs,
        PatchKind::Conflict,
        None,
    ));
    Ok(())
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
        guid: path.to_string(),
        path: path.to_string(),
        class: class.to_string(),
        content_hash: hash.to_string(),
        base_content: None, // binary: hash only
        mtime: 0,
        conflicted,
    }
}

fn patch(
    path: &str,
    class: &str,
    direction: Direction,
    kind: PatchKind,
    content: Option<&[u8]>,
) -> BlobPatch {
    BlobPatch {
        path: path.to_string(),
        class: class.to_string(),
        direction,
        kind,
        content: content.map(<[u8]>::to_vec),
    }
}

fn class_of(
    fs: Option<&BlobInstance>,
    studio: Option<&BlobInstance>,
    base: Option<&InstanceRecord>,
) -> String {
    fs.or(studio)
        .map(|b| b.class.clone())
        .or_else(|| base.map(|r| r.class.clone()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemoryVfs;

    const CLASS: &str = "Terrain";

    fn blobs(items: &[(&str, &[u8])]) -> BTreeMap<String, BlobInstance> {
        items
            .iter()
            .map(|(path, content)| {
                (
                    path.to_string(),
                    BlobInstance::new(*path, CLASS, content.to_vec()),
                )
            })
            .collect()
    }

    fn base_with(store: &StateStore, path: &str, content: &[u8]) {
        advance(store, path, CLASS, content).unwrap();
    }

    #[test]
    fn scan_blobs_collects_only_terrain_files_with_exact_bytes() {
        let blob: Vec<u8> = (0u16..300).map(|i| (i % 256) as u8).collect();
        let vfs = MemoryVfs::new()
            .with_file("Workspace/Terrain.terrain", blob.clone())
            .with_file("Workspace/Main.luau", b"return 1".to_vec())
            .with_file("README.md", b"not an instance".to_vec());

        let found = scan_blobs(&vfs, Path::new("")).unwrap();

        // Only the `.terrain` blob is picked up; the script and the README are left to others.
        let keys: Vec<&String> = found.keys().collect();
        assert_eq!(keys, vec!["Workspace/Terrain.terrain"]);
        let instance = &found["Workspace/Terrain.terrain"];
        assert_eq!(instance.class, TERRAIN_CLASS);
        assert_eq!(instance.content, blob);
    }

    #[test]
    fn a_blob_round_trips_each_way_without_byte_loss() {
        // Random-ish bytes, including a NUL and high bytes, to catch any text assumption.
        let blob: Vec<u8> = (0u16..512).map(|i| (i % 256) as u8).collect();

        // FS -> Studio: a new blob is pushed with its exact bytes.
        let mut vfs = MemoryVfs::new();
        let store = StateStore::open_in_memory().unwrap();
        let patches = reconcile_blobs(
            &mut vfs,
            &store,
            &blobs(&[("Terrain.terrain", &blob)]),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].content.as_deref(), Some(blob.as_slice()));

        // Studio -> FS: the blob is written to disk byte-for-byte.
        let mut vfs = MemoryVfs::new();
        let store = StateStore::open_in_memory().unwrap();
        reconcile_blobs(
            &mut vfs,
            &store,
            &BTreeMap::new(),
            &blobs(&[("Terrain.terrain", &blob)]),
        )
        .unwrap();
        assert_eq!(vfs.read(Path::new("Terrain.terrain")).unwrap(), blob);
    }

    #[test]
    fn a_one_sided_change_syncs_and_advances_the_base() {
        let mut vfs = MemoryVfs::new().with_file("t.terrain", b"v1".to_vec());
        let store = StateStore::open_in_memory().unwrap();
        base_with(&store, "t.terrain", b"v1");

        let patches = reconcile_blobs(
            &mut vfs,
            &store,
            &blobs(&[("t.terrain", b"v2")]),
            &blobs(&[("t.terrain", b"v1")]),
        )
        .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].content.as_deref(), Some(b"v2".as_slice()));
        assert_eq!(
            store.get("t.terrain").unwrap().unwrap().content_hash,
            content_hash(b"v2")
        );
    }

    #[test]
    fn a_change_on_both_sides_freezes_a_binary_conflict_without_overwriting() {
        let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
        let store = StateStore::open_in_memory().unwrap();
        base_with(&store, "t.terrain", b"base");

        let patches = reconcile_blobs(
            &mut vfs,
            &store,
            &blobs(&[("t.terrain", b"fs-edit")]),
            &blobs(&[("t.terrain", b"studio-edit")]),
        )
        .unwrap();

        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].kind, PatchKind::Conflict);
        // Neither side was overwritten: the file on disk is untouched.
        assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"base");
        // The path is frozen, and the base (hash only) is preserved.
        let record = store.get("t.terrain").unwrap().unwrap();
        assert!(record.conflicted);
        assert_eq!(record.base_content, None);

        // A frozen path is skipped on the next reconcile.
        let again = reconcile_blobs(
            &mut vfs,
            &store,
            &blobs(&[("t.terrain", b"fs-2")]),
            &blobs(&[("t.terrain", b"studio-2")]),
        )
        .unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn a_deleted_blob_propagates() {
        let mut vfs = MemoryVfs::new().with_file("t.terrain", b"v1".to_vec());
        let store = StateStore::open_in_memory().unwrap();
        base_with(&store, "t.terrain", b"v1");
        // FS deleted it; Studio still has the unchanged blob.
        let patches = reconcile_blobs(
            &mut vfs,
            &store,
            &BTreeMap::new(),
            &blobs(&[("t.terrain", b"v1")]),
        )
        .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].direction, Direction::ToStudio);
        assert_eq!(patches[0].kind, PatchKind::Delete);
        assert_eq!(store.get("t.terrain").unwrap(), None);
    }
}
