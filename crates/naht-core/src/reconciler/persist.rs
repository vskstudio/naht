use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::hash::content_hash;
use crate::merge;
use crate::state::{InstanceRecord, StateStore};
use crate::vfs::Vfs;

use super::model::{ReconcileError, Status, TextInstance};
use super::scan::decode;

/// The Studio content each persisted text base implies, to seed a session's Studio mirror on open.
///
/// # Errors
///
/// Returns an error when the state store cannot be read or a base is not UTF-8.
pub fn studio_at_base(
    store: &StateStore,
) -> Result<BTreeMap<String, TextInstance>, ReconcileError> {
    let mut mirror = BTreeMap::new();
    for record in store.all()? {
        let Some(bytes) = record.base_content.as_deref() else {
            continue;
        };
        let text = decode(&record.path, bytes)?;
        mirror.insert(
            record.path.clone(),
            TextInstance::new(record.path, record.class, text),
        );
    }
    for side in store.studio_sides()? {
        match side.content {
            Some(text) => {
                mirror.insert(
                    side.path.clone(),
                    TextInstance::new(side.path, side.class, text),
                );
            }
            None => {
                mirror.remove(&side.path);
            }
        }
    }
    Ok(mirror)
}

/// Track the latest Studio side of a frozen or already tracked path, `None` for a Studio deletion.
///
/// # Errors
///
/// Returns an error when the state store cannot be read or written.
pub fn record_studio_side(
    store: &StateStore,
    path: &str,
    class: &str,
    content: Option<&str>,
) -> Result<(), ReconcileError> {
    let frozen = store.get(path)?.is_some_and(|record| record.conflicted);
    if frozen || store.studio_side(path)?.is_some() {
        store.set_studio_side(path, class, content)?;
    }
    Ok(())
}

/// Track a Studio deletion of a frozen or already tracked path, under the class its base records.
///
/// # Errors
///
/// Returns an error when the state store cannot be read or written.
pub fn record_studio_deletion(store: &StateStore, path: &str) -> Result<(), ReconcileError> {
    let Some(record) = store.get(path)? else {
        return Ok(());
    };
    record_studio_side(store, path, &record.class, None)
}

/// List the frozen (conflicted) paths.
///
/// # Errors
///
/// Returns an error when the state store cannot be read.
pub fn status(store: &StateStore) -> Result<Status, ReconcileError> {
    Ok(Status {
        conflicted: store
            .conflicted()?
            .into_iter()
            .map(|record| record.path)
            .collect(),
    })
}

/// Clear a conflict once the user has removed the markers from the file.
///
/// Refuses while any marker remains. On success the path unfreezes without moving its base: the base
/// still holds the Studio content the conflict was frozen against, so the next reconcile sees the
/// resolved file as a filesystem change and pushes it to Studio.
///
/// # Errors
///
/// Returns an error when `path` has no conflict to resolve, markers remain or the state store fails.
pub fn resolve(vfs: &impl Vfs, store: &StateStore, path: &str) -> Result<(), ReconcileError> {
    let record = store
        .get(path)?
        .ok_or_else(|| ReconcileError::NotConflicted(PathBuf::from(path)))?;
    if !record.conflicted {
        return Err(ReconcileError::NotConflicted(PathBuf::from(path)));
    }
    let text = decode(path, &vfs.read(Path::new(&record.path))?)?;
    if merge::has_conflict_markers(&text) {
        return Err(ReconcileError::MarkersRemain(PathBuf::from(path)));
    }
    store.set_conflicted(&record.guid, false)?;
    Ok(())
}

/// Advance the persisted base for one path to `content`, clearing any conflict flag.
///
/// The daemon calls this when the plugin **acks** a Studio-bound patch: the base is held at the last
/// agreed content until then, so a patch the plugin never applied keeps re-diffing instead of being
/// silently considered synced.
///
/// # Errors
///
/// Returns an error when the base text cannot be stored in the state store.
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
    store.clear_studio_side(path)?;
    Ok(())
}

pub(super) fn freeze(
    store: &StateStore,
    path: &str,
    class: &str,
    studio: &str,
) -> Result<(), ReconcileError> {
    store.upsert(&base_record(path, class, studio, true))?;
    store.clear_studio_side(path)?;
    Ok(())
}

fn base_record(path: &str, class: &str, content: &str, conflicted: bool) -> InstanceRecord {
    InstanceRecord {
        guid: path.to_owned(),
        path: path.to_owned(),
        class: class.to_owned(),
        content_hash: content_hash(content.as_bytes()),
        base_content: Some(content.as_bytes().to_vec()),
        mtime: 0,
        conflicted,
    }
}
