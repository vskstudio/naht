use naht::commands;

use super::support::{child, property, read_place, service};

#[test]
fn from_rojo_migrates_the_tree_so_the_build_matches_rojo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(root.join("src").join("common")).unwrap();
    std::fs::write(root.join("src").join("common").join("Mod.luau"), "return 1").unwrap();
    std::fs::write(
        root.join("src").join("common").join("Boot.server.luau"),
        "print(1)",
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("common").join("Ui.client.luau"),
        "print(2)",
    )
    .unwrap();
    std::fs::write(
        root.join("default.project.json"),
        r#"{
            "name": "Game",
            "tree": {
                "$className": "DataModel",
                "ReplicatedStorage": { "Common": { "$path": "src/common" } }
            }
        }"#,
    )
    .unwrap();

    commands::init(&root, true).unwrap();

    let place = dir.path().join("game.rbxl");
    commands::build(&root, &place).unwrap();
    let dom = read_place(&place);

    let rs = service(&dom, "ReplicatedStorage").expect("ReplicatedStorage service");
    let common = child(&dom, rs, "Common").expect("Common under ReplicatedStorage");
    assert_eq!(common.class, "Folder");
    let mut mapped: Vec<(String, String)> = common
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .map(|i| (i.name.clone(), i.class.as_str().to_owned()))
        .collect();
    mapped.sort();
    assert_eq!(
        mapped,
        vec![
            ("Boot".to_owned(), "Script".to_owned()),
            ("Mod".to_owned(), "ModuleScript".to_owned()),
            ("Ui".to_owned(), "LocalScript".to_owned()),
        ]
    );
    let workspace = service(&dom, "Workspace");
    assert!(workspace.is_none_or(|ws| child(&dom, ws, "src").is_none()));
}

#[test]
fn from_rojo_carries_classname_and_properties_with_a_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("default.project.json"),
        r#"{
            "name": "Game",
            "tree": {
                "$className": "DataModel",
                "ServerStorage": {
                    "Settings": { "$className": "BoolValue", "$properties": { "Value": true } }
                }
            }
        }"#,
    )
    .unwrap();

    commands::init(&root, true).unwrap();

    let place = dir.path().join("game.rbxl");
    commands::build(&root, &place).unwrap();
    let dom = read_place(&place);

    let storage = service(&dom, "ServerStorage").expect("ServerStorage service");
    let settings = child(&dom, storage, "Settings").expect("Settings under ServerStorage");
    assert_eq!(settings.class, "BoolValue");
    assert_eq!(
        property(settings, "Value"),
        Some(&rbx_dom_weak::types::Variant::Bool(true))
    );
}

#[test]
fn from_rojo_reports_what_it_cannot_represent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("default.project.json"),
        r#"{
            "name": "Game",
            "tree": {
                "$className": "DataModel",
                "$ignoreUnknownInstances": true,
                "Workspace": {
                    "Block": {
                        "$className": "Part",
                        "$properties": { "Size": { "Type": "Vector3", "Value": [1, 2, 3] } }
                    }
                }
            }
        }"#,
    )
    .unwrap();

    let warnings = commands::migrate_from_rojo(&root).unwrap();

    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Size") && w.contains("cannot be represented")),
        "expected a warning about the unrepresentable Vector3 property, got: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("$ignoreUnknownInstances")),
        "expected a warning about the unsupported directive, got: {warnings:?}"
    );
}
