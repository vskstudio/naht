use std::path::{Component, Path, PathBuf};

use super::project_path::validate_project_path;
use super::{DirEntry, EntryKind, Vfs, VfsError};

/// A [`Vfs`] adapter that presents `inner` in coordinates relative to a fixed `root`.
///
/// Every path is joined onto `root` before reaching `inner`, and paths returned by [`Vfs::list`] are
/// stripped back to root-relative form. The daemon wraps a [`DiskVfs`](super::DiskVfs) so the
/// reconciler keys instances by project-relative paths (the stable wire identity) while disk I/O
/// lands under the real project directory.
#[derive(Debug, Clone)]
pub struct RootedVfs<V: Vfs> {
    root: PathBuf,
    inner: V,
}

impl<V: Vfs> RootedVfs<V> {
    /// Wrap `inner`, anchoring all relative paths under `root`.
    pub fn new(root: impl Into<PathBuf>, inner: V) -> Self {
        Self {
            root: root.into(),
            inner,
        }
    }

    fn checked_absolute(&self, path: &Path) -> Result<PathBuf, VfsError> {
        let absolute = self.root.join(path);
        let mut current = self.root.clone();
        for component in path.components() {
            match component {
                Component::Normal(name) => current.push(name),
                Component::CurDir => continue,
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(VfsError::InvalidPath(path.display().to_string()));
                }
            }
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(VfsError::Symlink(current));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(source) => {
                    return Err(VfsError::Io {
                        path: current,
                        source,
                    });
                }
            }
        }
        Ok(absolute)
    }

    /// Validate a protocol path against this rooted filesystem, including existing symlink
    /// components. Symlinks are rejected even when they currently point inside the root because
    /// their target can change between validation and the subsequent filesystem operation.
    pub fn validate_path(&self, path: &str) -> Result<(), VfsError> {
        validate_project_path(path).map_err(|error| VfsError::InvalidPath(error.to_string()))?;
        self.checked_absolute(Path::new(path)).map(|_| ())
    }

    fn relativize(&self, entry: &Path) -> PathBuf {
        entry
            .strip_prefix(&self.root)
            .unwrap_or(entry)
            .to_path_buf()
    }
}

impl<V: Vfs> Vfs for RootedVfs<V> {
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError> {
        self.inner.read(&self.checked_absolute(path)?)
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let mut entries = self.inner.list(&self.checked_absolute(path)?)?;
        for entry in &mut entries {
            entry.path = self.relativize(&entry.path);
        }
        Ok(entries)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        self.inner.write(&self.checked_absolute(path)?, contents)
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        self.inner.create_dir(&self.checked_absolute(path)?)
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        self.inner.remove(&self.checked_absolute(path)?)
    }

    fn exists(&self, path: &Path) -> bool {
        self.checked_absolute(path)
            .map(|absolute| self.inner.exists(&absolute))
            .unwrap_or(false)
    }

    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError> {
        self.inner.kind(&self.checked_absolute(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemoryVfs;

    #[test]
    fn rooted_vfs_anchors_relative_paths_under_root_and_lists_relative() {
        let backing = MemoryVfs::new().with_file("project/keep.luau", "k");
        let mut rooted = RootedVfs::new("project", backing);

        rooted.write(Path::new("sub/new.luau"), b"n").unwrap();
        assert_eq!(rooted.read(Path::new("sub/new.luau")).unwrap(), b"n");
        assert_eq!(
            rooted
                .inner
                .read(Path::new("project/sub/new.luau"))
                .unwrap(),
            b"n"
        );

        let names: Vec<_> = rooted
            .list(Path::new(""))
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(
            names,
            vec![PathBuf::from("keep.luau"), PathBuf::from("sub")]
        );
    }
}
