//! The directory walk rules shared by every scanner and the filesystem watcher, and the scan result
//! that reports what could not be read.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

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

/// Whether any normal component of `path` is skipped by every scan, so the path lies under an
/// ignored directory. Current and parent markers are not names and never match.
#[must_use]
pub fn has_ignored_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(is_ignored_name),
        Component::CurDir | Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
            false
        }
    })
}

/// Paths the scan did not see.
///
/// Covers unreadable files and everything under a skipped directory.
/// The reconciler leaves these untouched on both sides, so an unseen file is never mistaken for a
/// deleted one.
#[derive(Debug, Default, Clone)]
pub struct Unseen {
    skipped: Vec<PathBuf>,
}

impl Unseen {
    /// Whether the path-keyed instance `key` is unseen: it lies under an ignored directory or is, or
    /// lies under, a file the scan could not read.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        let path = Path::new(key);
        has_ignored_component(path) || self.skipped.iter().any(|skipped| path.starts_with(skipped))
    }

    /// The unreadable paths, in walk order.
    pub fn skipped(&self) -> impl Iterator<Item = &Path> {
        self.skipped.iter().map(PathBuf::as_path)
    }
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

    /// The paths this scan did not see, for the reconciler to leave untouched.
    #[must_use]
    pub fn unseen(&self) -> Unseen {
        Unseen {
            skipped: self
                .skipped
                .iter()
                .map(|skipped| skipped.path.clone())
                .collect(),
        }
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
