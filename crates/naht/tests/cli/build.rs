use std::time::Duration;

use naht::commands;
use naht_core::Snapshot;

use super::support::{read_place, root_classes};

#[test]
fn build_produces_a_reloadable_model() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    commands::init(&root, false).unwrap();
    std::fs::write(root.join("src").join("Mod.luau"), "return 2").unwrap();

    let output = dir.path().join("out.rbxm");
    commands::build(&root, &output).unwrap();
    assert!(output.exists());

    let dom = read_place(&output);
    let names: Vec<_> = dom
        .root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap().name.clone())
        .collect();
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

    let place = dir.path().join("game.rbxl");
    commands::build(&root, &place).unwrap();
    let classes = root_classes(&read_place(&place));
    assert!(classes.contains(&"ServerScriptService".to_string()));
    assert!(classes.contains(&"ReplicatedStorage".to_string()));
    assert!(classes.contains(&"Workspace".to_string()));

    let model = dir.path().join("game.rbxm");
    commands::build(&root, &model).unwrap();
    assert!(
        root_classes(&read_place(&model))
            .iter()
            .all(|class| class != "ServerScriptService")
    );
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

    std::fs::write(root.join("src").join("B.luau"), "return 2").unwrap();

    let mut rebuilt = false;
    for _ in 0..100 {
        if let Ok(bytes) = std::fs::read(&output)
            && let Ok(dom) = rbx_binary::from_reader(&bytes[..])
        {
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
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        rebuilt,
        "the watcher should have rebuilt the output with the new file"
    );
}

#[test]
fn cli_place_build_root_is_a_data_model() {
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
    let dom = read_place(&place);

    assert_eq!(dom.get_by_ref(dom.root_ref()).unwrap().class, "DataModel");
    let classes = root_classes(&dom);
    assert!(classes.contains(&"ReplicatedStorage".to_string()));
    assert!(classes.contains(&"Workspace".to_string()));
}

#[test]
fn assets_disabled_build_is_byte_identical_and_rewrites_nothing() {
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
