//! Stage 10 watcher/edge-case resilience: the daemon reconciles against the *final* filesystem
//! state, so an atomic save (write-temp-then-rename) and a rapid create→delete coalesce correctly
//! rather than flapping into spurious delete+create patches.

use naht::session::Session;
use naht_core::protocol::Change;
use naht_core::reconciler::PatchKind;
use naht_core::state::StateStore;
use std::fs;
use std::path::Path;
use std::slice;

fn session(root: &Path) -> Session {
    fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None, true)
}

#[test]
fn an_atomic_save_yields_one_update_with_the_final_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = session(root);

    fs::write(root.join("src").join("A.luau"), "return 1").unwrap();
    session.rescan().unwrap();
    let (cursor, first) = session.take_patches(0);
    assert_eq!(first.len(), 1);
    session.ack(&[first[0].path.clone()]).unwrap();

    fs::write(root.join("src").join("A.luau.tmp"), "return 2").unwrap();
    fs::rename(
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

    fs::write(root.join("src").join("Temp.luau"), "return 0").unwrap();
    fs::remove_file(root.join("src").join("Temp.luau")).unwrap();
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

    fs::write(&file, "a\nb\nc\n").unwrap();
    session.rescan().unwrap();
    let (_, first) = session.take_patches(0);
    let path = first[0].path.clone();
    session.ack(slice::from_ref(&path)).unwrap();

    fs::write(&file, "A\nb\nc\n").unwrap();
    session.rescan().unwrap();

    session
        .apply_changes(vec![Change::Upsert {
            path,
            class: "ModuleScript".to_owned(),
            content: "a\nb\nC\n".to_owned(),
        }])
        .unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "A\nb\nC\n");
}
