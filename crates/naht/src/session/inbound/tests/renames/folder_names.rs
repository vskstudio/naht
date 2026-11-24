use super::*;

fn all_changed_folder() -> [(&'static str, &'static str); 2] {
    [
        ("Tree/OldDir/a.luau", "return 1"),
        ("Tree/OldDir/Sub/b.luau", "return 2"),
    ]
}

fn all_changed_listing(folder: &str) -> Vec<InventoryEntry> {
    vec![
        inventory_entry(
            &format!("Tree/{folder}/a.luau"),
            "ModuleScript",
            "return 10",
        ),
        inventory_entry(
            &format!("Tree/{folder}/Sub/b.luau"),
            "ModuleScript",
            "return 20",
        ),
    ]
}

#[test]
fn a_folder_whose_scripts_all_changed_is_followed_by_their_names_and_asks_for_their_content() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &all_changed_folder());

    let wanted = session
        .apply_inventory(&all_changed_listing("NewDir"))
        .unwrap();

    assert_eq!(wanted, ["Tree/NewDir/Sub/b.luau", "Tree/NewDir/a.luau"]);
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/a.luau")).unwrap(),
        "return 1"
    );
}

#[test]
fn the_upserts_of_a_folder_followed_by_names_move_every_script_with_studio_content() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &all_changed_folder());
    session
        .apply_inventory(&all_changed_listing("NewDir"))
        .unwrap();

    session
        .apply_changes(vec![
            upsert("Tree/NewDir/a.luau", "return 10"),
            upsert("Tree/NewDir/Sub/b.luau", "return 20"),
        ])
        .unwrap();

    assert!(!root.path().join("Tree/OldDir").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/a.luau")).unwrap(),
        "return 10"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/Sub/b.luau")).unwrap(),
        "return 20"
    );
}

#[test]
fn a_disk_edit_made_while_stopped_merges_with_studio_content_under_the_folder_followed_by_names() {
    let root = tempfile::tempdir().unwrap();
    let base = "local a = 1\nlocal b = 2\nlocal c = 3\nreturn a\n";
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/a.luau", base),
            ("Tree/OldDir/b.luau", "return 2"),
        ],
    );
    fs::write(
        root.path().join("Tree/OldDir/a.luau"),
        "local a = 100\nlocal b = 2\nlocal c = 3\nreturn a\n",
    )
    .unwrap();
    let studio = "local a = 1\nlocal b = 2\nlocal c = 300\nreturn a\n";
    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/NewDir/a.luau", "ModuleScript", studio),
            inventory_entry("Tree/NewDir/b.luau", "ModuleScript", "return 20"),
        ])
        .unwrap();
    assert_eq!(wanted, ["Tree/NewDir/a.luau", "Tree/NewDir/b.luau"]);

    session
        .apply_changes(vec![upsert("Tree/NewDir/a.luau", studio)])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Tree/NewDir/a.luau")).unwrap(),
        "local a = 100\nlocal b = 2\nlocal c = 300\nreturn a\n"
    );
}

#[test]
fn a_new_folder_whose_names_differ_from_the_old_folder_infers_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &all_changed_folder());

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/NewDir/a.luau", "ModuleScript", "return 10"),
            inventory_entry("Tree/NewDir/Sub/c.luau", "ModuleScript", "return 20"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/OldDir/Sub/b.luau")).unwrap(),
        "return 2"
    );
}

#[test]
fn a_new_folder_whose_classes_differ_from_the_old_folder_infers_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &all_changed_folder());

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/NewDir/a.luau", "Script", "return 10"),
            inventory_entry("Tree/NewDir/Sub/b.luau", "ModuleScript", "return 20"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldDir/a.luau").exists());
}

#[test]
fn two_new_folders_with_the_same_names_infer_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &all_changed_folder());
    let mut listing = all_changed_listing("NewA");
    listing.extend(all_changed_listing("NewB"));

    let wanted = session.apply_inventory(&listing).unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldDir/a.luau").exists());
}

#[test]
fn two_old_folders_with_the_same_names_infer_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldA/a.luau", "return 1"),
            ("Tree/OldA/b.luau", "return 2"),
            ("Tree/OldB/a.luau", "return 3"),
            ("Tree/OldB/b.luau", "return 4"),
        ],
    );

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/NewDir/a.luau", "ModuleScript", "return 10"),
            inventory_entry("Tree/NewDir/b.luau", "ModuleScript", "return 20"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldA/a.luau").exists());
    assert!(root.path().join("Tree/OldB/a.luau").exists());
}

