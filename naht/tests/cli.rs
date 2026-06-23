//! Acceptance tests for the Stage 5 commands, driving `naht::commands` directly over temp project
//! directories.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use naht::commands;
use naht::config::Config;
use naht_core::limits::Reason;
use naht_core::reconciler::{self, TextInstance};
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};
use naht_core::Snapshot;

/// Class names of the reloaded place's `DataModel` children.
fn root_classes(dom: &rbx_dom_weak::WeakDom) -> Vec<String> {
    dom.root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap().class.as_str().to_string())
        .collect()
}

/// A reloaded place's service instance of the given class, if present.
fn service<'a>(dom: &'a rbx_dom_weak::WeakDom, class: &str) -> Option<&'a rbx_dom_weak::Instance> {
    dom.root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|instance| instance.class == class)
}

/// A child of `instance` with the given name, if present.
fn child<'a>(
    dom: &'a rbx_dom_weak::WeakDom,
    instance: &rbx_dom_weak::Instance,
    name: &str,
) -> Option<&'a rbx_dom_weak::Instance> {
    instance
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|child| child.name == name)
}

/// A reloaded instance's property value by key, if present.
fn property<'a>(
    instance: &'a rbx_dom_weak::Instance,
    key: &str,
) -> Option<&'a rbx_dom_weak::types::Variant> {
    instance
        .properties
        .iter()
        .find(|(k, _)| k.as_str() == key)
        .map(|(_, v)| v)
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

#[test]
fn serve_warning_honors_the_terrain_sync_config_flag() {
    let tree = Snapshot::new("Folder", "game")
        .with_child(Snapshot::new("Terrain", "Terrain"))
        .with_child(Snapshot::new("MeshPart", "Rock"));

    // Enabled via `[serve] terrain_sync`: the daemon drives the blob channel, so terrain is no longer
    // flagged — but the mesh binary still is. This exercises the config → serve wiring, not just the
    // `limits` unit.
    let enabled_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        enabled_dir.path().join("naht.toml"),
        "[serve]\nterrain_sync = true\n",
    )
    .unwrap();
    let enabled = Config::load(enabled_dir.path()).unwrap();
    let reasons: Vec<_> = commands::serve_warnings(&enabled, &tree)
        .into_iter()
        .map(|w| w.reason)
        .collect();
    assert!(!reasons.contains(&Reason::Terrain));
    assert!(reasons.contains(&Reason::MeshBinary));

    // Disabled (the default): terrain still fires the Stage 7 warning.
    let disabled_dir = tempfile::tempdir().unwrap();
    let disabled = Config::load(disabled_dir.path()).unwrap();
    let reasons: Vec<_> = commands::serve_warnings(&disabled, &tree)
        .into_iter()
        .map(|w| w.reason)
        .collect();
    assert!(reasons.contains(&Reason::Terrain));
}

#[test]
fn cli_place_build_root_is_a_data_model() {
    // Stage 9 criterion 1, through the CLI: a `.rbxl` build is a place whose root is a `DataModel`
    // directly parenting services — not a bare model instance list.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("game");
    std::fs::create_dir_all(root.join("ReplicatedStorage")).unwrap();
    std::fs::write(
        root.join("ReplicatedStorage").join("Shared.luau"),
        "return {}",
    )
    .unwrap();
    std::fs::write(root.join("Loose.luau"), "return 0").unwrap();

    let place = dir.path().join("game.rbxl");
    commands::build(&root, &place).unwrap();
    let dom = rbx_binary::from_reader(&std::fs::read(&place).unwrap()[..]).unwrap();

    // The reloaded root stands in for the DataModel, and the services are its direct children.
    assert_eq!(dom.get_by_ref(dom.root_ref()).unwrap().class, "DataModel");
    let classes = root_classes(&dom);
    assert!(classes.contains(&"ReplicatedStorage".to_string()));
    assert!(classes.contains(&"Workspace".to_string()));
}

