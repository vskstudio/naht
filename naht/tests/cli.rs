//! Acceptance tests for the Stage 5 commands, driving `naht::commands` directly over temp project
//! directories.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use naht::commands;
use naht::config::Config;
use naht_core::reconciler::{self, TextInstance};
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};

/// Class names of the reloaded place's `DataModel` children.
fn root_classes(dom: &rbx_dom_weak::WeakDom) -> Vec<String> {
    dom.root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap().class.as_str().to_string())
        .collect()
}

fn one(path: &str, content: &str) -> BTreeMap<String, TextInstance> {
    let mut map = BTreeMap::new();
    map.insert(
        path.to_string(),
        TextInstance::new(path, "ModuleScript", content),
    );
    map
}

#[test]
fn init_scaffolds_a_working_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    commands::init(&root, false).unwrap();

    assert!(root.join("naht.toml").exists());
    assert!(root.join("src").join("Hello.luau").exists());
    let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|line| line.trim() == "/.naht"));

    // The config loads and the name defaults to the directory.
    let config = Config::load(&root).unwrap();
    assert_eq!(config.project_name(&root), "proj");

    // The scaffolded source is discoverable by the reconciler's scan.
    let vfs = RootedVfs::new(root.canonicalize().unwrap(), DiskVfs::new());
    let scanned = reconciler::scan_text(&vfs, Path::new("")).unwrap();
    assert!(scanned.keys().any(|key| key.ends_with("Hello.luau")));
}

#[test]
fn init_refuses_to_overwrite_an_existing_project() {
    let dir = tempfile::tempdir().unwrap();
    commands::init(dir.path(), false).unwrap();
    assert!(commands::init(dir.path(), false).is_err());
}

#[test]
fn init_from_rojo_converts_name_and_place_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("default.project.json"),
        r#"{ "name": "MyGame", "servePlaceIds": [123456], "tree": { "$className": "DataModel" } }"#,
    )
    .unwrap();

    commands::init(&root, true).unwrap();

    let config = Config::load(&root).unwrap();
    assert_eq!(config.project.name.as_deref(), Some("MyGame"));
    assert_eq!(config.serve.place_id, Some(123456));
}

#[test]
fn build_produces_a_reloadable_model() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    commands::init(&root, false).unwrap();
    std::fs::write(root.join("src").join("Mod.luau"), "return 2").unwrap();

    let output = dir.path().join("out.rbxm");
    commands::build(&root, &output).unwrap();
    assert!(output.exists());

    let bytes = std::fs::read(&output).unwrap();
    let dom = rbx_binary::from_reader(&bytes[..]).unwrap();
    let names: Vec<_> = dom
        .root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap().name.clone())
        .collect();
    // The `src` folder is present; the internal `.naht` dir never is.
    assert!(names.contains(&"src".to_string()));
    assert!(!names.contains(&".naht".to_string()));
}

#[test]
fn build_place_maps_top_level_dirs_to_services() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(root.join("ServerScriptService")).unwrap();
    std::fs::write(
        root.join("ServerScriptService").join("Main.server.luau"),
        "print(1)",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("ReplicatedStorage")).unwrap();
    std::fs::write(
        root.join("ReplicatedStorage").join("Shared.luau"),
        "return {}",
    )
    .unwrap();
    std::fs::write(root.join("Loose.luau"), "return 0").unwrap();

    // A place output (.rbxl) maps top-level service directories to services.
    let place = dir.path().join("game.rbxl");
    commands::build(&root, &place).unwrap();
    let place_dom = rbx_binary::from_reader(&std::fs::read(&place).unwrap()[..]).unwrap();
    let classes = root_classes(&place_dom);
    assert!(classes.contains(&"ServerScriptService".to_string()));
    assert!(classes.contains(&"ReplicatedStorage".to_string()));
    assert!(classes.contains(&"Workspace".to_string())); // the loose file lands here

    // A model output (.rbxm) of the same project stays a bare instance list: the directory is a
    // plain Folder, not a service. (Stage 5 behavior unchanged.)
    let model = dir.path().join("game.rbxm");
    commands::build(&root, &model).unwrap();
    let model_dom = rbx_binary::from_reader(&std::fs::read(&model).unwrap()[..]).unwrap();
    assert!(root_classes(&model_dom)
        .iter()
        .all(|class| class != "ServerScriptService"));
}

#[test]
fn build_watch_rebuilds_the_output_on_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("A.luau"), "return 1").unwrap();

    let output = dir.path().join("out.rbxm");
    commands::build(&root, &output).unwrap();
    let _watcher = commands::spawn_build_watcher(&root, &output).unwrap();

    // Add a second source file; the watcher should rebuild the output to include it.
    std::fs::write(root.join("src").join("B.luau"), "return 2").unwrap();

    let mut rebuilt = false;
    for _ in 0..100 {
        if let Ok(bytes) = std::fs::read(&output) {
            if let Ok(dom) = rbx_binary::from_reader(&bytes[..]) {
                let src = dom
                    .root()
                    .children()
                    .iter()
                    .map(|r| dom.get_by_ref(*r).unwrap())
                    .find(|instance| instance.name == "src");
                if src.map(|folder| folder.children().len()) == Some(2) {
                    rebuilt = true;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        rebuilt,
        "the watcher should have rebuilt the output with the new file"
    );
}

#[test]
fn status_and_resolve_reflect_the_conflict_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let rel = "src/m.luau";
    std::fs::write(root.join("src").join("m.luau"), "local a = 1\n").unwrap();

    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let mut vfs = RootedVfs::new(root.clone(), DiskVfs::new());
    // Set the base, then drive an overlapping change on both sides to freeze the path.
    reconciler::reconcile(
        &mut vfs,
        &store,
        &one(rel, "local a = 1\n"),
        &one(rel, "local a = 1\n"),
    )
    .unwrap();
    reconciler::reconcile(
        &mut vfs,
        &store,
        &one(rel, "local a = 11\n"),
        &one(rel, "local a = 22\n"),
    )
    .unwrap();
    assert_eq!(
        reconciler::status(&store).unwrap().conflicted,
        vec![rel.to_string()]
    );

    // `status` runs cleanly against the conflicted project.
    commands::status(&root).unwrap();

    // `resolve` refuses while markers remain, then succeeds once they are gone.
    assert!(commands::resolve(&root, rel).is_err());
    std::fs::write(root.join("src").join("m.luau"), "local a = 99\n").unwrap();
    commands::resolve(&root, rel).unwrap();

    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    assert!(reconciler::status(&store).unwrap().conflicted.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_fails_clearly_when_no_daemon_is_running() {
    let dir = tempfile::tempdir().unwrap();
    // Point at a port nothing listens on, so the connection is refused deterministically.
    std::fs::write(dir.path().join("naht.toml"), "[serve]\nport = 1\n").unwrap();
    let config = Config::load(dir.path()).unwrap();
    assert!(commands::pull(&config).await.is_err());
}
