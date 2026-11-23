use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use naht_core::binary::{BlobInstance, BlobPatch};
use naht_core::protocol::{EventKind, Outcome};
use naht_core::reconciler::{Direction, Patch, TextInstance};
use naht_core::state::{InstanceRecord, StateError, StateStore};
use naht_core::vfs::{DirEntry, EntryKind, Vfs, VfsError};

use super::{Session, SessionError};

#[derive(Debug, Default)]
pub(super) struct Journal {
    pub(super) studio: Vec<(String, Option<TextInstance>)>,
    pub(super) blobs: BlobsJournal,
    pub(super) disk: Vec<DiskChange>,
    pub(super) base: BTreeMap<String, InstanceRecord>,
    pub(super) outbox: Vec<Outbound>,
}

#[derive(Debug, Default)]
pub(super) enum BlobsJournal {
    #[default]
    Untouched,
    Before(Option<BTreeMap<String, BlobInstance>>),
}

#[derive(Debug)]
pub(super) enum DiskChange {
    Dir(PathBuf),
    RemovedDir(PathBuf),
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },
    File {
        path: PathBuf,
        previous: Option<Vec<u8>>,
    },
}

#[derive(Debug)]
pub(super) enum Outbound {
    Event {
        path: String,
        direction: Direction,
        kind: EventKind,
        outcome: Outcome,
    },
    Queue(Patch),
    QueueBlob(BlobPatch),
    Unacked(String, Option<Patch>),
}

pub(super) struct Transaction<'a, V: Vfs> {
    session: &'a mut Session<V>,
}

impl<'a, V: Vfs> Transaction<'a, V> {
    pub(super) fn begin(session: &'a mut Session<V>) -> Result<Self, SessionError> {
        let base = base_records(&session.store)?;
        session.journal = Some(Journal {
            base,
            ..Journal::default()
        });
        Ok(Self { session })
    }

    pub(super) fn commit(self) {
        if let Some(journal) = self.session.journal.take() {
            for outbound in journal.outbox {
                self.session.apply(outbound);
            }
        }
    }
}

impl<V: Vfs> Deref for Transaction<'_, V> {
    type Target = Session<V>;

    fn deref(&self) -> &Session<V> {
        self.session
    }
}

impl<V: Vfs> DerefMut for Transaction<'_, V> {
    fn deref_mut(&mut self) -> &mut Session<V> {
        self.session
    }
}

impl<V: Vfs> Drop for Transaction<'_, V> {
    fn drop(&mut self) {
        if let Some(journal) = self.session.journal.take() {
            drop(journal);
        }
    }
}

fn rollback<V: Vfs>(session: &mut Session<V>, journal: Journal) {
    restore_disk(&mut session.vfs, journal.disk);
    restore_base(&session.store, &journal.base);
    restore_mirror(&mut session.studio, journal.studio);
    if let BlobsJournal::Before(previous) = journal.blobs {
        session.studio_blobs = previous;
    }
}

fn base_records(store: &StateStore) -> Result<BTreeMap<String, InstanceRecord>, StateError> {
    Ok(store
        .all()?
        .into_iter()
        .map(|record| (record.guid.clone(), record))
        .collect())
}

fn restore_disk(vfs: &mut impl Vfs, changes: Vec<DiskChange>) {
    for change in changes.into_iter().rev() {
        let outcome = match change {
            DiskChange::File {
                path,
                previous: Some(bytes),
            } => vfs.write(&path, &bytes),
            DiskChange::File {
                path,
                previous: None,
            } => remove_if_present(vfs, &path),
            DiskChange::Dir(path) => remove_if_empty_dir(vfs, &path),
            DiskChange::RemovedDir(path) => vfs.create_dir(&path),
            DiskChange::Renamed { from, to } => vfs.rename(&to, &from),
        };
        if let Err(error) = outcome {
            tracing::error!(
                target: "naht::session",
                error = ?error,
                "could not undo a filesystem write from a failed batch"
            );
        }
    }
}

fn remove_if_present(vfs: &mut impl Vfs, path: &Path) -> Result<(), VfsError> {
    if vfs.exists(path) {
        vfs.remove(path)
    } else {
        Ok(())
    }
}

