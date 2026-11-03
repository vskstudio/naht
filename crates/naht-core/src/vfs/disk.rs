use std::path::Path;

use super::{DirEntry, EntryKind, Vfs, VfsError};
use std::fs;

/// The real-disk [`Vfs`] used by the daemon.
#[derive(Debug, Clone)]
pub struct DiskVfs;

impl DiskVfs {
    /// Create a disk-backed filesystem. Paths are used as given (absolute, or relative to the
    /// process working directory).
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiskVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs for DiskVfs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        fs::read(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let read_dir = fs::read_dir(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut entries = Vec::new();
        for entry in read_dir {
            let entry = entry.map_err(|source| VfsError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| VfsError::Io {
                path: entry.path(),
                source,
            })?;
            let kind = if file_type.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            };
            entries.push(DirEntry {
                path: entry.path(),
                kind,
            });
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| VfsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(path, contents).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        fs::create_dir_all(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        let metadata = fs::symlink_metadata(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let result = if metadata.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        result.map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        let metadata = fs::symlink_metadata(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if metadata.is_dir() {
            Ok(EntryKind::Dir)
        } else {
            Ok(EntryKind::File)
        }
    }
}
