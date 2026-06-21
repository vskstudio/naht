//! Stage 10 watcher/edge-case resilience: the daemon reconciles against the *final* filesystem
//! state, so an atomic save (write-temp-then-rename) and a rapid create→delete coalesce correctly
//! rather than flapping into spurious delete+create patches.

use naht::session::Session;
use naht_core::protocol::Change;
use naht_core::reconciler::PatchKind;
use naht_core::state::StateStore;

fn session(root: &std::path::Path) -> Session {
    std::fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None)
}

#[test]
fn an_atomic_save_yields_one_update_with_the_final_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = session(root);

    // Establish the base: the file syncs, and the plugin applies and acks it (advancing the base).
    std::fs::write(root.join("src").join("A.luau"), "return 1").unwrap();
    session.rescan().unwrap();
    let (cursor, first) = session.take_patches(0);
    assert_eq!(first.len(), 1);
    session.ack(&[first[0].path.clone()]).unwrap();

    // Atomic save: write a temp file, then rename it over the original.
    std::fs::write(root.join("src").join("A.luau.tmp"), "return 2").unwrap();
    std::fs::rename(
        root.join("src").join("A.luau.tmp"),
        root.join("src").join("A.luau"),
    )
    .unwrap();
    session.rescan().unwrap();

    let (_, second) = session.take_patches(cursor);
    assert_eq!(second.len(), 1, "atomic save should coalesce to one patch");
    assert_eq!(second[0].kind, PatchKind::Update);
    assert_eq!(second[0].content.as_deref(), Some("return 2"));
}

#[test]
fn a_create_then_delete_before_reconcile_produces_no_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = session(root);

    // A transient file that is gone again before the (debounced) reconcile runs.
    std::fs::write(root.join("src").join("Temp.luau"), "return 0").unwrap();
    std::fs::remove_file(root.join("src").join("Temp.luau")).unwrap();
    session.rescan().unwrap();

    let (_, patches) = session.take_patches(0);
    assert!(
        patches.is_empty(),
        "a file that never settled should produce no patch, got {patches:?}"
    );
}

#[test]
fn an_unacked_patch_holds_the_base_so_a_racing_studio_edit_merges_not_clobbers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = session(root);
    let file = root.join("src").join("A.luau");

    // Agreed base, applied and acked by the plugin.
    std::fs::write(&file, "a\nb\nc\n").unwrap();
    session.rescan().unwrap();
    let (_, first) = session.take_patches(0);
    let path = first[0].path.clone();
    session.ack(std::slice::from_ref(&path)).unwrap();

    // The filesystem edits line 1, but the plugin never acks — so the base is held at the agreed
    // content rather than jumping ahead to a change Studio hasn't seen.
    std::fs::write(&file, "A\nb\nc\n").unwrap();
    session.rescan().unwrap();

    // Studio edits line 3 concurrently and pushes it. Because the base is still the common ancestor,
    // this 3-way merges instead of overwriting the filesystem's line-1 change.
    session
        .apply_changes(vec![Change::Upsert {
            path,
            class: "ModuleScript".to_string(),
            content: "a\nb\nC\n".to_string(),
        }])
        .unwrap();

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "A\nb\nC\n");
}
