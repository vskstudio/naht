//! The virtual filesystem the reconciler runs against.
//!
//! Everything above this layer works through the [`Vfs`] trait, so the mapping and reconcile logic
//! is unit-testable against an in-memory tree ([`MemoryVfs`]) with no disk dependency. [`DiskVfs`]
//! is the real-disk implementation used by the daemon.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// Validate a path received from the Studio protocol as project-relative.
///
/// The daemon joins protocol paths to a project root, so absolute paths and parent components
/// would otherwise escape that root. Backslashes and drive prefixes are rejected explicitly so the
/// same wire contract is safe on Unix and Windows.
pub fn validate_project_path(path: &str) -> Result<(), ProjectPathError> {
    if path.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ProjectPathError::Absolute);
    }
    if path.contains('\0') {
        return Err(ProjectPathError::Nul);
    }

    for component in path.split(['/', '\\']) {
        if component.is_empty() {
            return Err(ProjectPathError::EmptyComponent);
        }
        if component == "." || component == ".." {
            return Err(ProjectPathError::Traversal);
        }
        if component.contains(':') {
            return Err(ProjectPathError::DrivePrefix);
        }
    }
    Ok(())
}

/// Why a protocol path is not a safe project-relative path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectPathError {
    /// No path was supplied.
    #[error("path must not be empty")]
    Empty,
    /// The path starts at the filesystem root.
    #[error("absolute paths are not allowed")]
    Absolute,
    /// The path contains an empty component.
    #[error("path contains an empty component")]
    EmptyComponent,
    /// The path contains a parent or current-directory component.
    #[error("path traversal components are not allowed")]
    Traversal,
    /// The path contains a NUL byte.
    #[error("NUL bytes are not allowed in paths")]
    Nul,
    /// The path contains a Windows drive prefix.
    #[error("drive prefixes are not allowed")]
    DrivePrefix,
}

/// Errors raised by a [`Vfs`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    /// The path does not exist.
    #[error("path not found: {0}")]
    NotFound(PathBuf),
    /// A directory operation was attempted on a file.
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),
    /// A file operation was attempted on a directory.
    #[error("not a file: {0}")]
    NotAFile(PathBuf),
    /// A protocol path is malformed or is not safely rooted.
    #[error("invalid project path: {0}")]
    InvalidPath(String),
    /// A path component is a symlink, so its destination cannot be proven to stay in the project.
    #[error("symlink path is not allowed: {0}")]
    Symlink(PathBuf),
    /// An underlying I/O failure on the real disk.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// The path the operation targeted.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Whether a directory entry is a file or a subdirectory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
}

/// One entry returned by [`Vfs::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Full path of the entry.
    pub path: PathBuf,
    /// Whether it is a file or a directory.
    pub kind: EntryKind,
}

/// A pluggable filesystem: read, list, write, remove.
///
/// Mutating operations take `&mut self` so the in-memory implementation needs no interior
/// mutability; the daemon owns its [`Vfs`] exclusively.
pub trait Vfs {
    /// Read a file's raw bytes.
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    /// List the immediate children of a directory, sorted by path.
    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    /// Write a file, creating parent directories as needed.
    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError>;
    /// Create a directory (and any missing parents). Needed to represent an empty `Folder`, which
    /// has no file to imply its existence.
    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError>;
    /// Remove a file, or a directory and everything under it.
    fn remove(&mut self, path: &Path) -> Result<(), VfsError>;
    /// Whether a path exists (as a file or a directory).
    fn exists(&self, path: &Path) -> bool;
    /// Classify a path as a file or a directory.
    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError>;
}

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
        // The implicit root (empty path) always exists; any explicit dir must be present.
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
        entries.sort_by(|a, b| a.path.cmp(&b.path));
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
        if self.dirs.remove(path) {
            self.files.retain(|p, _| !p.starts_with(path));
            self.dirs.retain(|p| !p.starts_with(path));
            return Ok(());
        }
        Err(VfsError::NotFound(path.to_path_buf()))
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
        std::fs::read(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError> {
        let read_dir = std::fs::read_dir(path).map_err(|source| VfsError::Io {
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
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| VfsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, contents).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError> {
        std::fs::create_dir_all(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn remove(&mut self, path: &Path) -> Result<(), VfsError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|source| VfsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let result = if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
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
        let metadata = std::fs::symlink_metadata(path).map_err(|source| VfsError::Io {
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

/// A [`Vfs`] adapter that presents `inner` in coordinates relative to a fixed `root`.
///
/// Every path is joined onto `root` before reaching `inner`, and paths returned by [`Vfs::list`] are
/// stripped back to root-relative form. The daemon wraps a [`DiskVfs`] so the reconciler keys
/// instances by project-relative paths (the stable wire identity) while disk I/O lands under the
/// real project directory.
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

    #[test]
    fn rooted_vfs_anchors_relative_paths_under_root_and_lists_relative() {
        let backing = MemoryVfs::new().with_file("project/keep.luau", "k");
        let mut rooted = RootedVfs::new("project", backing);

        // Writes land under the root...
        rooted.write(Path::new("sub/new.luau"), b"n").unwrap();
        assert_eq!(rooted.read(Path::new("sub/new.luau")).unwrap(), b"n");
        assert_eq!(
            rooted
                .inner
                .read(Path::new("project/sub/new.luau"))
                .unwrap(),
            b"n"
        );

        // ...and listing returns paths relative to the root, not absolute.
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
        let names: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
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
}
