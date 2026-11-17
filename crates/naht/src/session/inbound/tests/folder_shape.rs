use super::*;
use std::path::PathBuf;

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap()
}

fn delete(path: &str) -> Change {
    Change::Delete {
        path: path.to_owned(),
    }
}

fn init_folder_session() -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Folder").join("init.luau"), "return 1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![upsert("Folder.luau", "return 1")])
        .unwrap();
    (root, session)
}

fn flat_script_session() -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Folder.luau"), "return 1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![upsert("Folder.luau", "return 1")])
        .unwrap();
    (root, session)
}

fn gain_first_child(session: &mut Session, delete_first: bool) {
    let grow = [
        upsert("Folder/init.luau", "return 1"),
        upsert("Folder/Child.luau", "child"),
    ];
    let mut batch = Vec::new();
    if delete_first {
        batch.push(delete("Folder.luau"));
    }
    batch.extend(grow);
    if !delete_first {
        batch.push(delete("Folder.luau"));
    }
    session.apply_changes(batch).unwrap();
}

#[test]
fn a_childless_init_script_that_gains_a_child_keeps_its_init_file_when_the_delete_comes_last() {
    let (root, mut session) = init_folder_session();
    gain_first_child(&mut session, false);

    assert_eq!(read(root.path(), "Folder/init.luau"), "return 1");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
    assert!(!root.path().join("Folder.luau").exists());
}

#[test]
fn a_childless_init_script_that_gains_a_child_keeps_its_init_file_when_the_delete_comes_first() {
    let (root, mut session) = init_folder_session();
    gain_first_child(&mut session, true);

    assert_eq!(read(root.path(), "Folder/init.luau"), "return 1");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
    assert!(!root.path().join("Folder.luau").exists());
}

#[test]
fn a_flat_script_that_gains_a_child_becomes_an_init_folder_when_the_delete_comes_last() {
    let (root, mut session) = flat_script_session();
    gain_first_child(&mut session, false);

    assert_eq!(read(root.path(), "Folder/init.luau"), "return 1");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
    assert!(!root.path().join("Folder.luau").exists());
}

#[test]
fn a_flat_script_that_gains_a_child_becomes_an_init_folder_when_the_delete_comes_first() {
    let (root, mut session) = flat_script_session();
    gain_first_child(&mut session, true);

    assert_eq!(read(root.path(), "Folder/init.luau"), "return 1");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
    assert!(!root.path().join("Folder.luau").exists());
}

#[test]
fn a_flat_server_script_that_gains_a_child_becomes_an_init_server_folder() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Folder.server.luau"), "s1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![Change::Upsert {
            path: "Folder.server.luau".to_owned(),
            class: "Script".to_owned(),
            content: "s1".to_owned(),
        }])
        .unwrap();

    session
        .apply_changes(vec![
            Change::Upsert {
                path: "Folder/init.server.luau".to_owned(),
                class: "Script".to_owned(),
                content: "s1".to_owned(),
            },
            upsert("Folder/Child.luau", "child"),
            delete("Folder.server.luau"),
        ])
        .unwrap();

    assert_eq!(read(root.path(), "Folder/init.server.luau"), "s1");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
    assert!(!root.path().join("Folder.server.luau").exists());
}

#[test]
fn deleting_a_childless_init_script_removes_its_empty_folder() {
    let (root, mut session) = init_folder_session();
    session.apply_changes(vec![delete("Folder.luau")]).unwrap();

    assert!(!root.path().join("Folder").exists());
}

#[test]
fn renaming_a_childless_init_script_removes_its_empty_folder() {
    let (root, mut session) = init_folder_session();
    session
        .apply_changes(vec![delete("Folder.luau"), upsert("Bar.luau", "return 1")])
        .unwrap();

    assert!(!root.path().join("Folder").exists());
    assert_eq!(read(root.path(), "Bar.luau"), "return 1");
}

