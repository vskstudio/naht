use super::*;
use naht_core::hash::source_fingerprint;
use naht_core::inventory::InventoryEntry;

struct CaseFoldingVfs {
    inner: MemoryVfs,
    refused_write: Option<PathBuf>,
}

impl CaseFoldingVfs {
    fn spelled(&self, path: &Path) -> PathBuf {
        let Some(parent) = path.parent() else {
            return path.to_path_buf();
        };
        let folded = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase());
        self.inner
            .list(parent)
            .ok()
            .and_then(|entries| {
                entries.into_iter().find(|entry| {
                    entry
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_lowercase())
                        == folded
                })
            })
            .map_or_else(|| path.to_path_buf(), |entry| entry.path)
    }
}

impl Vfs for CaseFoldingVfs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.inner.read(&self.spelled(path))
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        self.inner.list(&self.spelled(path))
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if self.refused_write.as_deref() == Some(path) {
            return Err(VfsError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            });
        }
        let target = self.spelled(path);
        self.inner.write(&target, contents)
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        let target = self.spelled(path);
        self.inner.create_dir(&target)
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        let target = self.spelled(path);
        self.inner.remove(&target)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), VfsError> {
        let source = self.spelled(from);
        let contents = self.inner.read(&source)?;
        self.inner.remove(&source)?;
        self.inner.write(to, &contents)
    }

    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(&self.spelled(path))
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        self.inner.kind(&self.spelled(path))
    }
}

fn session_over_case_folding_disk(refused_write: Option<&str>) -> Session<CaseFoldingVfs> {
    let mut inner = MemoryVfs::new();
    inner.write(Path::new("foo.luau"), b"old").unwrap();
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, "foo.luau", "ModuleScript", "old").unwrap();
    let vfs = CaseFoldingVfs {
        inner,
        refused_write: refused_write.map(PathBuf::from),
    };
    Session::assemble(RootedVfs::new("", vfs), store, "demo", None, false).unwrap()
}

fn root_names<V: Vfs>(session: &Session<V>) -> Vec<String> {
    session
        .vfs
        .list(Path::new(""))
        .unwrap()
        .into_iter()
        .map(|entry| entry.path.to_string_lossy().into_owned())
        .collect()
}

fn inventory_entry(path: &str, class: &str, content: &str) -> InventoryEntry {
    InventoryEntry {
        path: path.to_owned(),
        class: class.to_owned(),
        hash: source_fingerprint(content.as_bytes()),
    }
}

fn session_with_synced_scripts(root: &Path, scripts: &[(&str, &str)]) -> Session {
    let store = StateStore::open_in_memory().unwrap();
    for (path, content) in scripts {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, content).unwrap();
        set_base(&store, path, "ModuleScript", content).unwrap();
    }
    Session::new(root, store, "demo", None, false).unwrap()
}

#[test]
fn an_inventory_removes_the_old_file_of_a_rename_made_while_stopped() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[("Old.luau", "return 1")]);

    session
        .apply_inventory(&[inventory_entry("New.luau", "ModuleScript", "return 1")])
        .unwrap();

    assert_eq!(root_names(&session), ["New.luau"]);
    assert_eq!(
        fs::read_to_string(root.path().join("New.luau")).unwrap(),
        "return 1"
    );
}

#[test]
fn an_inventory_leaves_a_script_studio_created_alone() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[]);

    session
        .apply_inventory(&[inventory_entry("Fresh.luau", "ModuleScript", "return 2")])
        .unwrap();

    assert_eq!(root_names(&session), Vec::<String>::new());
}

#[test]
fn an_inventory_leaves_a_script_studio_deleted_alone() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(
        root.path(),
        &[("Keep.luau", "return 1"), ("Gone.luau", "return 2")],
    );

    session
        .apply_inventory(&[inventory_entry("Keep.luau", "ModuleScript", "return 1")])
        .unwrap();

    assert_eq!(root_names(&session), ["Gone.luau", "Keep.luau"]);
}

#[test]
fn an_inventory_leaves_a_content_edit_alone() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[("Old.luau", "return 1")]);

    session
        .apply_inventory(&[inventory_entry("Old.luau", "ModuleScript", "return 2")])
        .unwrap();

    assert_eq!(
        fs::read_to_string(root.path().join("Old.luau")).unwrap(),
        "return 1"
    );
}

#[test]
fn an_inventory_does_not_pair_scripts_of_different_classes() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[("Old.luau", "return 1")]);

    session
        .apply_inventory(&[inventory_entry("New.luau", "Script", "return 1")])
        .unwrap();

    assert_eq!(root_names(&session), ["Old.luau"]);
}

