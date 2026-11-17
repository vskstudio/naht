use std::collections::BTreeMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use naht_core::protocol::Change;
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::Vfs;

use super::{Journaled, Transaction};
use crate::session::Session;

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

fn upsert(path: &str, content: &str) -> Change {
    Change::Upsert {
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        content: content.to_owned(),
    }
}

fn base_records(session: &Session) -> BTreeMap<String, InstanceRecord> {
    session
        .store
        .all()
        .unwrap()
        .into_iter()
        .map(|record| (record.guid.clone(), record))
        .collect()
}

#[test]
fn a_path_absent_before_the_batch_and_written_twice_is_absent_after_the_batch_fails() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());

    let mut batch = Transaction::begin(&mut session).unwrap();
    let inner = &mut *batch;
    let mut disk = Journaled {
        inner: &mut inner.vfs,
        journal: &mut inner.journal,
    };
    disk.write(Path::new("Twice.luau"), b"1").unwrap();
    disk.write(Path::new("Twice.luau"), b"2").unwrap();
    assert!(root.path().join("Twice.luau").exists());
    drop(batch);

    assert!(!root.path().join("Twice.luau").exists());
}

#[test]
fn a_file_deleted_then_recreated_by_the_batch_gets_its_original_content_back() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Keep.luau"), "orig").unwrap();
    let mut session = session_in(root.path());

    let mut batch = Transaction::begin(&mut session).unwrap();
    let inner = &mut *batch;
    let mut disk = Journaled {
        inner: &mut inner.vfs,
        journal: &mut inner.journal,
    };
    disk.remove(Path::new("Keep.luau")).unwrap();
    disk.write(Path::new("Keep.luau"), b"new").unwrap();
    drop(batch);

    assert_eq!(
        fs::read_to_string(root.path().join("Keep.luau")).unwrap(),
        "orig"
    );
}

#[test]
fn a_panic_inside_a_batch_restores_the_state_it_had_changed() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    let before = base_records(&session);

    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut batch = Transaction::begin(&mut session).unwrap();
        batch
            .stage_text_changes(vec![upsert("Aok.luau", "X")])
            .unwrap();
        batch.reconcile_all().unwrap();
        panic!("the batch panicked after writing");
    }));

    assert!(outcome.is_err());
    assert!(!root.path().join("Aok.luau").exists());
    assert!(!session.studio.contains_key("Aok.luau"));
    assert_eq!(base_records(&session), before);
}

#[test]
fn removing_a_directory_outside_a_batch_keeps_its_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Tree")).unwrap();
    fs::write(root.path().join("Tree").join("Deep.luau"), "deep").unwrap();
    let mut session = session_in(root.path());

    let mut disk = Journaled {
        inner: &mut session.vfs,
        journal: &mut session.journal,
    };
    assert!(disk.remove(Path::new("Tree")).is_err());

    assert_eq!(
        fs::read_to_string(root.path().join("Tree").join("Deep.luau")).unwrap(),
        "deep"
    );
}

#[test]
fn a_batch_cannot_remove_a_directory_so_its_files_survive_a_failed_batch() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Dir")).unwrap();
    fs::write(root.path().join("Dir").join("Child.luau"), "child").unwrap();
    let mut session = session_in(root.path());

    let mut batch = Transaction::begin(&mut session).unwrap();
    let inner = &mut *batch;
    let mut disk = Journaled {
        inner: &mut inner.vfs,
        journal: &mut inner.journal,
    };
    assert!(disk.remove(Path::new("Dir")).is_err());
    drop(batch);

    assert!(root.path().join("Dir").is_dir());
    assert_eq!(
        fs::read_to_string(root.path().join("Dir").join("Child.luau")).unwrap(),
        "child"
    );
}
