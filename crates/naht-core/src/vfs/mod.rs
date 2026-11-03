//! The virtual filesystem the reconciler runs against.
//!
//! Everything above this layer works through the [`Vfs`] trait, so the mapping and reconcile logic
//! is unit-testable against an in-memory tree ([`MemoryVfs`]) with no disk dependency. [`DiskVfs`]
//! is the real-disk implementation used by the daemon.

use std::io;
use std::path::{Path, PathBuf};

mod disk;
mod memory;
mod project_path;
mod rooted;
#[cfg(test)]
mod tests;

pub use disk::DiskVfs;
pub use memory::MemoryVfs;
pub use project_path::{ProjectPathError, validate_project_path};
pub use rooted::RootedVfs;

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
    /// A protocol path failed the project path rules.
    #[error("invalid project path: {path}")]
    Malformed {
        /// The path as it was supplied.
        path: String,
        /// The rule the path broke.
        #[source]
        source: ProjectPathError,
    },
    /// A path component is a symlink, so its destination cannot be proven to stay in the project.
    #[error("symlink path is not allowed: {0}")]
    Symlink(PathBuf),
    /// An underlying I/O failure on the real disk.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying operating system error.
        source: io::Error,
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
    /// Returns the whole contents of the file at `path`, failing when it is absent or is a directory.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the file cannot be read.
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    /// Lists the direct children of the directory at `path`, not its subtree; fails when `path` is absent or is a file.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the directory cannot be listed.
    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    /// Creates or overwrites the file at `path`, creating its missing parent directories; fails when `path` names a directory.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the file cannot be written.
    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError>;
    /// Creates `path` and its missing parents, and succeeds when it already exists; fails when a file is at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the directory cannot be created.
    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError>;
    /// Deletes a file, or a directory together with its whole subtree; fails when nothing is at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the path cannot be removed.
    fn remove(&mut self, path: &Path) -> Result<(), VfsError>;
    /// Reports whether a file or a directory is present at `path`; a path that cannot be checked, or that the project boundary refuses, counts as absent.
    fn exists(&self, path: &Path) -> bool;
    /// Reports whether the entry at `path` is a file or a directory; fails when nothing is at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`VfsError`] when the path cannot be inspected.
    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError>;
}
