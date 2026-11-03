//! Opaque binary-blob sync: the path that lifts terrain (and any file-level binary) from "deferred"
//! to a conflict-safe, byte-exact sync (architecture §9).
//!
//! Unlike scripts, a blob can't be diffed or merged: it is compared by hash and synced whole,
//! last-writer-wins, and a change on *both* sides since the base **freezes** the path instead of
//! silently overwriting either copy. The persisted base stores only the hash (no `base_content`, per
//! architecture §5's binary rule), so a restart still detects change without keeping the bytes.

mod model;
mod scan;
mod sync;
#[cfg(test)]
mod tests;

pub use model::{BLOB_EXTENSION, BlobInstance, BlobPatch, TERRAIN_CLASS};
pub use scan::scan_blobs;
pub use sync::reconcile_blobs;
