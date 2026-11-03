use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::hash::content_hash;
use crate::reconciler::{Direction, PatchKind};
use crate::state::StateStore;
use crate::vfs::{MemoryVfs, Vfs};

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
    let mut vfs = MemoryVfs::new();
    let instances = blobs(&[(path, content)]);
    reconcile_blobs(&mut vfs, store, &instances, &instances).unwrap();
}

#[test]
fn scan_blobs_collects_only_terrain_files_with_exact_bytes() {
    let blob: Vec<u8> = (0u16..300).map(|index| (index % 256) as u8).collect();
    let vfs = MemoryVfs::new()
        .with_file("Workspace/Terrain.terrain", blob.clone())
        .with_file("Workspace/Main.luau", b"return 1".to_vec())
        .with_file("README.md", b"not an instance".to_vec());

    let found = scan_blobs(&vfs, Path::new("")).unwrap();

    let keys: Vec<&String> = found.keys().collect();
    assert_eq!(keys, vec!["Workspace/Terrain.terrain"]);
    let instance = &found["Workspace/Terrain.terrain"];
    assert_eq!(instance.class, TERRAIN_CLASS);
    assert_eq!(instance.content, blob);
}

#[test]
fn a_blob_round_trips_each_way_without_byte_loss() {
    let blob: Vec<u8> = (0u16..512).map(|index| (index % 256) as u8).collect();

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

    let mut terrain_vfs = MemoryVfs::new();
    let terrain_store = StateStore::open_in_memory().unwrap();
    reconcile_blobs(
        &mut terrain_vfs,
        &terrain_store,
        &BTreeMap::new(),
        &blobs(&[("Terrain.terrain", &blob)]),
    )
    .unwrap();
    assert_eq!(
        terrain_vfs.read(Path::new("Terrain.terrain")).unwrap(),
        blob
    );
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
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"base");
    let record = store.get("t.terrain").unwrap().unwrap();
    assert!(record.conflicted);
    assert_eq!(record.base_content, None);

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