#[test]
fn an_inventory_keeps_a_lone_init_folder_in_its_layout() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[("Dir/init.luau", "return 3")]);

    session
        .apply_inventory(&[inventory_entry("Dir.luau", "ModuleScript", "return 3")])
        .unwrap();

    assert_eq!(root_names(&session), ["Dir"]);
    assert_eq!(
        fs::read_to_string(root.path().join("Dir/init.luau")).unwrap(),
        "return 3"
    );
}

#[test]
fn an_empty_inventory_removes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_scripts(root.path(), &[("Old.luau", "return 1")]);

    session.apply_inventory(&[]).unwrap();

    assert_eq!(root_names(&session), ["Old.luau"]);
}

#[test]
fn a_case_only_rename_moves_the_file_instead_of_deleting_it() {
    let mut session = session_over_case_folding_disk(None);

    session
        .apply_changes(vec![
            Change::Delete {
                path: "foo.luau".to_owned(),
            },
            upsert("Foo.luau", "new"),
        ])
        .unwrap();

    assert_eq!(root_names(&session), ["Foo.luau"]);
    assert_eq!(session.vfs.read(Path::new("Foo.luau")).unwrap(), b"new");
}

#[test]
fn a_failed_batch_undoes_a_case_only_rename() {
    let mut session = session_over_case_folding_disk(Some("Bad.luau"));

    let outcome = session.apply_changes(vec![
        Change::Delete {
            path: "foo.luau".to_owned(),
        },
        upsert("Foo.luau", "new"),
        upsert("Bad.luau", "bad"),
    ]);

    assert!(outcome.is_err());
    assert_eq!(root_names(&session), ["foo.luau"]);
    assert_eq!(session.vfs.read(Path::new("foo.luau")).unwrap(), b"old");
}

#[test]
fn a_case_only_pair_over_two_distinct_files_keeps_both_apart() {
    let mut inner = MemoryVfs::new();
    inner.write(Path::new("foo.luau"), b"old").unwrap();
    inner.write(Path::new("Foo.luau"), b"other").unwrap();
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, "foo.luau", "ModuleScript", "old").unwrap();
    set_base(&store, "Foo.luau", "ModuleScript", "other").unwrap();
    let mut session =
        Session::assemble(RootedVfs::new("", inner), store, "demo", None, false).unwrap();

    session
        .apply_changes(vec![
            Change::Delete {
                path: "foo.luau".to_owned(),
            },
            upsert("Foo.luau", "new"),
        ])
        .unwrap();

    assert_eq!(root_names(&session), ["Foo.luau"]);
    assert_eq!(session.vfs.read(Path::new("Foo.luau")).unwrap(), b"new");
}

#[test]
fn a_case_only_folder_rename_moves_the_folder_on_disk() {
    let mut inner = MemoryVfs::new();
    inner.create_dir(Path::new("Utils")).unwrap();
    inner.write(Path::new("Utils/a.luau"), b"old").unwrap();
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, "Utils/a.luau", "ModuleScript", "old").unwrap();
    let mut session = Session::assemble(
        RootedVfs::new(
            "",
            CaseFoldingVfs {
                inner,
                refused_write: None,
            },
        ),
        store,
        "demo",
        None,
        false,
    )
    .unwrap();

    session
        .apply_changes(vec![
            Change::Delete {
                path: "Utils/a.luau".to_owned(),
            },
            upsert("utils/a.luau", "new"),
        ])
        .unwrap();

    assert_eq!(root_names(&session), ["utils"]);
    assert_eq!(session.vfs.read(Path::new("utils/a.luau")).unwrap(), b"new");
}

#[test]
fn a_case_only_folder_rename_onto_another_folder_is_refused() {
    let mut inner = MemoryVfs::new();
    inner.create_dir(Path::new("Utils")).unwrap();
    inner.write(Path::new("Utils/a.luau"), b"old").unwrap();
    inner.create_dir(Path::new("utils")).unwrap();
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, "Utils/a.luau", "ModuleScript", "old").unwrap();
    let mut session =
        Session::assemble(RootedVfs::new("", inner), store, "demo", None, false).unwrap();

    let outcome = session.apply_changes(vec![
        Change::Delete {
            path: "Utils/a.luau".to_owned(),
        },
        upsert("utils/a.luau", "new"),
    ]);

    assert!(matches!(
        outcome,
        Err(SessionError::FolderRenameBlocked { .. })
    ));
    assert_eq!(root_names(&session), ["Utils", "utils"]);
}
