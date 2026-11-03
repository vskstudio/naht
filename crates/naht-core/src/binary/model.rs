use serde::{Deserialize, Serialize};

use crate::reconciler::{Classed, Direction, PatchKind};

/// The filesystem extension a binary blob lives behind.
///
/// A `*.terrain` file is the opaque voxel blob the daemon syncs (architecture §9). The convention
/// mirrors the script-suffix conventions in the mapper, kept here because only the binary path
/// consumes it.
pub const BLOB_EXTENSION: &str = "terrain";

/// The Roblox class a `*.terrain` blob maps to.
pub const TERRAIN_CLASS: &str = "Terrain";

/// One binary instance on a side of the sync, keyed by its filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobInstance {
    /// The blob's filesystem path (its stable key).
    pub path: String,
    /// The Roblox class name (e.g. `Terrain`).
    pub class: String,
    /// The raw bytes (encoded as a `MessagePack` binary blob, not an integer array).
    #[serde(with = "serde_bytes")]
    pub content: Vec<u8>,
}

impl Classed for BlobInstance {
    fn class_name(&self) -> &str {
        &self.class
    }
}

impl BlobInstance {
    /// Convenience constructor.
    pub fn new(
        path: impl Into<String>,
        class: impl Into<String>,
        content: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            path: path.into(),
            class: class.into(),
            content: content.into(),
        }
    }
}

/// One binary reconciliation action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobPatch {
    /// The blob's filesystem path.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// Which way the patch flows.
    pub direction: Direction,
    /// What the patch does. A `Conflict` froze the path; no bytes were written.
    pub kind: PatchKind,
    /// The blob bytes, or `None` for a delete or a conflict freeze.
    #[serde(with = "serde_bytes")]
    pub content: Option<Vec<u8>>,
}

pub(super) fn patch(
    path: &str,
    class: &str,
    direction: Direction,
    kind: PatchKind,
    content: Option<&[u8]>,
) -> BlobPatch {
    BlobPatch {
        path: path.to_owned(),
        class: class.to_owned(),
        direction,
        kind,
        content: content.map(<[u8]>::to_vec),
    }
}
