use std::fs;
use std::path::Path;

use naht_core::binary::BlobInstance;
use naht_core::reconciler::{Direction, Patch, PatchKind};
use naht_core::state::StateStore;
use tracing::subscriber::with_default;

use crate::session::Session;
use crate::test_logs::LogBuffer;

fn session_in(root: &Path) -> Session {
    Session::new(
        root,
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        false,
    )
    .unwrap()
}

#[test]
fn an_acked_patch_leaves_the_queue() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    fs::write(root.path().join("A.luau"), "one").unwrap();
    session.rescan().unwrap();
    assert_eq!(session.take_patches(0).1.len(), 1);
    let delivered = session.queue["A.luau"].0;

    session.ack(&[delivered]).unwrap();

    assert_eq!(session.take_patches(0).1.len(), 0);
}

#[test]
fn an_ack_for_a_superseded_delivery_does_not_confirm_the_newer_patch() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    fs::write(root.path().join("A.luau"), "one").unwrap();
    session.rescan().unwrap();
    let delivered = session.queue["A.luau"].0;
    fs::write(root.path().join("A.luau"), "two").unwrap();
    session.rescan().unwrap();

    session.ack(&[delivered]).unwrap();

    assert!(!session.studio.contains_key("A.luau"));
    let (_, patches) = session.take_patches(0);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].patch.content.as_deref(), Some("two"));
}

fn terrain_session_in(root: &Path) -> Session {
    Session::new(
        root,
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        true,
    )
    .unwrap()
}

#[test]
fn a_blob_patch_leaves_the_queue_once_the_plugin_moves_its_cursor_past_it() {
    let root = tempfile::tempdir().unwrap();
    let mut session = terrain_session_in(root.path());
    session
        .apply_blob_changes(vec![BlobInstance::new(
            "Terrain.terrain",
            "Terrain",
            vec![1_u8, 2, 3],
        )])
        .unwrap();
    let blob = root.path().join("Terrain.terrain");
    fs::write(&blob, [4_u8, 5, 6]).unwrap();
    session.rescan().unwrap();
    session.rescan().unwrap();
    let (next, patches) = session.take_blob_patches(0);
    assert_eq!(patches.len(), 1);

    session.take_blob_patches(next);

    assert!(session.blob_queue.is_empty());
}

#[test]
fn an_unchanged_patch_keeps_its_delivered_seq_across_a_rescan() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    fs::write(root.path().join("A.luau"), "one").unwrap();
    session.rescan().unwrap();
    let delivered = session.queue["A.luau"].0;
    fs::write(root.path().join("B.luau"), "two").unwrap();
    session.rescan().unwrap();

    session.ack(&[delivered]).unwrap();

    assert!(!session.queue.contains_key("A.luau"));
    let (_, patches) = session.take_patches(0);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].patch.path, "B.luau");
}

#[test]
fn a_delivered_unacked_patch_is_redelivered_to_a_restarted_plugin() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    fs::write(root.path().join("A.luau"), "one").unwrap();
    session.rescan().unwrap();
    let (cursor, delivered) = session.take_patches(0);
    assert_eq!(delivered.len(), 1);
    session.rescan().unwrap();
    assert_eq!(session.take_patches(cursor).1.len(), 0);

    let (_, replay) = session.take_patches(0);

    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].patch.path, "A.luau");
}

#[test]
fn a_duplicate_patch_is_not_logged_as_emitted_twice() {
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    let patch = Patch {
        path: "A.luau".to_owned(),
        class: "ModuleScript".to_owned(),
        direction: Direction::ToStudio,
        kind: PatchKind::Create,
        content: Some("one".to_owned()),
    };

    with_default(subscriber, || {
        session.enqueue(patch.clone());
        session.enqueue(patch);
    });

    let text = logs.text();
    assert_eq!(text.matches("patch emitted").count(), 1);
    assert_eq!(session.queue.len(), 1);
}
