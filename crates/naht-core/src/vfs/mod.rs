//! The virtual filesystem the reconciler runs against.
//!
//! Everything above this layer works through the [`Vfs`] trait, so the mapping and reconcile logic
//! is unit-testable against an in-memory tree ([`MemoryVfs`]) with no disk dependency. [`DiskVfs`]
//! is the real-disk implementation used by the daemon.

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
    /// A path component is a symlink, so its destination cannot be proven to stay in the project.
    #[error("symlink path is not allowed: {0}")]
    Symlink(PathBuf),
    /// An underlying I/O failure on the real disk.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
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
    fn read(&self, path: &Path) -> Result<Vec<u8>, VfsError>;
    fn list(&self, path: &Path) -> Result<Vec<DirEntry>, VfsError>;
    fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), VfsError>;
    fn create_dir(&mut self, path: &Path) -> Result<(), VfsError>;
    fn remove(&mut self, path: &Path) -> Result<(), VfsError>;
    fn exists(&self, path: &Path) -> bool;
    fn kind(&self, path: &Path) -> Result<EntryKind, VfsError>;
}
