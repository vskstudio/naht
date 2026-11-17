use rusqlite::Connection;

use super::StateStore;

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
