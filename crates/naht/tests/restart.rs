//! Restart behaviour of a sync session over its persisted base.

#![cfg(test)]

use std::fs;
use std::path::Path;

use naht::commands;
use naht::session::Session;
use naht_core::binary::BlobInstance;
use naht_core::protocol::Change;
use naht_core::reconciler::{Direction, Patch, PatchKind};
use naht_core::state::StateStore;

const FILE: &str = "src/A.luau";

fn open(root: &Path, terrain_sync: bool) -> Session {
    fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None, terrain_sync).unwrap()
}

fn patches_of(session: &Session, cursor: u64) -> (u64, Vec<Patch>) {
    let (next, sequenced) = session.take_patches(cursor);
    (next, sequenced.into_iter().map(|item| item.patch).collect())
}

fn synced_and_acked(root: &Path, content: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(FILE), content).unwrap();
    let mut before = open(root, false);
    before.rescan().unwrap();
    let (_, first) = before.take_patches(0);
    let seqs: Vec<u64> = first.iter().map(|patch| patch.seq).collect();
    before.ack(&seqs).unwrap();
}

#[test]
fn a_restart_leaves_an_unchanged_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    synced_and_acked(root, "return 1");

    let mut after = open(root, false);
    after.rescan().unwrap();
    let (_, patches) = patches_of(&after, 0);

    assert!(
        root.join(FILE).exists(),
        "restart deleted an unchanged file"
    );
    assert!(patches.is_empty(), "restart re-emitted {patches:?}");
}

#[test]
fn an_edit_made_while_the_daemon_was_down_becomes_a_studio_update() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    synced_and_acked(root, "return 1");
    fs::write(root.join(FILE), "return 2").unwrap();

    let mut after = open(root, false);
    after.rescan().unwrap();
    let (_, patches) = patches_of(&after, 0);

    assert_eq!(patches.len(), 1, "expected one patch, got {patches:?}");
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Update);
    assert_eq!(patches[0].content.as_deref(), Some("return 2"));
    assert_eq!(fs::read_to_string(root.join(FILE)).unwrap(), "return 2");
}

