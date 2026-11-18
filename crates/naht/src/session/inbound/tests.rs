use std::collections::BTreeMap;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use naht_core::binary::{BlobInstance, TERRAIN_CLASS};
use naht_core::protocol::Change;
use naht_core::reconciler::set_base;
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::{DirEntry, EntryKind, MemoryVfs, RootedVfs, Vfs, VfsError};
use tracing::subscriber::with_default;

use super::remove_emptied_dirs_in;
use crate::session::transaction::{DiskChange, Journal};
use crate::session::{Session, SessionError};

mod folder_shape;

fn upsert(path: &str, content: &str) -> Change {
    Change::Upsert {
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        content: content.to_owned(),
    }
}

#[test]
fn a_failed_batch_restores_the_entry_it_overwrote() {
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        false,
    )
    .unwrap();
    session
        .apply_changes(vec![upsert("Good.luau", "A")])
        .unwrap();

    let outcome = session.apply_changes(vec![
        upsert("Good.luau", "B"),
        Change::Delete {
            path: "../escape.luau".to_owned(),
        },
    ]);

    assert!(outcome.is_err());
    assert_eq!(session.studio["Good.luau"].content, "A");
}

fn session_in(root: &Path) -> Session {
    Session::new(
        root,
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        false,
    )
    .unwrap()
}

fn base_records(session: &Session) -> BTreeMap<String, InstanceRecord> {
    session
        .store
        .all()
        .unwrap()
        .into_iter()
        .map(|record| (record.guid.clone(), record))
        .collect()
}

#[cfg(unix)]
fn batch_ending_in_read_only_dir(
    session: &mut Session,
    root: &Path,
    changes: Vec<Change>,
) -> Result<(), SessionError> {
    let read_only = root.join("ro");
    fs::create_dir_all(&read_only).unwrap();
    fs::set_permissions(&read_only, fs::Permissions::from_mode(0o555)).unwrap();
    let outcome = session.apply_changes(changes);
    fs::set_permissions(&read_only, fs::Permissions::from_mode(0o755)).unwrap();
    outcome
}

#[cfg(unix)]
#[test]
fn a_failed_batch_leaves_no_disk_write_behind() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    let before = base_records(&session);

    let outcome = batch_ending_in_read_only_dir(
        &mut session,
        root.path(),
        vec![upsert("Aok.luau", "X"), upsert("ro/B.luau", "Y")],
    );

    assert!(outcome.is_err());
    assert!(!root.path().join("Aok.luau").exists());
    assert!(!session.studio.contains_key("Aok.luau"));
    assert_eq!(base_records(&session), before);
    session.rescan().unwrap();
    assert!(!root.path().join("Aok.luau").exists());
    assert_eq!(base_records(&session), before);
}

#[cfg(unix)]
#[test]
fn a_failed_batch_restores_the_file_it_updated() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session_in(root.path());
    session
        .apply_changes(vec![upsert("Aok.luau", "A")])
        .unwrap();
    let before = base_records(&session);

    let outcome = batch_ending_in_read_only_dir(
        &mut session,
        root.path(),
        vec![upsert("Aok.luau", "B"), upsert("ro/B.luau", "Y")],
    );

    assert!(outcome.is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("Aok.luau")).unwrap(),
        "A"
    );
    assert_eq!(session.studio["Aok.luau"].content, "A");
    assert_eq!(base_records(&session), before);
    session.rescan().unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("Aok.luau")).unwrap(),
        "A"
    );
    assert_eq!(base_records(&session), before);
}

fn blob(path: &str) -> BlobInstance {
    BlobInstance::new(path, TERRAIN_CLASS, vec![1_u8, 2, 3])
}

#[test]
fn a_failed_blob_batch_leaves_no_blob_on_disk_or_in_the_mirror() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Blocker"), "a file").unwrap();
    let mut session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        true,
    )
    .unwrap();

    let outcome =
        session.apply_blob_changes(vec![blob("A_Good.terrain"), blob("Blocker/B_Bad.terrain")]);

    assert!(outcome.is_err());
    assert!(!root.path().join("A_Good.terrain").exists());
    assert!(
        session
            .studio_blobs
            .as_ref()
            .is_none_or(|blobs| !blobs.contains_key("A_Good.terrain"))
    );
    session.rescan().unwrap();
    assert!(!root.path().join("A_Good.terrain").exists());
}

#[test]
fn a_failed_batch_leaves_the_activity_log_and_patch_queue_unchanged() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Blocker"), "a file").unwrap();
    fs::write(root.path().join("Other.luau"), "from disk").unwrap();
    let mut session = Session::new(
        root.path(),
        StateStore::open_in_memory().unwrap(),
        "demo",
        None,
        true,
    )
    .unwrap();
    session.studio_blobs = Some(BTreeMap::from([(
        "Blocker/B_Bad.terrain".to_owned(),
        blob("Blocker/B_Bad.terrain"),
    )]));
    let before = session.activity(0);
    let queued_before = session.take_patches(0).1.len();

    let outcome = session.apply_changes(vec![upsert("Aok.luau", "A")]);

    assert!(outcome.is_err());
    let after = session.activity(0);
    assert_eq!(after.events.len(), before.events.len());
    assert_eq!(after.pending, before.pending);
    assert_eq!(session.take_patches(0).1.len(), queued_before);
}

