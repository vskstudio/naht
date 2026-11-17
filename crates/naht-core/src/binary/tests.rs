use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::hash::content_hash;
use crate::reconciler::{Direction, PatchKind, ReconcileError};
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::{MemoryVfs, Vfs, VfsError};
use crate::walk::{Scan, Unseen};
use std::path::PathBuf;

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
    sync_blobs(&mut vfs, store, &instances, Some(&instances)).unwrap();
}

#[test]
fn scan_blobs_collects_only_terrain_files_with_exact_bytes() {
    let blob: Vec<u8> = (u8::MIN..=u8::MAX).cycle().take(300).collect();
    let vfs = MemoryVfs::new()
        .with_file("Workspace/Terrain.terrain", blob.clone())
        .with_file("Workspace/Main.luau", b"return 1".to_vec())
        .with_file("README.md", b"not an instance".to_vec());

    let found = scan_blobs(&vfs, Path::new("")).unwrap().instances;

    let keys: Vec<&String> = found.keys().collect();
    assert_eq!(keys, vec!["Workspace/Terrain.terrain"]);
    let instance = &found["Workspace/Terrain.terrain"];
    assert_eq!(instance.class, TERRAIN_CLASS);
    assert_eq!(instance.content, blob);
}

#[test]
fn a_blob_round_trips_each_way_without_byte_loss() {
    let blob: Vec<u8> = (u8::MIN..=u8::MAX).cycle().take(512).collect();

    let mut vfs = MemoryVfs::new();
    let store = StateStore::open_in_memory().unwrap();
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("Terrain.terrain", &blob)]),
        Some(&BTreeMap::new()),
    )
    .unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some(blob.as_slice()));

    let mut terrain_vfs = MemoryVfs::new();
    let terrain_store = StateStore::open_in_memory().unwrap();
    sync_blobs(
        &mut terrain_vfs,
        &terrain_store,
        &BTreeMap::new(),
        Some(&blobs(&[("Terrain.terrain", &blob)])),
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

    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"v2")]),
        Some(&blobs(&[("t.terrain", b"v1")])),
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

    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].kind, PatchKind::Conflict);
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"base");
    let record = store.get("t.terrain").unwrap().unwrap();
    assert!(record.conflicted);
    assert_eq!(record.base_content, None);
    assert_eq!(record.content_hash, content_hash(b"studio-edit"));

    let again = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-2")]),
        Some(&blobs(&[("t.terrain", b"studio-2")])),
    )
    .unwrap();
    assert_eq!(again.len(), 0);
}

fn freeze_t(vfs: &mut MemoryVfs, store: &StateStore) {
    vfs.write(Path::new("t.terrain"), b"fs-edit").unwrap();
    sync_blobs(
        vfs,
        store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();
}

#[test]
fn keep_disk_pushes_the_disk_bytes_to_studio_on_the_next_reconcile() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);

    resolve(&vfs, &store, "t.terrain", Keep::Disk).unwrap();
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some(&b"fs-edit"[..]));
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"fs-edit");
}

#[test]
fn keep_studio_writes_the_studio_bytes_to_disk_on_the_next_reconcile() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);

    resolve(&vfs, &store, "t.terrain", Keep::Studio).unwrap();
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToFs);
    assert_eq!(patches[0].content.as_deref(), Some(&b"studio-edit"[..]));
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"studio-edit");
}

#[test]
fn keep_studio_with_the_disk_file_gone_leaves_the_next_reconcile_to_recreate_it() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);
    vfs.remove(Path::new("t.terrain")).unwrap();

    resolve(&vfs, &store, "t.terrain", Keep::Studio).unwrap();
    assert!(store.get("t.terrain").unwrap().is_none());
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &BTreeMap::new(),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].kind, PatchKind::Create);
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"studio-edit");
}

#[test]
fn keep_disk_refuses_a_disk_copy_that_is_missing() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);
    vfs.remove(Path::new("t.terrain")).unwrap();

    assert!(matches!(
        resolve(&vfs, &store, "t.terrain", Keep::Disk),
        Err(ReconcileError::MissingOnDisk(_))
    ));
    assert!(store.get("t.terrain").unwrap().unwrap().conflicted);
}

#[test]
fn a_deleted_blob_propagates() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"v1".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"v1");
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &BTreeMap::new(),
        Some(&blobs(&[("t.terrain", b"v1")])),
    )
    .unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Delete);
    assert_eq!(store.get("t.terrain").unwrap(), None);
}

