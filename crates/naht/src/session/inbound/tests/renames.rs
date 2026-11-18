use super::*;

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

fn inventory_script(path: &str, content: &str) -> TextInstance {
    TextInstance::new(
        path.to_owned(),
        "ModuleScript".to_owned(),
        content.to_owned(),
    )
}

fn session_with_synced_old_script(root: &Path) -> Session {
    fs::write(root.join("Old.luau"), "return 1").unwrap();
    let store = StateStore::open_in_memory().unwrap();
    set_base(&store, "Old.luau", "ModuleScript", "return 1").unwrap();
    Session::new(root, store, "demo", None, false).unwrap()
}

#[test]
fn an_inventory_removes_the_old_file_of_a_rename_made_while_stopped() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_old_script(root.path());

    session
        .apply_inventory(&[inventory_script("New.luau", "return 1")])
        .unwrap();

    assert_eq!(root_names(&session), ["New.luau"]);
    assert_eq!(
        fs::read_to_string(root.path().join("New.luau")).unwrap(),
        "return 1"
    );
}

#[test]
fn an_inventory_writes_a_script_studio_has_that_the_daemon_lacks() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();

    session
        .apply_inventory(&[inventory_script("Fresh.luau", "return 2")])
        .unwrap();

    assert_eq!(root_names(&session), ["Fresh.luau"]);
}

#[test]
fn an_empty_inventory_removes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_with_synced_old_script(root.path());

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
