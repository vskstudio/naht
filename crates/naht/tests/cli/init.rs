use std::path::Path;

use naht::commands;
use naht::config::Config;
use naht_core::reconciler;
use naht_core::vfs::{DiskVfs, RootedVfs};
use std::fs;

#[test]
fn init_scaffolds_a_working_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    commands::init(&root, false).unwrap();

    assert!(root.join("naht.toml").exists());
    assert!(root.join("src").join("Hello.luau").exists());
    let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|line| line.trim() == "/.naht"));

    let config = Config::load(&root).unwrap();
    assert_eq!(config.project_name(&root), "proj");

    let vfs = RootedVfs::new(root.canonicalize().unwrap(), DiskVfs::new());
    let scanned = reconciler::scan_text(&vfs, Path::new("")).unwrap();
    assert!(scanned.keys().any(|key| key.ends_with("Hello.luau")));
}

#[test]
fn init_refuses_to_overwrite_an_existing_project() {
    let dir = tempfile::tempdir().unwrap();
    commands::init(dir.path(), false).unwrap();
    commands::init(dir.path(), false).expect_err("init refuses an existing project");
}

#[test]
fn init_from_rojo_converts_name_and_place_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("default.project.json"),
        r#"{ "name": "MyGame", "servePlaceIds": [123456], "tree": { "$className": "DataModel" } }"#,
    )
    .unwrap();

    commands::init(&root, true).unwrap();

    let config = Config::load(&root).unwrap();
    assert_eq!(config.project.name.as_deref(), Some("MyGame"));
    assert_eq!(config.serve.place_id, Some(123_456));
}
