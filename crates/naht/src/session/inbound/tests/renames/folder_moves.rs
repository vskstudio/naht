use super::*;

fn folder_session_over_case_folding_disk(
    refused_write: Option<&str>,
    scripts: &[(&str, &str)],
) -> Session<CaseFoldingVfs> {
    let mut inner = MemoryVfs::new();
    let store = StateStore::open_in_memory().unwrap();
    for (path, content) in scripts {
        let parent = Path::new(path).parent().unwrap();
        if !parent.as_os_str().is_empty() && !inner.exists(parent) {
            inner.create_dir(parent).unwrap();
        }
        inner.write(Path::new(path), content.as_bytes()).unwrap();
        set_base(&store, path, "ModuleScript", content).unwrap();
    }
    let vfs = CaseFoldingVfs {
        inner,
        refused_write: refused_write.map(PathBuf::from),
    };
    Session::assemble(RootedVfs::new("", vfs), store, "demo", None, false).unwrap()
}

fn renamed_folder_listing(changed_content: &str) -> Vec<InventoryEntry> {
    vec![
        inventory_entry("Tree/NewDir/Kept.luau", "ModuleScript", "return 7"),
        inventory_entry("Tree/NewDir/Changed.luau", "ModuleScript", changed_content),
    ]
}

fn old_folder_scripts() -> [(&'static str, &'static str); 2] {
    [
        ("Tree/OldDir/Kept.luau", "return 7"),
        ("Tree/OldDir/Changed.luau", "return 2"),
    ]
}

#[test]
fn a_folder_renamed_while_stopped_takes_a_script_studio_deleted() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/Kept.luau", "return 7"),
            ("Tree/OldDir/Gone.luau", "return 9"),
        ],
    );

    let wanted = session
        .apply_inventory(&[inventory_entry(
            "Tree/NewDir/Kept.luau",
            "ModuleScript",
            "return 7",
        )])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(!root.path().join("Tree/OldDir").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Gone.luau")).unwrap(),
        "return 9"
    );
}

#[test]
fn a_script_under_a_folder_that_was_not_renamed_is_left_alone_when_studio_deleted_it() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/Kept.luau", "return 7"),
            ("Tree/OldDir/Gone.luau", "return 9"),
        ],
    );

    session
        .apply_inventory(&[inventory_entry(
            "Tree/OldDir/Kept.luau",
            "ModuleScript",
            "return 7",
        )])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Gone.luau")).unwrap(),
        "return 9"
    );
}

#[test]
fn a_script_changed_in_studio_under_a_renamed_folder_is_asked_for_and_not_moved() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());

    let wanted = session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();

    assert_eq!(wanted, ["Tree/NewDir/Changed.luau"]);
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Changed.luau")).unwrap(),
        "return 2"
    );
    assert!(!root.path().join("Tree/NewDir/Changed.luau").exists());
}

#[test]
fn the_upsert_of_a_changed_script_moves_it_under_the_renamed_folder_with_studio_content() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();

    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    assert!(!root.path().join("Tree/OldDir").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Changed.luau")).unwrap(),
        "return 3"
    );
}

#[test]
fn a_refused_write_rolls_the_folder_move_back() {
    let mut session = folder_session_over_case_folding_disk(
        Some("Tree/NewDir/Changed.luau"),
        &old_folder_scripts(),
    );
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();

    let outcome = session.apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")]);

    assert!(outcome.is_err());
    assert!(!session.vfs.exists(Path::new("Tree/NewDir/Changed.luau")));
    assert_eq!(
        session
            .vfs
            .read(Path::new("Tree/OldDir/Changed.luau"))
            .unwrap(),
        b"return 2"
    );
}