#[test]
fn a_server_script_edit_of_a_childless_init_script_keeps_the_init_file() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Folder").join("init.server.luau"), "s1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    let server = |content: &str| Change::Upsert {
        path: "Folder.server.luau".to_owned(),
        class: "Script".to_owned(),
        content: content.to_owned(),
    };
    session.apply_changes(vec![server("s1")]).unwrap();

    session.apply_changes(vec![server("s2")]).unwrap();

    assert_eq!(read(root.path(), "Folder/init.server.luau"), "s2");
    assert!(!root.path().join("Folder.server.luau").exists());
}

#[test]
fn an_init_and_a_child_upserted_over_a_flat_script_without_deleting_it_keep_the_folder_shape() {
    let (root, mut session) = flat_script_session();

    session
        .apply_changes(vec![
            upsert("Folder/init.luau", "x"),
            upsert("Folder/Child.luau", "child"),
        ])
        .unwrap();

    assert_eq!(read(root.path(), "Folder/init.luau"), "x");
    assert_eq!(read(root.path(), "Folder/Child.luau"), "child");
}

#[test]
fn a_configured_source_root_spelled_with_a_dot_keeps_its_folder_when_only_its_init_script_is_deleted()
 {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src").join("init.luau"), "return 1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .protect_source_roots([PathBuf::from("./src")])
        .unwrap();
    session
        .apply_changes(vec![upsert("src.luau", "return 1")])
        .unwrap();

    session.apply_changes(vec![delete("src.luau")]).unwrap();

    assert!(root.path().join("src").is_dir());
}

#[test]
fn a_configured_nested_source_root_spelled_with_a_dot_keeps_its_directory_when_its_last_file_is_deleted()
 {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("packages").join("Shared")).unwrap();
    let mut session = session_in(root.path());
    session
        .apply_changes(vec![upsert("packages/Shared/A.luau", "return 1")])
        .unwrap();
    session
        .protect_source_roots([PathBuf::from("./packages/Shared")])
        .unwrap();

    session
        .apply_changes(vec![delete("packages/Shared/A.luau")])
        .unwrap();

    assert!(root.path().join("packages/Shared").is_dir());
}

#[test]
fn a_flat_script_that_gains_a_non_script_child_becomes_an_init_file_without_the_flat_one() {
    let (root, mut session) = flat_script_session();

    session
        .apply_changes(vec![
            delete("Folder.luau"),
            upsert("Folder/init.luau", "v2"),
        ])
        .unwrap();

    assert_eq!(read(root.path(), "Folder/init.luau"), "v2");
    assert!(!root.path().join("Folder.luau").exists());
}

#[test]
fn an_init_script_that_loses_its_last_child_becomes_a_flat_script_without_the_folder() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Folder").join("init.luau"), "v1").unwrap();
    fs::write(root.path().join("Folder").join("Child.luau"), "child").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![
            upsert("Folder/init.luau", "v1"),
            upsert("Folder/Child.luau", "child"),
        ])
        .unwrap();

    session
        .apply_changes(vec![
            delete("Folder/Child.luau"),
            delete("Folder/init.luau"),
            upsert("Folder.luau", "v2"),
        ])
        .unwrap();

    assert_eq!(read(root.path(), "Folder.luau"), "v2");
    assert!(!root.path().join("Folder").exists());
}

#[test]
fn a_deleted_flat_path_leaves_the_studio_mirror_when_its_init_file_is_upserted_in_the_same_batch() {
    let (root, mut session) = flat_script_session();
    fs::remove_file(root.path().join("Folder.luau")).unwrap();
    fs::create_dir_all(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Folder").join("init.luau"), "return 2").unwrap();

    session
        .apply_changes(vec![
            delete("Folder.luau"),
            upsert("Folder/init.luau", "return 2"),
        ])
        .unwrap();

    assert!(!session.studio.contains_key("Folder.luau"));
    assert_eq!(session.studio["Folder/init.luau"].content, "return 2");
    assert!(!root.path().join("Folder.luau").exists());
    assert_eq!(read(root.path(), "Folder/init.luau"), "return 2");
}