#[test]
fn assets_disabled_build_is_byte_identical_and_rewrites_nothing() {
    // Stage 12 criterion 4. With `[assets]` off (the default), the build does no network I/O and
    // rewrites no property, so it is fully deterministic.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    commands::init(&root, false).unwrap();
    std::fs::write(root.join("src").join("Mod.luau"), "return 2").unwrap();

    let first = dir.path().join("a.rbxm");
    let second = dir.path().join("b.rbxm");
    commands::build(&root, &first).unwrap();
    commands::build(&root, &second).unwrap();
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap(),
        "two assets-disabled builds of the same project must be byte-identical"
    );

    // Snapshot-level: a property that *would* be uploaded if assets were on is left exactly as-is.
    let mut snapshot = Snapshot::new("Folder", "proj").with_child(
        Snapshot::new("MeshPart", "Rock").with_property(
            "MeshId",
            rbx_dom_weak::types::Variant::String("meshes/rock.obj".to_string()),
        ),
    );
    commands::resolve_assets(&root, &mut snapshot).unwrap();
    let rock = snapshot.children.iter().find(|c| c.name == "Rock").unwrap();
    assert_eq!(
        rock.properties.get("MeshId"),
        Some(&rbx_dom_weak::types::Variant::String(
            "meshes/rock.obj".to_string()
        )),
        "assets disabled must not rewrite the reference"
    );
}

#[test]
fn from_rojo_migrates_the_tree_so_the_build_matches_rojo() {
    // Stage 17 criterion 1: a Rojo `$path` that the directory convention does not already place is
    // migrated so `naht build` produces the same instance tree Rojo would.
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
    let dom = rbx_binary::from_reader(&std::fs::read(&place).unwrap()[..]).unwrap();

    let rs = service(&dom, "ReplicatedStorage").expect("ReplicatedStorage service");
    let common = child(&dom, rs, "Common").expect("Common under ReplicatedStorage");
    assert_eq!(common.class, "Folder");
    let mut mapped: Vec<(String, String)> = common
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .map(|i| (i.name.clone(), i.class.as_str().to_string()))
        .collect();
    mapped.sort();
    assert_eq!(
        mapped,
        vec![
            ("Boot".to_string(), "Script".to_string()),
            ("Mod".to_string(), "ModuleScript".to_string()),
            ("Ui".to_string(), "LocalScript".to_string()),
        ]
    );
    // The `src` source root is grafted at its instance location, not left as a literal folder.
    let workspace = service(&dom, "Workspace");
    assert!(workspace.is_none_or(|ws| child(&dom, ws, "src").is_none()));
}

#[test]
fn from_rojo_carries_classname_and_properties_with_a_round_trip() {
    // Stage 17 criterion 2: a `$className` / `$properties` the convention cannot express is carried
    // into the migrated project and survives the build.
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
    let dom = rbx_binary::from_reader(&std::fs::read(&place).unwrap()[..]).unwrap();

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
    // Stage 17 criterion 3: anything Naht cannot represent is reported, never dropped silently.
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

#[test]
fn package_plugin_produces_a_loadable_rbxmx_with_the_entry_script() {
    // Stage 18: the plugin packs into one installable `.rbxmx` that loads headless (rbx_xml) with the
    // entry Script and its sibling modules under a single `Naht` folder.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("Plugin.server.luau"), "-- entry point\nreturn nil").unwrap();
    std::fs::write(src.join("Apply.luau"), "return {}").unwrap();
    std::fs::write(src.join("Client.luau"), "return {}").unwrap();

    let out = dir.path().join("naht-plugin.rbxmx");
    commands::package_plugin(&src, &out).unwrap();

    let dom = rbx_xml::from_reader_default(&std::fs::read(&out).unwrap()[..]).unwrap();
    let naht = dom
        .root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|i| i.name == "Naht")
        .expect("single top-level Naht folder");
    assert_eq!(naht.class, "Folder");

    let plugin = child(&dom, naht, "Plugin").expect("entry Plugin script under Naht");
    assert_eq!(plugin.class, "Script");
    assert_eq!(
        child(&dom, naht, "Apply").map(|m| m.class.as_str().to_string()),
        Some("ModuleScript".to_string())
    );
}

#[test]
fn check_release_version_guards_the_tag_against_the_workspace_version() {
    let version = naht_core::version();
    // The matching tag passes, with or without the `v` prefix.
    assert!(commands::check_release_version(&format!("v{version}")).is_ok());
    assert!(commands::check_release_version(version).is_ok());
    // A mismatched tag fails the job.
    assert!(commands::check_release_version("v0.0.0-not-the-version").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_fails_clearly_when_no_daemon_is_running() {
    let dir = tempfile::tempdir().unwrap();
    // Point at a port nothing listens on, so the connection is refused deterministically.
    std::fs::write(dir.path().join("naht.toml"), "[serve]\nport = 1\n").unwrap();
    let config = Config::load(dir.path()).unwrap();
    assert!(commands::pull(&config).await.is_err());
}