fn remove_if_empty_dir(vfs: &mut impl Vfs, path: &Path) -> Result<(), VfsError> {
    if !vfs.exists(path) || vfs.kind(path)? != EntryKind::Dir || !vfs.list(path)?.is_empty() {
        return Ok(());
    }
    vfs.remove(path)
}

fn restore_base(store: &StateStore, before: &BTreeMap<String, InstanceRecord>) {
    let current = match base_records(store) {
        Ok(current) => current,
        Err(error) => {
            tracing::error!(target: "naht::session", error = ?error, "could not read the base to undo a failed batch");
            return;
        }
    };
    for guid in current.keys().filter(|guid| !before.contains_key(*guid)) {
        if let Err(error) = store.remove(guid) {
            tracing::error!(target: "naht::session", error = ?error, "could not undo a base record from a failed batch");
        }
    }
    for (guid, record) in before {
        if current.get(guid) == Some(record) {
            continue;
        }
        if let Err(error) = store.upsert(record) {
            tracing::error!(target: "naht::session", error = ?error, "could not restore a base record from a failed batch");
        }
    }
}

fn restore_mirror<Instance>(
    mirror: &mut BTreeMap<String, Instance>,
    journal: Vec<(String, Option<Instance>)>,
) {
    for (path, previous) in journal.into_iter().rev() {
        match previous {
            Some(instance) => {
                mirror.insert(path, instance);
            }
            None => {
                mirror.remove(&path);
            }
        }
    }
}

pub(super) struct Journaled<'a, V: Vfs> {
    pub(super) inner: &'a mut V,
    pub(super) journal: &'a mut Option<Journal>,
}

impl<V: Vfs> Journaled<'_, V> {
    fn push(&mut self, change: DiskChange) {
        if let Some(journal) = self.journal.as_mut() {
            journal.disk.push(change);
        }
    }

    fn content_before(&self, path: &Path) -> Result<Option<Vec<u8>>, VfsError> {
        match self.inner.kind(path) {
            Ok(EntryKind::File) => self.inner.read(path).map(Some),
            _ => Ok(None),
        }
    }

    pub(super) fn remove_if_emptied(&mut self, path: &Path) -> Result<(), VfsError> {
        let emptied = matches!(self.inner.kind(path), Ok(EntryKind::Dir))
            && self.inner.list(path)?.is_empty();
        if !emptied {
            return Ok(());
        }
        self.inner.remove(path)?;
        self.push(DiskChange::RemovedDir(path.to_path_buf()));
        Ok(())
    }

    fn record_missing_dirs(&mut self, dir: &Path) {
        let missing: Vec<PathBuf> = dir
            .ancestors()
            .filter(|ancestor| !ancestor.as_os_str().is_empty() && !self.inner.exists(ancestor))
            .map(Path::to_path_buf)
            .collect();
        for path in missing.into_iter().rev() {
            self.push(DiskChange::Dir(path));
        }
    }
}

impl<V: Vfs> Vfs for Journaled<'_, V> {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.inner.read(path)
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        self.inner.list(path)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if self.journal.is_none() {
            return self.inner.write(path, contents);
        }
        let previous = self.content_before(path)?;
        if let Some(parent) = path.parent() {
            self.record_missing_dirs(parent);
        }
        self.inner.write(path, contents)?;
        self.push(DiskChange::File {
            path: path.to_path_buf(),
            previous,
        });
        Ok(())
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        if self.journal.is_some() {
            self.record_missing_dirs(path);
        }
        self.inner.create_dir(path)
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        if matches!(self.inner.kind(path), Ok(EntryKind::Dir)) {
            return Err(VfsError::NotAFile(path.to_path_buf()));
        }
        if self.journal.is_none() {
            return self.inner.remove(path);
        }
        let previous = self.content_before(path)?;
        self.inner.remove(path)?;
        if let Some(previous) = previous {
            self.push(DiskChange::File {
                path: path.to_path_buf(),
                previous: Some(previous),
            });
        }
        Ok(())
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), VfsError> {
        self.inner.rename(from, to)?;
        self.push(DiskChange::Renamed {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
        });
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        self.inner.kind(path)
    }
}

#[cfg(test)]
mod tests;