struct RefusesDirRemoval {
    inner: MemoryVfs,
    kind: io::ErrorKind,
}

impl Vfs for RefusesDirRemoval {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.inner.read(path)
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        self.inner.list(path)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        self.inner.write(path, contents)
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        self.inner.create_dir(path)
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        if matches!(self.inner.kind(path), Ok(EntryKind::Dir)) {
            return Err(VfsError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(self.kind),
            });
        }
        self.inner.remove(path)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), VfsError> {
        self.inner.rename(from, to)
    }

    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        self.inner.kind(path)
    }
}

fn batch_emptying_a_folder_the_disk_refuses_to_remove(kind: io::ErrorKind) {
    let mut vfs = RefusesDirRemoval {
        inner: MemoryVfs::new()
            .with_file("src/Folder/A.luau", "return 1")
            .with_file("src/Other.luau", "return 2"),
        kind,
    };
    vfs.inner.remove(Path::new("src/Folder/A.luau")).unwrap();
    let mut journal = Some(Journal::default());

    remove_emptied_dirs_in(&mut vfs, &mut journal, &[Path::new("src/Folder")]);

    assert!(vfs.inner.exists(Path::new("src/Folder")));
    assert!(!vfs.inner.exists(Path::new("src/Folder/A.luau")));
    assert!(vfs.inner.exists(Path::new("src/Other.luau")));
    let removed_dirs = journal
        .iter()
        .flat_map(|journal| &journal.disk)
        .filter(|change| matches!(change, DiskChange::RemovedDir(_)))
        .count();
    assert_eq!(removed_dirs, 0);
}

#[test]
fn a_folder_the_disk_reports_not_empty_is_kept_and_the_batch_is_not_failed() {
    batch_emptying_a_folder_the_disk_refuses_to_remove(io::ErrorKind::DirectoryNotEmpty);
}

#[test]
fn a_folder_the_disk_refuses_for_another_io_reason_is_kept_and_the_batch_is_not_failed() {
    batch_emptying_a_folder_the_disk_refuses_to_remove(io::ErrorKind::PermissionDenied);
}

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_client_path_with_control_bytes_is_logged_escaped() {
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();
    let hostile = "src/Bad\nDir\u{1b}[2J";
    let mut vfs = RefusesDirRemoval {
        inner: MemoryVfs::new().with_dir(hostile),
        kind: io::ErrorKind::PermissionDenied,
    };
    let mut journal = Some(Journal::default());

    with_default(subscriber, || {
        remove_emptied_dirs_in(&mut vfs, &mut journal, &[Path::new(hostile)]);
    });

    let output = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(output.contains("kept a directory"), "{output}");
    assert!(
        output.contains(r#"path="src/Bad\nDir\u{1b}[2J""#),
        "{output}"
    );
    assert!(!output.contains('\u{1b}'), "{output}");
    assert_eq!(output.matches('\n').count(), 1, "{output}");
}

#[test]
fn a_studio_edit_of_a_childless_init_script_writes_no_sibling_file() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Folder")).unwrap();
    fs::write(root.path().join("Folder").join("init.luau"), "return 1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![upsert("Folder.luau", "return 1")])
        .unwrap();

    session
        .apply_changes(vec![upsert("Folder.luau", "return 2")])
        .unwrap();

    assert!(!root.path().join("Folder.luau").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Folder").join("init.luau")).unwrap(),
        "return 2"
    );
}

#[test]
fn a_childless_init_script_over_a_flat_script_writes_the_flat_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Folder.luau"), "return 1").unwrap();
    let mut session = session_in(root.path());
    session.rescan().unwrap();
    session
        .apply_changes(vec![upsert("Folder/init.luau", "return 1")])
        .unwrap();

    session
        .apply_changes(vec![upsert("Folder/init.luau", "return 2")])
        .unwrap();

    assert!(!root.path().join("Folder").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Folder.luau")).unwrap(),
        "return 2"
    );
}

#[test]
fn a_path_upserted_and_deleted_in_one_batch_keeps_the_upsert_in_either_order() {
    let orders = [
        vec![
            upsert("A.luau", "v2"),
            Change::Delete {
                path: "A.luau".to_owned(),
            },
        ],
        vec![
            Change::Delete {
                path: "A.luau".to_owned(),
            },
            upsert("A.luau", "v2"),
        ],
    ];
    for batch in orders {
        let root = tempfile::tempdir().unwrap();
        let mut session = session_in(root.path());
        session.apply_changes(vec![upsert("A.luau", "v1")]).unwrap();

        session.apply_changes(batch).unwrap();

        assert_eq!(
            fs::read_to_string(root.path().join("A.luau")).unwrap(),
            "v2"
        );
        assert_eq!(session.studio["A.luau"].content, "v2");
    }
}

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