#[test]
fn an_old_folder_with_a_script_still_listed_is_not_followed_by_names() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/OldDir/a.luau", "return 1"),
            ("Tree/OldDir/b.luau", "return 2"),
            ("Tree/OldDir/Stay.luau", "return 5"),
        ],
    );

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/OldDir/Stay.luau", "ModuleScript", "return 5"),
            inventory_entry("Tree/NewDir/a.luau", "ModuleScript", "return 10"),
            inventory_entry("Tree/NewDir/b.luau", "ModuleScript", "return 20"),
        ])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldDir/a.luau").exists());
}

#[test]
fn a_folder_followed_by_names_does_not_block_a_folder_followed_by_content() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/Changed/a.luau", "return 1"),
            ("Tree/Changed/b.luau", "return 2"),
            ("Tree/Same/k.luau", "return 7"),
            ("Tree/Same/g.luau", "return 9"),
        ],
    );

    let wanted = session
        .apply_inventory(&[
            inventory_entry("Tree/ChangedNew/a.luau", "ModuleScript", "return 10"),
            inventory_entry("Tree/ChangedNew/b.luau", "ModuleScript", "return 20"),
            inventory_entry("Tree/SameNew/k.luau", "ModuleScript", "return 7"),
        ])
        .unwrap();

    assert_eq!(wanted, ["Tree/ChangedNew/a.luau", "Tree/ChangedNew/b.luau"]);
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/SameNew/g.luau")).unwrap(),
        "return 9"
    );
}

#[test]
fn a_deleted_single_script_folder_is_not_followed_by_names_into_an_unrelated_new_one() {
    let root = tempfile::tempdir().unwrap();
    let mut session =
        session_with_synced_scripts(root.path(), &[("Tree/Combat/a.luau", "return 1")]);

    let wanted = session
        .apply_inventory(&[inventory_entry(
            "Tree/Weapons/a.luau",
            "ModuleScript",
            "return 10",
        )])
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(root.path().join("Tree/Combat/a.luau")).unwrap(),
        "return 1"
    );
}

#[test]
fn a_new_folder_the_mirror_already_holds_is_not_followed_by_names() {
    let root = tempfile::tempdir().unwrap();
    let mut scripts = all_changed_folder().to_vec();
    scripts.push(("Tree/NewDir/c.luau", "return 3"));
    let mut session = session_with_synced_scripts(root.path(), &scripts);

    let wanted = session
        .apply_inventory(&all_changed_listing("NewDir"))
        .unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldDir/a.luau").exists());
}

#[test]
fn a_new_folder_holding_a_script_paired_by_content_is_not_followed_by_names() {
    let root = tempfile::tempdir().unwrap();
    let mut scripts = all_changed_folder().to_vec();
    scripts.push(("Tree/Other/c.luau", "return 3"));
    let mut session = session_with_synced_scripts(root.path(), &scripts);
    let mut listing = all_changed_listing("NewDir");
    listing.push(inventory_entry(
        "Tree/NewDir/c.luau",
        "ModuleScript",
        "return 3",
    ));

    let wanted = session.apply_inventory(&listing).unwrap();

    assert_eq!(wanted, Vec::<String>::new());
    assert!(root.path().join("Tree/OldDir/a.luau").exists());
}

#[test]
fn an_old_folder_a_paired_script_leaves_for_another_folder_is_not_followed_by_names() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[
            ("Tree/Old/a.luau", "return 1"),
            ("Tree/Old/Sub/b.luau", "return 2"),
            ("Tree/Old/c.luau", "return 3"),
        ],
    );
    let listing = vec![
        inventory_entry("Tree/Moved/a.luau", "ModuleScript", "return 1"),
        inventory_entry("Tree/New/Sub/b.luau", "ModuleScript", "return 20"),
        inventory_entry("Tree/New/c.luau", "ModuleScript", "return 30"),
        inventory_entry("Tree/New/a.luau", "ModuleScript", "return 5"),
    ];

    let wanted = session.apply_inventory(&listing).unwrap();

    assert!(!wanted.iter().any(|path| path.starts_with("Tree/New/")));
    assert!(root.path().join("Tree/Old/Sub/b.luau").exists());
}
