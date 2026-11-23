use super::*;

fn volume_is_case_insensitive(dir: &Path) -> bool {
    fs::write(dir.join("case-probe"), b"").unwrap();
    let folds = dir.join("CASE-PROBE").exists();
    fs::remove_file(dir.join("case-probe")).unwrap();
    folds
}

fn assert_case_insensitive_volume(dir: &Path) {
    assert!(
        volume_is_case_insensitive(dir),
        "the temp dir {} is case-sensitive, so the real-volume case rename tests prove nothing here; they must run on a case-insensitive volume",
        dir.display()
    );
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn disk_session(root: &Path, base_path: &str, base_content: &str) -> Session {
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, base_path, "ModuleScript", base_content).unwrap();
    Session::new(root, store, "demo", None, false).unwrap()
}

#[test]
fn a_case_only_folder_rename_on_a_case_insensitive_volume_keeps_the_folder_contents() {
    let root = tempfile::tempdir().unwrap();
    assert_case_insensitive_volume(root.path());
    fs::create_dir(root.path().join("Foo")).unwrap();
    fs::write(root.path().join("Foo").join("a.luau"), "old").unwrap();
    let mut session = disk_session(root.path(), "Foo/a.luau", "old");

    session
        .apply_changes(vec![
            Change::Delete {
                path: "Foo/a.luau".to_owned(),
            },
            upsert("foo/a.luau", "new"),
        ])
        .unwrap();

    assert_eq!(names_in(root.path()), ["foo"]);
    assert_eq!(
        fs::read_to_string(root.path().join("foo").join("a.luau")).unwrap(),
        "new"
    );
}

#[test]
fn a_case_only_folder_rename_onto_a_folder_spelled_differently_is_refused_and_rolled_back() {
    let root = tempfile::tempdir().unwrap();
    assert_case_insensitive_volume(root.path());
    fs::create_dir(root.path().join("utils")).unwrap();
    fs::write(root.path().join("utils").join("a.luau"), "old").unwrap();
    let mut session = disk_session(root.path(), "Utils/a.luau", "old");

    let outcome = session.apply_changes(vec![
        Change::Delete {
            path: "Utils/a.luau".to_owned(),
        },
        upsert("utils/a.luau", "new"),
    ]);

    assert!(matches!(
        outcome,
        Err(SessionError::FolderRenameBlocked { .. })
    ));
    assert_eq!(names_in(root.path()), ["utils"]);
    assert_eq!(
        fs::read_to_string(root.path().join("utils").join("a.luau")).unwrap(),
        "old"
    );
}
