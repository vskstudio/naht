use super::*;
use crate::commands;
use crate::config::INTERNAL_DIR;
use naht_core::protocol::Change;
use naht_core::state::StateStore;
use std::fs;
use std::path::PathBuf;

const FILE: &str = "Outer/Inner/A.luau";

fn upsert(path: &str, content: &str) -> Change {
    Change::Upsert {
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        content: content.to_owned(),
    }
}

fn synced_project(protected: &[&str]) -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(INTERNAL_DIR)).unwrap();
    fs::create_dir_all(root.path().join("Outer/Inner")).unwrap();
    fs::write(root.path().join(FILE), "return 1").unwrap();
    let store = StateStore::open(&root.path().join(INTERNAL_DIR).join("state.db")).unwrap();
    let mut session = Session::new(root.path(), store, "demo", None, false).unwrap();
    session.rescan().unwrap();
    session
        .apply_changes(vec![upsert(FILE, "return 1")])
        .unwrap();
    session
        .protect_source_roots(protected.iter().map(PathBuf::from))
        .unwrap();
    (root, session)
}

fn studio_drops_the_file(session: &mut Session) {
    session.studio.remove(FILE);
}

#[test]
fn a_resolve_that_lifts_a_freeze_on_a_file_studio_dropped_removes_the_folder_it_empties() {
    let (root, mut session) = synced_project(&[]);
    session.store.set_conflicted(FILE, true).unwrap();
    studio_drops_the_file(&mut session);

    commands::resolve(root.path(), FILE, None).unwrap();
    session.rescan().unwrap();

    assert!(!root.path().join(FILE).exists());
    assert!(!root.path().join("Outer/Inner").exists());
    assert!(root.path().join("Outer").is_dir());
}

#[cfg(windows)]
#[test]
fn a_resolve_given_backslashes_lifts_the_freeze_on_the_forward_slash_key() {
    let (root, session) = synced_project(&[]);
    session.store.set_conflicted(FILE, true).unwrap();

    commands::resolve(root.path(), "Outer\\Inner\\A.luau", None).unwrap();

    assert_eq!(session.store.conflicted().unwrap(), Vec::new());
}

#[test]
fn a_pull_that_deletes_the_last_file_of_a_folder_removes_the_folder() {
    let (root, mut session) = synced_project(&[]);
    studio_drops_the_file(&mut session);

    session.rescan().unwrap();

    assert!(!root.path().join(FILE).exists());
    assert!(!root.path().join("Outer/Inner").exists());
    assert!(root.path().join("Outer").is_dir());
}

#[test]
fn a_pull_keeps_a_configured_source_root_when_its_last_file_is_deleted() {
    let (root, mut session) = synced_project(&["Outer/Inner"]);
    studio_drops_the_file(&mut session);

    session.rescan().unwrap();

    assert!(!root.path().join(FILE).exists());
    assert!(root.path().join("Outer/Inner").is_dir());
}

#[test]
fn a_folder_the_user_empties_on_disk_is_kept_by_a_rescan() {
    let (root, mut session) = synced_project(&[]);
    fs::remove_file(root.path().join(FILE)).unwrap();

    session.rescan().unwrap();

    assert!(root.path().join("Outer/Inner").is_dir());
}
