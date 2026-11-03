use std::path::Path;

use crate::hash::content_hash;
use crate::merge;
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::Vfs;

use super::model::{ReconcileError, Status};
use super::scan::decode;

/// List the frozen (conflicted) paths.
pub fn status(store: &StateStore) -> Result<Status, ReconcileError> {
    Ok(Status {
        conflicted: store.conflicted()?.into_iter().map(|r| r.path).collect(),
    })
}

/// Clear a conflict once the user has removed the markers from the file.
///
/// Refuses while any marker remains; on success the resolved file content becomes the new base and
/// the path unfreezes.
pub fn resolve(vfs: &impl Vfs, store: &StateStore, path: &str) -> Result<(), ReconcileError> {
    let record = store
        .get(path)?
        .ok_or_else(|| ReconcileError::NotConflicted(path.to_string()))?;
    if !record.conflicted {
        return Err(ReconcileError::NotConflicted(path.to_string()));
    }
    let text = decode(path, &vfs.read(Path::new(&record.path))?)?;
    if merge::has_conflict_markers(&text) {
        return Err(ReconcileError::MarkersRemain(path.to_string()));
    }
    advance_base(store, &record.path, &record.class, &text)?;
    Ok(())
}

/// Advance the persisted base for one path to `content`, clearing any conflict flag.
///
/// The daemon calls this when the plugin **acks** a Studio-bound patch: the base is held at the last
/// agreed content until then, so a patch the plugin never applied keeps re-diffing instead of being
/// silently considered synced.
pub fn set_base(
    store: &StateStore,
    path: &str,
    class: &str,
    content: &str,
) -> Result<(), ReconcileError> {
    advance_base(store, path, class, content)
}

pub(super) fn advance_base(
    store: &StateStore,
    path: &str,
    class: &str,
    content: &str,
) -> Result<(), ReconcileError> {
    store.upsert(&base_record(path, class, content, false))?;
    Ok(())
}

pub(super) fn freeze(
    store: &StateStore,
    path: &str,
    class: &str,
    base: &str,
) -> Result<(), ReconcileError> {
    store.upsert(&base_record(path, class, base, true))?;
    Ok(())
}

fn base_record(path: &str, class: &str, content: &str, conflicted: bool) -> InstanceRecord {
    InstanceRecord {
        guid: path.to_string(),
        path: path.to_string(),
        class: class.to_string(),
        content_hash: content_hash(content.as_bytes()),
        base_content: Some(content.as_bytes().to_vec()),
        mtime: 0,
        conflicted,
    }
}
