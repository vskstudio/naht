use super::*;
use crate::vfs::{MemoryVfs, validate_project_path};

fn fixture() -> MemoryVfs {
    MemoryVfs::new()
        .with_file("proj/Greeter.luau", "return 1")
        .with_file("proj/runtime/main.server.luau", "print('server')")
        .with_file("proj/runtime/ui.client.luau", "print('client')")
        .with_file(
            "proj/disabled.server.luau",
            "--!naht { Disabled = true }\nprint('x')",
        )
        .with_file("proj/folder/init.luau", "return {}")
        .with_file("proj/folder/inner.luau", "return 2")
        .with_dir("proj/empty")
        .with_file("proj/README.md", "not an instance")
}

fn child<'a>(snapshot: &'a Snapshot, name: &str) -> &'a Snapshot {
    snapshot
        .children
        .iter()
        .find(|child| child.name == name)
        .unwrap_or_else(|| panic!("missing child {name}"))
}

#[test]
fn maps_classes_and_hierarchy() {
    let vfs = fixture();
    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
    assert_eq!(tree.class, "Folder");
    assert_eq!(tree.name, "proj");

    assert_eq!(child(&tree, "Greeter").class, "ModuleScript");

    let runtime = child(&tree, "runtime");
    assert_eq!(runtime.class, "Folder");
    assert_eq!(child(runtime, "main").class, "Script");
    assert_eq!(child(runtime, "ui").class, "LocalScript");
}

#[test]
fn non_source_file_is_skipped() {
    let vfs = fixture();
    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
    assert!(tree.children.iter().all(|child| child.name != "README.md"));
}

#[test]
fn init_collapses_directory_into_the_instance() {
    let vfs = fixture();
    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
    let folder = child(&tree, "folder");
    assert_eq!(folder.class, "ModuleScript");
    assert_eq!(folder.source(), Some("return {}"));
    assert_eq!(child(folder, "inner").class, "ModuleScript");
}

#[test]
fn frontmatter_properties_are_attached() {
    let vfs = fixture();
    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
    let disabled = child(&tree, "disabled");
    assert_eq!(disabled.class, "Script");
    assert_eq!(
        disabled.properties.get("Disabled"),
        Some(&Variant::Bool(true))
    );
    assert_eq!(disabled.source(), Some("print('x')"));
}

#[test]
fn empty_directory_maps_to_an_empty_folder() {
    let vfs = fixture();
    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
    let empty = child(&tree, "empty");
    assert_eq!(empty.class, "Folder");
    assert_eq!(empty.children.len(), 0);
}

#[test]
fn round_trip_snapshot_to_files_to_snapshot_is_identity() {
    let source = fixture();
    let tree = snapshot_dir(&source, Path::new("proj")).unwrap();

    let mut dest = MemoryVfs::new();
    write_tree(&mut dest, Path::new(""), &tree).unwrap();
    let round_tripped = snapshot_dir(&dest, Path::new("proj")).unwrap();

    assert_eq!(round_tripped, tree);
}

#[test]
fn structured_properties_survive_fs_snapshot_fs_round_trip() {
    use rbx_dom_weak::types::{Attributes, Color3, Enum, Tags, Vector3};

    let mut attributes = Attributes::new();
    attributes.insert("Health".to_owned(), Variant::Float64(100.0));
    attributes.insert("Title".to_owned(), Variant::String("hero".to_owned()));

    let project = Snapshot::new("Folder", "proj").with_child(
        Snapshot::new("ModuleScript", "Config")
            .with_property(SOURCE_PROPERTY, Variant::String("return {}".to_owned()))
            .with_property("Color", Variant::Color3(Color3::new(0.1, 0.5, 0.9)))
            .with_property("Position", Variant::Vector3(Vector3::new(1.0, 2.0, 3.0)))
            .with_property("Material", Variant::Enum(Enum::from_u32(256)))
            .with_property("Attributes", Variant::Attributes(attributes))
            .with_property(
                "Tags",
                Variant::Tags(Tags::from(vec!["combat".to_owned(), "npc".to_owned()])),
            ),
    );

    let mut dest = MemoryVfs::new();
    write_tree(&mut dest, Path::new(""), &project).unwrap();
    let round_tripped = snapshot_dir(&dest, Path::new("proj")).unwrap();

    assert_eq!(round_tripped, project);
}

