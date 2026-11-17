use std::path::{Path, PathBuf};

use crate::hash::content_hash;
use crate::reconciler::ReconcileError;
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::Vfs;

/// The side a frozen binary conflict keeps; the other side is overwritten on the next reconcile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Keep the filesystem copy and push it to Studio.
    Disk,
    /// Keep the Studio copy and write it over the filesystem copy.
    Studio,
}

pub(super) const KEEP_DISK_MARKER: &str = "";

/// Clears a frozen binary conflict by choosing the side to keep.
///
/// Only the persisted record changes here. Keeping the disk stores the keep-disk marker as the base,
/// whatever hash the record held before, so the next reconcile pushes the disk bytes to Studio.
/// Keeping the Studio side records the disk hash as the base, so the next reconcile sees the Studio
/// bytes as a change to write; when the disk copy is gone, the record is dropped and the next
/// reconcile recreates the file from Studio.
///
/// # Errors
///
/// Returns an error when `path` is not a frozen binary conflict, the disk copy is missing while
/// keeping it, or the state store fails.
pub fn resolve(
    vfs: &impl Vfs,
    store: &StateStore,
    path: &str,
    keep: Keep,
) -> Result<(), ReconcileError> {
    let record = store
        .get(path)?
        .filter(|record| record.conflicted && record.base_content.is_none())
        .ok_or_else(|| ReconcileError::NotConflicted(PathBuf::from(path)))?;
    let on_disk = vfs.exists(Path::new(path));
    match (keep, on_disk) {
        (Keep::Disk, false) => Err(ReconcileError::MissingOnDisk(PathBuf::from(path))),
        (Keep::Disk, true) => {
            store.upsert(&InstanceRecord {
                content_hash: KEEP_DISK_MARKER.to_owned(),
                conflicted: false,
                ..record
            })?;
            Ok(())
        }
        (Keep::Studio, true) => {
            let disk = vfs.read(Path::new(path))?;
            store.upsert(&InstanceRecord {
                content_hash: content_hash(&disk),
                conflicted: false,
                ..record
            })?;
            Ok(())
        }
        (Keep::Studio, false) => {
            store.remove(&record.guid)?;
            Ok(())
        }
    }
}
