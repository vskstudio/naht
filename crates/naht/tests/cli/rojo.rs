use naht::commands::{self, CommandError};

use super::support::{child, property, read_place, service};
use rbx_dom_weak::types::Variant;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[test]
fn from_rojo_migrates_the_tree_so_the_build_matches_rojo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    fs::create_dir_all(root.join("src").join("common")).unwrap();
    fs::write(root.join("src").join("common").join("Mod.luau"), "return 1").unwrap();
    fs::write(
        root.join("src").join("common").join("Boot.server.luau"),
        "print(1)",
    )
    .unwrap();
    fs::write(
        root.join("src").join("common").join("Ui.client.luau"),
        "print(2)",
    )
    .unwrap();
    fs::write(
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
        .map(|reference| dom.get_by_ref(*reference).unwrap())
        .map(|instance| (instance.name.clone(), instance.class.as_str().to_owned()))
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
    fs::create_dir_all(&root).unwrap();
    fs::write(
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
    assert_eq!(property(settings, "Value"), Some(&Variant::Bool(true)));
}

#[test]
fn from_rojo_reports_what_it_cannot_represent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    fs::create_dir_all(&root).unwrap();
    fs::write(
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
            .any(|warning| warning.contains("Size") && warning.contains("cannot be represented")),
        "expected a warning about the unrepresentable Vector3 property, got: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("$ignoreUnknownInstances")),
        "expected a warning about the unsupported directive, got: {warnings:?}"
    );
}

fn project_with_a_sibling_outside_the_root(dir: &Path) -> io::Result<PathBuf> {
    let root = dir.join("game");
    fs::create_dir_all(root.join("src"))?;
    let outside = dir.join("outside");
    fs::create_dir_all(&outside)?;
    fs::write(outside.join("Secret.luau"), "return 'outside'")?;
    Ok(root)
}

fn tree_config(root: &Path, source: &str) -> io::Result<()> {
    fs::write(
        root.join("naht.toml"),
        format!(
            "[project]\nname = \"Game\"\n\n[[tree]]\ninstance = \"ReplicatedStorage/Shared\"\npath = '{source}'\n"
        ),
    )
}

#[test]
fn from_rojo_rejects_a_path_that_climbs_out_of_the_project() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = project_with_a_sibling_outside_the_root(dir.path())?;
    fs::write(
        root.join("default.project.json"),
        r#"{
            "name": "Game",
            "tree": {
                "$className": "DataModel",
                "ReplicatedStorage": { "Shared": { "$path": "../outside" } }
            }
        }"#,
    )?;

    commands::init(&root, true).expect_err("a tree path outside the project is refused");
    assert!(!root.join("naht.toml").exists());
    Ok(())
}

#[test]
fn build_rejects_a_tree_path_that_climbs_out_of_the_project() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = project_with_a_sibling_outside_the_root(dir.path())?;
    tree_config(&root, "../outside")?;
    let place = dir.path().join("game.rbxl");

    commands::build(&root, &place).expect_err("a tree path outside the project is refused");
    assert!(!place.exists());
    Ok(())
}

#[test]
fn build_rejects_an_absolute_tree_path() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = project_with_a_sibling_outside_the_root(dir.path())?;
    let outside = dir.path().join("outside");
    tree_config(&root, &outside.to_string_lossy())?;
    let place = dir.path().join("game.rbxl");

    commands::build(&root, &place).expect_err("an absolute tree path is refused");
    assert!(!place.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn build_rejects_a_tree_path_that_leaves_the_project_through_a_symlink() -> io::Result<()> {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir()?;
    let root = project_with_a_sibling_outside_the_root(dir.path())?;
    symlink(dir.path().join("outside"), root.join("src").join("link"))?;
    tree_config(&root, "src/link")?;
    let place = dir.path().join("game.rbxl");

    commands::build(&root, &place).expect_err("a symlink leaving the project is refused");
    assert!(!place.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn build_rejects_a_tree_path_naming_a_windows_device() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = project_with_a_sibling_outside_the_root(dir.path())?;
    fs::create_dir_all(root.join("src").join("NUL"))?;
    fs::write(root.join("src").join("NUL").join("Mod.luau"), "return 1")?;
    tree_config(&root, "src/NUL")?;
    let place = dir.path().join("game.rbxl");

    assert!(matches!(
        commands::build(&root, &place),
        Err(CommandError::TreePathNotPortable { .. })
    ));
    assert!(!place.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn from_rojo_rejects_a_path_naming_a_windows_device() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("game");
    fs::create_dir_all(root.join("src").join("CON"))?;
    fs::write(
        root.join("default.project.json"),
        r#"{
            "name": "Game",
            "tree": {
                "$className": "DataModel",
                "ReplicatedStorage": { "Shared": { "$path": "src/CON" } }
            }
        }"#,
    )?;

    assert!(matches!(
        commands::init(&root, true),
        Err(CommandError::TreePathNotPortable { .. })
    ));
    assert!(!root.join("naht.toml").exists());
    Ok(())
}