#[test]
fn write_tree_rejects_instance_names_that_would_leave_the_root() {
    let unsafe_instances = [
        ("Folder", ".."),
        ("Folder", "."),
        ("Folder", ""),
        ("Folder", "a/b"),
        ("Folder", "a\\b"),
        ("Folder", "bad\0name"),
        ("Folder", "C:evil"),
        ("Folder", ".hidden"),
        ("ModuleScript", "../escape"),
        ("Script", "nested/Script"),
    ];
    for (class, name) in unsafe_instances {
        let mut vfs = MemoryVfs::new();
        let snapshot = Snapshot::new(class, name);
        assert!(
            write_tree(&mut vfs, Path::new("proj"), &snapshot).is_err(),
            "{class} named {name:?} must be rejected"
        );
    }
}

#[test]
fn write_tree_refuses_a_forbidden_character_in_a_name_and_keeps_ordinary_ones() {
    let forbidden = ('\0'..='\u{1f}').chain("<>\"|?*".chars());
    for character in forbidden {
        let name = format!("a{character}b");
        let mut vfs = MemoryVfs::new();
        let snapshot = Snapshot::new("Folder", &name);
        assert!(
            matches!(
                write_tree(&mut vfs, Path::new("proj"), &snapshot),
                Err(MapError::UnsafeName(_))
            ),
            "{name:?} must be refused"
        );
        assert!(
            validate_project_path(&format!("Dir/{name}/Leaf.luau")).is_err(),
            "the project path rule must refuse {name:?} too"
        );
    }
    for name in ["a-b_c.d", "Dé jà", "Name (2)"] {
        let mut vfs = MemoryVfs::new();
        let snapshot = Snapshot::new("Folder", name);
        assert!(
            write_tree(&mut vfs, Path::new("proj"), &snapshot).is_ok(),
            "{name:?}"
        );
        assert_eq!(
            validate_project_path(&format!("Dir/{name}/Leaf.luau")),
            Ok(())
        );
    }
}

#[test]
fn write_tree_refuses_each_windows_device_name_the_project_path_rule_refuses() {
    for name in ["CON", "nul", "Com1.v2", "lpt9 ", "CONIN$"] {
        let mut vfs = MemoryVfs::new();
        let snapshot = Snapshot::new("Folder", name);
        assert!(
            matches!(
                write_tree(&mut vfs, Path::new("proj"), &snapshot),
                Err(MapError::UnsafeName(_))
            ),
            "{name:?} must be refused"
        );
        assert!(
            validate_project_path(&format!("Dir/{name}/Leaf.luau")).is_err(),
            "the project path rule must refuse {name:?} too"
        );
    }
    for name in ["Console", "COM10", "Nulled"] {
        let mut vfs = MemoryVfs::new();
        let snapshot = Snapshot::new("Folder", name);
        assert!(
            write_tree(&mut vfs, Path::new("proj"), &snapshot).is_ok(),
            "{name:?}"
        );
    }
}

#[test]
fn snapshot_skips_dependency_and_hidden_directories_like_the_scanners() {
    let vfs = MemoryVfs::new()
        .with_file("proj/Real.luau", "return 1")
        .with_file("proj/node_modules/Dep.luau", "return 2")
        .with_file("proj/target/Build.luau", "return 3")
        .with_file("proj/.hidden/Secret.luau", "return 4")
        .with_file("proj/.init.luau", "return 5");

    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();

    let names: Vec<&str> = tree
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect();
    assert_eq!(names, vec!["Real"]);
}

#[test]
fn an_ignored_init_file_does_not_turn_its_directory_into_a_script() {
    let vfs = MemoryVfs::new()
        .with_file("proj/folder/.init.luau", "return 1")
        .with_file("proj/folder/Child.luau", "return 2");

    let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();

    let folder = child(&tree, "folder");
    assert_eq!(folder.class, FOLDER);
    assert_eq!(folder.children.len(), 1);
}
