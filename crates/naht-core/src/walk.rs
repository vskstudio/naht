//! The directory walk rules shared by every scanner and the filesystem watcher, and the scan result
//! that reports what could not be read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::reconciler::ReconcileError;

const IGNORED_DIRS: [&str; 2] = ["node_modules", "target"];

/// Whether a directory entry name is skipped by every scan and by the filesystem watcher.
#[must_use]
pub fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || IGNORED_DIRS.contains(&name)
}

/// Whether the final component of `path` is skipped by every scan.
#[must_use]
pub fn is_ignored_entry(path: &Path) -> bool {
    path.file_name()
        .and_then(|os_name| os_name.to_str())
        .is_some_and(is_ignored_name)
}

/// A file a scan could not read, left out of the result instead of aborting the whole scan.
#[derive(Debug)]
pub struct SkippedFile {
    /// Where the file lives.
    pub path: PathBuf,
    /// Why it could not be read.
    pub error: ReconcileError,
}

/// What a scan collected: every readable instance, plus the files it had to skip.
#[derive(Debug)]
pub struct Scan<T> {
    /// Path-keyed instances collected from readable files.
    pub instances: BTreeMap<String, T>,
    /// Files that could not be read, in walk order.
    pub skipped: Vec<SkippedFile>,
}

impl<T> Scan<T> {
    /// Record `path` as skipped because of `error`.
    pub fn skip(&mut self, path: PathBuf, error: ReconcileError) {
        self.skipped.push(SkippedFile { path, error });
    }
}

impl<T> Default for Scan<T> {
    fn default() -> Self {
        Self {
            instances: BTreeMap::new(),
            skipped: Vec::new(),
        }
    }
}
