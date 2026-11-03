use naht::commands;
use naht_core::reconciler;
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};

use super::support::one;
use std::fs;

#[test]
fn status_and_resolve_reflect_the_conflict_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    let rel = "src/m.luau";
    fs::write(root.join("src").join("m.luau"), "local a = 1\n").unwrap();

    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let mut vfs = RootedVfs::new(root.clone(), DiskVfs::new());
    reconciler::reconcile(
        &mut vfs,
        &store,
        &one(rel, "local a = 1\n"),
        &one(rel, "local a = 1\n"),
    )
    .unwrap();
    reconciler::reconcile(
        &mut vfs,
        &store,
        &one(rel, "local a = 11\n"),
        &one(rel, "local a = 22\n"),
    )
    .unwrap();
    assert_eq!(
        reconciler::status(&store).unwrap().conflicted,
        vec![rel.to_owned()]
    );

    commands::status(&root).unwrap();

    assert!(commands::resolve(&root, rel).is_err());
    fs::write(root.join("src").join("m.luau"), "local a = 99\n").unwrap();
    commands::resolve(&root, rel).unwrap();

    let resolved_store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    assert!(
        reconciler::status(&resolved_store)
            .unwrap()
            .conflicted
            .is_empty()
    );
}
