use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use super::{DirEntry, EntryKind, Vfs, VfsError};

/// An in-memory [`Vfs`] for tests: deterministic, no disk, no I/O errors.
#[derive(Debug, Default, Clone)]
pub struct MemoryVfs {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
}

impl MemoryVfs {
    /// Create an empty in-memory filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder helper: insert a file (and its parent directories) up front.
    #[must_use]
    pub fn with_file(mut self, path: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> Self {
        let path = path.into();
        self.ensure_parents(&path);
        self.files.insert(path, contents.into());
        self
    }

    /// Builder helper: insert an (initially empty) directory up front.
    #[must_use]
    pub fn with_dir(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.ensure_parents(&path);
        self.dirs.insert(path);
        self
    }

    fn has_entries_under(&self, dir: &Path) -> bool {
        self.files
            .keys()
            .any(|file| file.starts_with(dir) && file != dir)
            || self
                .dirs
                .iter()
                .any(|child| child.starts_with(dir) && child != dir)
    }

    fn ensure_parents(&mut self, path: &Path) {
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir.as_os_str().is_empty() {
                break;
            }
            self.dirs.insert(dir.to_path_buf());
            current = dir.parent();
        }
    }
}

impl Vfs for MemoryVfs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        if self.dirs.contains(path) {
            return Err(VfsError::NotAFile(path.to_path_buf()));
        }
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| VfsError::NotFound(path.to_path_buf()))
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        if self.files.contains_key(path) {
            return Err(VfsError::NotADirectory(path.to_path_buf()));
        }
        if !path.as_os_str().is_empty() && !self.dirs.contains(path) {
            return Err(VfsError::NotFound(path.to_path_buf()));
        }
        let mut entries = Vec::new();
        for dir in &self.dirs {
            if dir.parent() == Some(path) {
                entries.push(DirEntry {
                    path: dir.clone(),
                    kind: EntryKind::Dir,
                });
            }
        }
        for file in self.files.keys() {
            if file.parent() == Some(path) {
                entries.push(DirEntry {
                    path: file.clone(),
                    kind: EntryKind::File,
                });
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if self.dirs.contains(path) {
            return Err(VfsError::NotAFile(path.to_path_buf()));
        }
        self.ensure_parents(path);
        self.files.insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        if self.files.contains_key(path) {
            return Err(VfsError::NotADirectory(path.to_path_buf()));
        }
        self.ensure_parents(path);
        self.dirs.insert(path.to_path_buf());
        Ok(())
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        if self.files.remove(path).is_some() {
            return Ok(());
        }
        if self.dirs.contains(path) {
            if self.has_entries_under(path) {
                return Err(VfsError::Io {
                    path: path.to_path_buf(),
                    source: io::Error::from(io::ErrorKind::DirectoryNotEmpty),
                });
            }
            self.dirs.remove(path);
            return Ok(());
        }
        Err(VfsError::NotFound(path.to_path_buf()))
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<(), VfsError> {
        if self.dirs.contains(to) {
            return Err(VfsError::NotAFile(to.to_path_buf()));
        }
        let Some(contents) = self.files.remove(from) else {
            return Err(if self.dirs.contains(from) {
                VfsError::NotAFile(from.to_path_buf())
            } else {
                VfsError::NotFound(from.to_path_buf())
            });
        };
        self.ensure_parents(to);
        self.files.insert(to.to_path_buf(), contents);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.dirs.contains(path)
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        if self.files.contains_key(path) {
            Ok(EntryKind::File)
        } else if self.dirs.contains(path) {
            Ok(EntryKind::Dir)
        } else {
            Err(VfsError::NotFound(path.to_path_buf()))
        }
    }
}
