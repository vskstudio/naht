use std::path::PathBuf;

use rusqlite::Connection;

use super::schema::{forward_slash_keys, schema_is_current};
use super::{InstanceRecord, StateStore};

const MAIN_SCHEMA: &str = "CREATE TABLE meta (
         key   TEXT PRIMARY KEY,
         value TEXT NOT NULL
     );
     CREATE TABLE instances (
         guid         TEXT PRIMARY KEY,
         path         TEXT NOT NULL,
         class        TEXT NOT NULL,
         content_hash TEXT NOT NULL,
         base_content BLOB,
         mtime        INTEGER NOT NULL,
         conflicted   INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE assets (
         content_hash TEXT PRIMARY KEY,
         asset_id     TEXT NOT NULL
     );
     INSERT INTO meta (key, value) VALUES ('schema_version', '1');
     INSERT INTO instances (guid, path, class, content_hash, base_content, mtime, conflicted)
         VALUES ('src/A.luau', 'src/A.luau', 'ModuleScript', 'hash', X'72657475726e2031', 0, 0);";

#[test]
fn a_database_from_the_main_branch_schema_opens_with_its_bases_and_tracks_studio_sides() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch(MAIN_SCHEMA)
        .unwrap();

    let store = StateStore::open(&path).unwrap();

    let base = store.get("src/A.luau").unwrap().expect("the base survives");
    assert_eq!(base.base_content.as_deref(), Some(b"return 1".as_slice()));
    assert_eq!(store.studio_sides().unwrap(), Vec::new());

    store
        .set_studio_side("src/A.luau", "ModuleScript", Some("return 2"))
        .unwrap();
    let side = store
        .studio_side("src/A.luau")
        .unwrap()
        .expect("the Studio side is tracked");
    assert_eq!(side.content.as_deref(), Some("return 2"));
    assert_eq!(store.studio_sides().unwrap().len(), 1);
}

#[test]
fn a_migration_interrupted_before_the_version_write_rolls_back_and_stays_not_current() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TRIGGER refuse_the_version BEFORE INSERT ON meta
             BEGIN SELECT RAISE(ABORT, 'interrupted before the version write'); END;",
        )
        .unwrap();

    assert!(
        StateStore::open(&path).err().is_some(),
        "the interrupted migration must fail"
    );

    let conn = Connection::open(&path).unwrap();
    assert!(!schema_is_current(&conn).unwrap());
    let instances: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'instances'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(instances, 0, "the migration must roll back as a whole");
}

fn record(path: &str, base: &str, conflicted: bool) -> InstanceRecord {
    InstanceRecord {
        guid: path.to_owned(),
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        content_hash: base.to_owned(),
        base_content: Some(base.as_bytes().to_vec()),
        mtime: 0,
        conflicted,
    }
}

fn store_with(dir: &tempfile::TempDir, records: &[InstanceRecord]) -> PathBuf {
    let path = dir.path().join("state.db");
    let store = StateStore::open(&path).unwrap();
    for record in records {
        store.upsert(record).unwrap();
    }
    path
}

#[test]
fn a_base_keyed_with_backslashes_moves_to_its_forward_slash_key_with_its_content_and_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_with(&dir, &[record("src\\Pkg\\A.luau", "return 1", true)]);

    forward_slash_keys(&mut Connection::open(&path).unwrap()).unwrap();

    let store = StateStore::open(&path).unwrap();
    assert_eq!(store.get("src\\Pkg\\A.luau").unwrap(), None);
    assert_eq!(
        store.get("src/Pkg/A.luau").unwrap(),
        Some(record("src/Pkg/A.luau", "return 1", true))
    );
}

#[test]
fn a_backslash_base_beside_its_forward_slash_twin_is_dropped_and_the_twin_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_with(
        &dir,
        &[
            record("src\\A.luau", "return 1", false),
            record("src/A.luau", "return 2", false),
        ],
    );

    forward_slash_keys(&mut Connection::open(&path).unwrap()).unwrap();

    let store = StateStore::open(&path).unwrap();
    assert_eq!(
        store.all().unwrap(),
        vec![record("src/A.luau", "return 2", false)]
    );
}

#[cfg(windows)]
#[test]
fn opening_a_store_on_windows_moves_backslash_bases_to_forward_slash_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = store_with(&dir, &[record("src\\A.luau", "return 1", false)]);

    let store = StateStore::open(&path).unwrap();

    assert_eq!(
        store.all().unwrap(),
        vec![record("src/A.luau", "return 1", false)]
    );
}
