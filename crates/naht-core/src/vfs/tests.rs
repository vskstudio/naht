use super::*;

#[test]
fn memory_write_then_read_round_trips() {
    let mut vfs = MemoryVfs::new();
    let path = Path::new("src/main.luau");
    vfs.write(path, b"print('hi')").unwrap();
    assert_eq!(vfs.read(path).unwrap(), b"print('hi')");
}

#[test]
fn writing_a_file_creates_its_parent_dirs() {
    let mut vfs = MemoryVfs::new();
    vfs.write(Path::new("a/b/c.luau"), b"x").unwrap();
    assert_eq!(vfs.kind(Path::new("a")).unwrap(), EntryKind::Dir);
    assert_eq!(vfs.kind(Path::new("a/b")).unwrap(), EntryKind::Dir);
}

#[test]
fn list_returns_sorted_immediate_children_only() {
    let vfs = MemoryVfs::new()
        .with_file("proj/a.luau", "a")
        .with_file("proj/sub/b.luau", "b")
        .with_dir("proj/empty");
    let entries = vfs.list(Path::new("proj")).unwrap();
    let names: Vec<_> = entries.iter().map(|entry| entry.path.clone()).collect();
    assert_eq!(
        names,
        vec![
            PathBuf::from("proj/a.luau"),
            PathBuf::from("proj/empty"),
            PathBuf::from("proj/sub"),
        ]
    );
}

#[test]
fn read_on_a_dir_is_not_a_file() {
    let vfs = MemoryVfs::new().with_dir("proj");
    assert!(matches!(
        vfs.read(Path::new("proj")),
        Err(VfsError::NotAFile(_))
    ));
}

#[test]
fn remove_refuses_a_directory_that_still_has_files() {
    let mut vfs = MemoryVfs::new()
        .with_file("proj/sub/a.luau", "a")
        .with_file("proj/keep.luau", "k");
    assert!(vfs.remove(Path::new("proj/sub")).is_err());
    assert!(vfs.exists(Path::new("proj/sub/a.luau")));
    assert!(vfs.exists(Path::new("proj/keep.luau")));
}

#[test]
fn remove_takes_an_empty_directory() {
    let mut vfs = MemoryVfs::new().with_dir("proj/empty");
    vfs.remove(Path::new("proj/empty")).unwrap();
    assert!(!vfs.exists(Path::new("proj/empty")));
}

#[test]
fn missing_read_is_not_found() {
    let vfs = MemoryVfs::new();
    assert!(matches!(
        vfs.read(Path::new("nope.luau")),
        Err(VfsError::NotFound(_))
    ));
}

#[test]
fn source_paths_accept_the_luau_families_the_mapper_reads() {
    for path in [
        "Greeter.luau",
        "runtime/main.server.luau",
        "ui/hud.client.luau",
        "folder/init.luau",
        "folder/init.server.luau",
    ] {
        assert_eq!(validate_source_path(path), Ok(()), "{path}");
    }
}

#[test]
fn source_paths_reject_dotted_components() {
    for path in [
        ".naht/state.db",
        ".naht/x.luau",
        "src/.hidden.luau",
        ".env.luau",
    ] {
        assert_eq!(
            validate_source_path(path),
            Err(ProjectPathError::Hidden),
            "{path}"
        );
    }
}

#[test]
fn source_paths_reject_files_that_map_to_no_instance() {
    for path in [
        "naht.toml",
        "default.project.json",
        "x.txt",
        "x",
        "Terrain.terrain",
        "x.luau/notes.md",
    ] {
        assert_eq!(
            validate_source_path(path),
            Err(ProjectPathError::Unmapped),
            "{path}"
        );
    }
}

#[test]
fn blob_paths_accept_only_terrain_blobs() {
    assert_eq!(validate_blob_path("Terrain.terrain"), Ok(()));
    assert_eq!(validate_blob_path("world/Map.terrain"), Ok(()));
    for path in ["naht.toml", "x.luau", "x.txt", "x"] {
        assert_eq!(
            validate_blob_path(path),
            Err(ProjectPathError::Unmapped),
            "{path}"
        );
    }
    assert_eq!(
        validate_blob_path(".naht/state.terrain"),
        Err(ProjectPathError::Hidden)
    );
}

#[test]
fn windows_device_names_are_rejected_on_every_platform() {
    for path in [
        "CON.luau",
        "nul.server.luau",
        "Com1.client.luau",
        "src/LPT9.luau",
        "AUX.luau",
        "prn.luau",
        "dir/CON/x.luau",
        "COM\u{b9}.luau",
        "CONIN$.luau",
        "conout$.server.luau",
        "CLOCK$.luau",
        "COM0.luau",
        "lpt0.client.luau",
    ] {
        assert_eq!(
            validate_source_path(path),
            Err(ProjectPathError::ReservedName),
            "{path}"
        );
    }
    assert_eq!(
        validate_blob_path("NUL.terrain"),
        Err(ProjectPathError::ReservedName)
    );
}

#[test]
fn names_that_only_start_with_a_device_name_are_accepted() {
    assert_eq!(validate_source_path("CONSOLE.luau"), Ok(()));
    assert_eq!(validate_source_path("Comment.luau"), Ok(()));
}

#[test]
fn rename_moves_a_file_over_the_one_at_the_target() {
    let mut vfs = MemoryVfs::new()
        .with_file("src/foo.luau", "new")
        .with_file("src/Foo.luau", "old");
    vfs.rename(Path::new("src/foo.luau"), Path::new("src/Foo.luau"))
        .unwrap();
    assert!(!vfs.exists(Path::new("src/foo.luau")));
    assert_eq!(vfs.read(Path::new("src/Foo.luau")).unwrap(), b"new");
}

#[test]
fn rename_refuses_a_directory_on_either_side() {
    let mut vfs = MemoryVfs::new()
        .with_file("src/Folder/A.luau", "a")
        .with_file("src/plain.luau", "p");
    assert!(matches!(
        vfs.rename(Path::new("src/Folder"), Path::new("src/Moved")),
        Err(VfsError::NotAFile(_))
    ));
    assert!(matches!(
        vfs.rename(Path::new("src/plain.luau"), Path::new("src/Folder")),
        Err(VfsError::NotAFile(_))
    ));
    assert!(vfs.exists(Path::new("src/plain.luau")));
}
