use super::*;
use naht_core::reconciler::{Direction, Patch, PatchKind};

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

pub(super) fn renamed_folder_listing(changed_content: &str) -> Vec<InventoryEntry> {
    vec![
        inventory_entry("Tree/NewDir/Kept.luau", "ModuleScript", "return 7"),
        inventory_entry("Tree/NewDir/Changed.luau", "ModuleScript", changed_content),
    ]
}

pub(super) fn old_folder_scripts() -> [(&'static str, &'static str); 2] {
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

fn queued_patch(path: &str, content: &str) -> Patch {
    Patch {
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        direction: Direction::ToStudio,
        kind: PatchKind::Update,
        content: Some(content.to_owned()),
    }
}

#[test]
fn twin_scripts_under_two_folders_do_not_infer_a_rename_and_leave_the_other_folder_alone() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/A/x.luau", "return 1"),
            ("Tree/A/z.luau", "return 5"),
            ("Tree/B/y.luau", "return 1"),
            ("Tree/B/w.luau", "return 6"),
        ],
    );

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/C/x.luau", "ModuleScript", "return 1"),
            inventory_entry("Tree/C/z.luau", "ModuleScript", "return 5"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/B/y.luau")).unwrap(),
        "return 1"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/B/w.luau")).unwrap(),
        "return 6"
    );
    assert!(!root.path().join("Tree/C/x.luau").exists());
    assert!(session.studio.contains_key("Tree/B/y.luau"));
    assert!(session.studio.contains_key("Tree/B/w.luau"));
}

#[test]
fn a_folder_split_across_two_new_folders_is_not_followed_into_either() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/a.luau", "return 7"),
            ("Tree/OldDir/b.luau", "return 2"),
        ],
    );

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/NewA/a.luau", "ModuleScript", "return 7"),
            inventory_entry("Tree/NewB/b.luau", "ModuleScript", "return 3"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewA/a.luau")).unwrap(),
        "return 7"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/b.luau")).unwrap(),
        "return 2"
    );
    assert!(!root.path().join("Tree/NewA/b.luau").exists());
    assert!(root.path().join("Tree/OldDir").is_dir());
}

#[test]
fn a_studio_deleted_script_does_not_overwrite_an_untracked_file_at_its_new_path() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/Kept.luau", "return 7"),
            ("Tree/OldDir/Gone.luau", "return 9"),
        ],
    );
    fs::create_dir_all(root.path().join("Tree/NewDir")).unwrap();
    fs::write(root.path().join("Tree/NewDir/Gone.luau"), "untracked").unwrap();

    session
        .apply_inventory(&[inventory_entry(
            "Tree/NewDir/Kept.luau",
            "ModuleScript",
            "return 7",
        )])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Gone.luau")).unwrap(),
        "untracked"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Gone.luau")).unwrap(),
        "return 9"
    );
}

#[test]
fn a_studio_deleted_script_moved_under_a_renamed_folder_is_mirrored_at_its_new_path_and_stays() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/Kept.luau", "return 7"),
            ("Tree/OldDir/Gone.luau", "return 9"),
        ],
    );
    let listing = [inventory_entry(
        "Tree/NewDir/Kept.luau",
        "ModuleScript",
        "return 7",
    )];
    session.apply_inventory(&listing).unwrap();

    assert!(!session.studio.contains_key("Tree/OldDir/Gone.luau"));
    assert_eq!(session.studio["Tree/NewDir/Gone.luau"].content, "return 9");
    assert!(
        session
            .store
            .get("Tree/NewDir/Gone.luau")
            .unwrap()
            .is_some()
    );
    assert!(
        session
            .store
            .get("Tree/OldDir/Gone.luau")
            .unwrap()
            .is_none()
    );

    let wanted = session.apply_inventory(&listing).unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(session.queue.is_empty());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Gone.luau")).unwrap(),
        "return 9"
    );
}

#[test]
fn a_script_already_pending_at_the_new_path_is_not_asked_for_nor_moved() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session.queue.insert(
        "Tree/NewDir/Changed.luau".to_owned(),
        (1, queued_patch("Tree/NewDir/Changed.luau", "return 4")),
    );

    let wanted = session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Changed.luau")).unwrap(),
        "return 2"
    );
}

#[test]
fn a_disk_edit_made_after_the_inventory_moves_with_its_pending_patch_and_is_not_lost() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();
    fs::write(
        root.path().join("Tree/OldDir/Changed.luau"),
        "return 2 edited",
    )
    .unwrap();
    session.queue.insert(
        "Tree/OldDir/Changed.luau".to_owned(),
        (
            1,
            queued_patch("Tree/OldDir/Changed.luau", "return 2 edited"),
        ),
    );

    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    assert!(!root.path().join("Tree/OldDir/Changed.luau").exists());
    let moved = fs::read_to_string(root.path().join("Tree/NewDir/Changed.luau")).unwrap();
    assert!(moved.contains("return 2 edited"), "disk edit lost: {moved}");
    assert!(moved.contains("return 3"), "studio content lost: {moved}");
}

#[test]
fn a_pending_patch_under_the_old_path_is_re_keyed_to_the_new_path_with_its_sequence() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();
    session.queue.insert(
        "Tree/OldDir/Changed.luau".to_owned(),
        (
            9,
            queued_patch("Tree/OldDir/Changed.luau", "return 2 edited"),
        ),
    );

    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    assert!(!session.queue.contains_key("Tree/OldDir/Changed.luau"));
    assert_eq!(session.queue["Tree/NewDir/Changed.luau"].0, 9);
}

#[test]
fn a_relocation_survives_an_unrelated_batch_until_its_target_is_posted() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();

    session
        .apply_changes(vec![upsert("Tree/Other.luau", "return 0")])
        .unwrap();
    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    assert!(!root.path().join("Tree/OldDir/Changed.luau").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Changed.luau")).unwrap(),
        "return 3"
    );
}
