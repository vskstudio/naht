//! Diff the current sides against the persisted base and produce directional patches, resolving
//! concurrent edits with a 3-way merge.
//!
//! This is the brain's decision layer, still with no network: given the current filesystem side,
//! the current Studio side (supplied directly here; over the wire in Stage 4), and the persisted
//! base, it decides per instance what to push where, advances the base on a clean result, and, on
//! an unmergeable conflict, writes git-style markers, freezes the path, and loses nothing.
//!
//! Stage 3 operates on text instances keyed by path; the daemon maps stable GUIDs onto these in
//! later stages. Binary bases (no `base_content`) are skipped here.

mod model;
mod persist;
mod scan;
mod shared;
mod sync;
#[cfg(test)]
mod tests;

pub use model::{Direction, Patch, PatchKind, ReconcileError, Status, TextInstance};
pub use persist::{resolve, set_base, status};
pub use scan::scan_text;
pub(crate) use shared::{Classed, Side, class_of, remove_fs_copy, union_keys, write_toward};
pub use sync::reconcile;
