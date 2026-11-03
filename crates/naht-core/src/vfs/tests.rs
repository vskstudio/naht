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
fn remove_dir_takes_its_subtree() {
    let mut vfs = MemoryVfs::new()
        .with_file("proj/sub/a.luau", "a")
        .with_file("proj/keep.luau", "k");
    vfs.remove(Path::new("proj/sub")).unwrap();
    assert!(!vfs.exists(Path::new("proj/sub")));
    assert!(!vfs.exists(Path::new("proj/sub/a.luau")));
    assert!(vfs.exists(Path::new("proj/keep.luau")));
}

#[test]
fn missing_read_is_not_found() {
    let vfs = MemoryVfs::new();
    assert!(matches!(
        vfs.read(Path::new("nope.luau")),
        Err(VfsError::NotFound(_))
    ));
}
