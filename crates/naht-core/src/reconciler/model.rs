use serde::{Deserialize, Serialize};

use crate::state::StateError;
use crate::vfs::VfsError;

/// Which way a patch flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Filesystem → Studio (carried to the plugin on its next long-poll).
    ToStudio,
    /// Studio → filesystem (written to disk by the daemon).
    ToFs,
}

/// What a patch does to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchKind {
    /// The instance is new on the target side.
    Create,
    /// The instance's content changed.
    Update,
    /// The instance was removed.
    Delete,
    /// Markers were written to the filesystem and the path frozen.
    Conflict,
}

/// One reconciliation action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// The instance's filesystem path (its stable key in Stage 3).
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// Which way the patch flows.
    pub direction: Direction,
    /// What the patch does.
    pub kind: PatchKind,
    /// The new content, or `None` for a delete.
    pub content: Option<String>,
}

/// A text instance on one side of the sync, keyed by its filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextInstance {
    /// The instance's filesystem path.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// The current text content.
    pub content: String,
}

impl TextInstance {
    /// Convenience constructor.
    pub fn new(
        path: impl Into<String>,
        class: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            class: class.into(),
            content: content.into(),
        }
    }
}

/// The set of frozen (conflicted) paths awaiting resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Paths currently frozen pending `resolve`.
    pub conflicted: Vec<String>,
}

/// Errors from reconciliation.
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    /// The state store failed.
    #[error(transparent)]
    State(#[from] StateError),
    /// A filesystem operation failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// A text file holds bytes that are not valid UTF-8.
    #[error("non-UTF-8 content at {0}")]
    NonUtf8(String),
    /// `resolve` was asked about a path that is not conflicted.
    #[error("no conflict to resolve at {0}")]
    NotConflicted(String),
    /// `resolve` was refused because the file still has conflict markers.
    #[error("conflict markers remain at {0}")]
    MarkersRemain(String),
}

pub(super) fn patch(
    path: &str,
    class: &str,
    direction: Direction,
    kind: PatchKind,
    content: Option<&str>,
) -> Patch {
    Patch {
        path: path.to_owned(),
        class: class.to_owned(),
        direction,
        kind,
        content: content.map(str::to_owned),
    }
}