#[test]
fn a_restart_leaves_an_unchanged_terrain_blob_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let blob = root.join("src").join("Map.terrain");
    fs::write(&blob, [1_u8, 2, 3]).unwrap();

    let mut before = open(root, true);
    before.rescan().unwrap();
    before
        .apply_blob_changes(vec![BlobInstance::new(
            "src/Map.terrain",
            "Terrain",
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    drop(before);

    let mut after = open(root, true);
    after.rescan().unwrap();
    let (_, patches) = after.take_blob_patches(0);

    assert!(blob.exists(), "restart deleted an unchanged terrain blob");
    assert!(
        patches.iter().all(|patch| patch.kind != PatchKind::Delete),
        "restart emitted a delete: {patches:?}"
    );
}

#[test]
fn a_terrain_blob_deleted_while_the_daemon_was_down_is_not_resurrected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let blob = root.join("src").join("Map.terrain");
    fs::write(&blob, [1_u8, 2, 3]).unwrap();

    let mut before = open(root, true);
    before.rescan().unwrap();
    before
        .apply_blob_changes(vec![BlobInstance::new(
            "src/Map.terrain",
            "Terrain",
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    drop(before);

    fs::remove_file(&blob).unwrap();
    let mut after = open(root, true);
    after.rescan().unwrap();
    after
        .apply_blob_changes(vec![BlobInstance::new(
            "src/Map.terrain",
            "Terrain",
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    let (_, patches) = after.take_blob_patches(0);

    assert!(!blob.exists(), "the deleted terrain blob was recreated");
    assert_eq!(patches.len(), 1, "expected one delete, got {patches:?}");
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Delete);
}

#[test]
fn a_blob_recreated_after_its_base_was_orphaned_is_created_in_studio() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    let blob = root.join("src").join("Map.terrain");
    fs::write(&blob, [1_u8, 2, 3]).unwrap();

    let mut synced = open(root, true);
    synced.rescan().unwrap();
    synced
        .apply_blob_changes(vec![BlobInstance::new(
            "src/Map.terrain",
            "Terrain",
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    drop(synced);

    fs::remove_file(&blob).unwrap();
    let mut orphaned = open(root, true);
    orphaned.rescan().unwrap();
    drop(orphaned);

    fs::write(&blob, [1_u8, 2, 3]).unwrap();
    let mut recreated = open(root, true);
    recreated.rescan().unwrap();
    let (_, before_report) = recreated.take_blob_patches(0);
    assert!(
        before_report.is_empty(),
        "Studio has not reported, nothing may be reconciled: {before_report:?}"
    );
    assert!(
        blob.exists(),
        "the re-created blob was removed before Studio reported"
    );

    recreated.apply_blob_changes(vec![]).unwrap();
    let (_, patches) = recreated.take_blob_patches(0);
    assert_eq!(
        patches.len(),
        1,
        "expected one ToStudio create, got {patches:?}"
    );
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Create);
}

fn conflicted(root: &Path) -> Session {
    synced_and_acked(root, "local a = 1\n");
    let mut session = open(root, false);
    session.rescan().unwrap();
    fs::write(root.join(FILE), "local a = 11\n").unwrap();
    session
        .apply_changes(vec![Change::Upsert {
            path: FILE.to_owned(),
            class: "ModuleScript".to_owned(),
            content: "local a = 22\n".to_owned(),
        }])
        .unwrap();
    assert!(
        fs::read_to_string(root.join(FILE))
            .unwrap()
            .contains("<<<<<<<"),
        "the overlapping edit should freeze the file with markers"
    );
    session
}

#[test]
fn a_resolved_conflict_reaches_studio_on_the_next_reconcile() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = conflicted(root);
    let (cursor, _) = patches_of(&session, 0);

    fs::write(root.join(FILE), "local a = 99\n").unwrap();
    commands::resolve(root, FILE, None).unwrap();
    session.rescan().unwrap();

    let (_, patches) = patches_of(&session, cursor);
    assert_eq!(
        patches.len(),
        1,
        "expected one ToStudio patch, got {patches:?}"
    );
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Update);
    assert_eq!(patches[0].content.as_deref(), Some("local a = 99\n"));
    assert_eq!(
        fs::read_to_string(root.join(FILE)).unwrap(),
        "local a = 99\n"
    );
}

#[test]
fn a_studio_edit_made_while_frozen_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut live = conflicted(root);
    live.apply_changes(vec![Change::Upsert {
        path: FILE.to_owned(),
        class: "ModuleScript".to_owned(),
        content: "local a = 33\n".to_owned(),
    }])
    .unwrap();
    drop(live);

    let mut restarted = open(root, false);
    restarted.rescan().unwrap();
    patches_of(&restarted, 0);
    fs::write(root.join(FILE), "local a = 99\n").unwrap();
    commands::resolve(root, FILE, None).unwrap();

    let mut resolved = open(root, false);
    resolved.rescan().unwrap();
    let (_, patches) = patches_of(&resolved, 0);

    assert!(
        patches
            .iter()
            .all(|patch| patch.content.as_deref() != Some("local a = 99\n")
                || patch.direction != Direction::ToStudio),
        "the Studio edit 33 was silently overwritten: {patches:?}"
    );
    let on_disk = fs::read_to_string(root.join(FILE)).unwrap();
    assert!(
        on_disk.contains("<<<<<<<") && on_disk.contains("local a = 33"),
        "expected the resolved file to conflict again with Studio's 33: {on_disk:?}"
    );
    assert_eq!(
        commands::status(root).unwrap().conflicted,
        vec![FILE.to_owned()]
    );
}

#[test]
fn a_resolved_conflict_reaches_studio_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    drop(conflicted(root));

    let mut restarted = open(root, false);
    restarted.rescan().unwrap();
    let (_, frozen) = patches_of(&restarted, 0);
    assert!(frozen.is_empty(), "a frozen path re-emitted {frozen:?}");
    assert_eq!(
        commands::status(root).unwrap().conflicted,
        vec![FILE.to_owned()]
    );
    drop(restarted);

    fs::write(root.join(FILE), "local a = 99\n").unwrap();
    commands::resolve(root, FILE, None).unwrap();

    let mut resolved = open(root, false);
    resolved.rescan().unwrap();
    let (_, patches) = patches_of(&resolved, 0);
    assert_eq!(
        patches.len(),
        1,
        "expected one ToStudio patch, got {patches:?}"
    );
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some("local a = 99\n"));
    assert_eq!(
        fs::read_to_string(root.join(FILE)).unwrap(),
        "local a = 99\n"
    );
}
