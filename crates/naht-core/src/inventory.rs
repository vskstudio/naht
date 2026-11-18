//! The script inventory a plugin posts on attach (`POST /inventory`): each placed script's path,
//! class and source fingerprint, never its source, so a large place stays under the body limit.

use serde::{Deserialize, Serialize};

/// One script Studio lists on attach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryEntry {
    /// The placed, project-relative path the plugin resolves for the script.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// The source fingerprint, as [`crate::hash::source_fingerprint`] computes it.
    pub hash: String,
}

/// The body of `POST /inventory`: every script the place holds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InventoryBatch {
    /// The scripts, in the order the plugin listed them.
    pub scripts: Vec<InventoryEntry>,
}
