use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{DirEntry, EntryKind, Vfs, VfsError};

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

fn temp_sibling(target: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = target
        .file_name()
        .map(|os_name| os_name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(".{name}.{}.{serial}.naht-tmp", process::id()))
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

fn write_then_rename(temp: &Path, target: &Path, contents: &[u8]) -> io::Result<()> {
    fs::write(temp, contents)?;
    if let Ok(metadata) = fs::metadata(target) {
        fs::set_permissions(temp, metadata.permissions())?;
    }
    fs::rename(temp, target)
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
        if is_symlink(path) {
            return Err(VfsError::Symlink(path.to_path_buf()));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| VfsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let temp = temp_sibling(path);
        write_then_rename(&temp, path, contents).map_err(|source| {
            drop(fs::remove_file(&temp));
            VfsError::Io {
                path: path.to_path_buf(),
                source,
            }
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn write_replaces_the_file_and_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Main.luau");
        fs::write(&target, "old contents").unwrap();
        let before = fs::metadata(&target).unwrap().ino();

        DiskVfs::new().write(&target, b"new contents").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new contents");
        assert_ne!(
            fs::metadata(&target).unwrap().ino(),
            before,
            "the write must replace the file, not truncate it in place"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn write_refuses_a_live_symlink_and_leaves_its_target_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside.txt");
        let link = dir.path().join("Main.luau");
        fs::write(&outside, "keep").unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let result = DiskVfs::new().write(&link, b"replaced");

        assert!(matches!(result, Err(VfsError::Symlink(_))));
        assert_eq!(fs::read(&outside).unwrap(), b"keep");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    }

    #[test]
    fn write_refuses_a_dangling_symlink_and_creates_nothing_behind_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.txt");
        let link = dir.path().join("Main.luau");
        std::os::unix::fs::symlink(&missing, &link).unwrap();

        let result = DiskVfs::new().write(&link, b"new");

        assert!(matches!(result, Err(VfsError::Symlink(_))));
        assert!(!missing.exists());
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    }
}
