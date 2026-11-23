use super::folder_moves::{old_folder_scripts, renamed_folder_listing};
use super::*;

#[test]
fn a_script_removed_in_studio_stays_put_when_its_folder_is_renamed_around_it() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/Kept.luau", "return 7"),
            ("Tree/OldDir/Moved.luau", "return 4"),
            ("Tree/OldDir/Gone.luau", "return 9"),
        ],
    );

    session
        .apply_inventory(&[
            inventory_entry("Tree/OldDir/Kept.luau", "ModuleScript", "return 7"),
            inventory_entry("Tree/NewDir/Moved.luau", "ModuleScript", "return 4"),
        ])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Gone.luau")).unwrap(),
        "return 9"
    );
    assert!(!root.path().join("Tree/NewDir/Gone.luau").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Kept.luau")).unwrap(),
        "return 7"
    );
}

#[test]
fn a_folder_whose_scripts_leave_for_two_new_folders_keeps_its_other_script_in_place() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/a.luau", "return 7"),
            ("Tree/OldDir/b.luau", "return 2"),
            ("Tree/OldDir/c.luau", "return 5"),
        ],
    );

    session
        .apply_inventory(&[
            inventory_entry("Tree/NewA/a.luau", "ModuleScript", "return 7"),
            inventory_entry("Tree/NewB/b.luau", "ModuleScript", "return 2"),
        ])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/c.luau")).unwrap(),
        "return 5"
    );
    assert!(!root.path().join("Tree/NewA/c.luau").exists());
    assert!(!root.path().join("Tree/NewB/c.luau").exists());
}

#[test]
fn an_untracked_file_appearing_at_a_relocation_target_is_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();
    fs::create_dir_all(root.path().join("Tree/NewDir")).unwrap();
    fs::write(root.path().join("Tree/NewDir/Changed.luau"), "untracked").unwrap();

    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    let landed = fs::read_to_string(root.path().join("Tree/NewDir/Changed.luau")).unwrap();
    assert!(
        landed.contains("untracked"),
        "untracked file lost: {landed}"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Changed.luau")).unwrap(),
        "return 2"
    );
}

#[test]
fn a_relocation_whose_old_file_vanished_falls_through_to_the_upsert_and_the_batch_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &old_folder_scripts());
    session
        .apply_inventory(&renamed_folder_listing("return 3"))
        .unwrap();
    fs::remove_file(root.path().join("Tree/OldDir/Changed.luau")).unwrap();

    session
        .apply_changes(vec![upsert("Tree/NewDir/Changed.luau", "return 3")])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Changed.luau")).unwrap(),
        "return 3"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Kept.luau")).unwrap(),
        "return 7"
    );
}