fn sync_blobs(
    vfs: &mut impl Vfs,
    store: &StateStore,
    fs: &BTreeMap<String, BlobInstance>,
    studio: Option<&BTreeMap<String, BlobInstance>>,
) -> Result<Vec<BlobPatch>, ReconcileError> {
    reconcile_blobs(vfs, store, fs, studio, &Unseen::default())
}

fn unseen_at(path: &str) -> Unseen {
    let mut scan = Scan::<BlobInstance>::default();
    scan.skip(
        PathBuf::from(path),
        ReconcileError::Vfs(VfsError::InvalidPath("unreadable".to_owned())),
    );
    scan.unseen()
}

#[test]
fn an_unreadable_blob_missing_from_the_fs_side_is_neither_deleted_nor_forgotten() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"v1".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"v1");

    let patches = reconcile_blobs(
        &mut vfs,
        &store,
        &BTreeMap::new(),
        Some(&blobs(&[("t.terrain", b"v1")])),
        &unseen_at("t.terrain"),
    )
    .unwrap();

    assert_eq!(patches.len(), 0);
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"v1");
    assert!(store.get("t.terrain").unwrap().is_some());
}

#[test]
fn an_unseen_path_missing_from_the_studio_report_keeps_its_base() {
    let mut vfs = MemoryVfs::new();
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"v1");

    let patches = reconcile_blobs(
        &mut vfs,
        &store,
        &BTreeMap::new(),
        Some(&BTreeMap::new()),
        &unseen_at("t.terrain"),
    )
    .unwrap();

    assert_eq!(patches.len(), 0);
    assert!(store.get("t.terrain").unwrap().is_some());
}

fn legacy_frozen_t(store: &StateStore, disk_hash: &[u8]) {
    store
        .upsert(&InstanceRecord {
            guid: "t.terrain".to_owned(),
            path: "t.terrain".to_owned(),
            class: CLASS.to_owned(),
            content_hash: content_hash(disk_hash),
            base_content: None,
            mtime: 0,
            conflicted: true,
        })
        .unwrap();
}

#[test]
fn keep_disk_on_a_legacy_record_pushes_the_disk_bytes_not_studio() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"fs-edit".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    legacy_frozen_t(&store, b"fs-edit");

    resolve(&vfs, &store, "t.terrain", Keep::Disk).unwrap();
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some(&b"fs-edit"[..]));
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"fs-edit");
}

#[test]
fn keep_studio_on_a_legacy_record_writes_the_studio_bytes_to_disk() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"fs-edit".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    legacy_frozen_t(&store, b"fs-edit");

    resolve(&vfs, &store, "t.terrain", Keep::Studio).unwrap();
    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToFs);
    assert_eq!(patches[0].content.as_deref(), Some(&b"studio-edit"[..]));
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"studio-edit");
}

#[test]
fn a_studio_edit_after_keep_disk_and_one_reconcile_reaches_the_disk() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);
    resolve(&vfs, &store, "t.terrain", Keep::Disk).unwrap();
    sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-later")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToFs);
    assert_eq!(vfs.read(Path::new("t.terrain")).unwrap(), b"studio-later");
}

#[test]
fn keep_disk_with_matching_studio_bytes_pushes_nothing() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"base".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    base_with(&store, "t.terrain", b"base");
    freeze_t(&mut vfs, &store);
    resolve(&vfs, &store, "t.terrain", Keep::Disk).unwrap();

    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"fs-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 0);
}

#[test]
fn a_legacy_empty_base_hash_still_keeps_the_disk_bytes() {
    let mut vfs = MemoryVfs::new().with_file("t.terrain", b"fs-edit".to_vec());
    let store = StateStore::open_in_memory().unwrap();
    store
        .upsert(&InstanceRecord {
            guid: "t.terrain".to_owned(),
            path: "t.terrain".to_owned(),
            class: CLASS.to_owned(),
            content_hash: String::new(),
            base_content: None,
            mtime: 0,
            conflicted: false,
        })
        .unwrap();

    let patches = sync_blobs(
        &mut vfs,
        &store,
        &blobs(&[("t.terrain", b"fs-edit")]),
        Some(&blobs(&[("t.terrain", b"studio-edit")])),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some(&b"fs-edit"[..]));
}
