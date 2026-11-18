use std::collections::BTreeSet;
use std::path::Path;

use naht_core::protocol::Change;
use naht_core::vfs::{validate_directory_path, validate_source_path};

use super::rejection::Rejection;
use crate::session::Session;

pub(super) fn refused_text_paths(session: &Session, changes: &[Change]) -> Vec<String> {
    let deleted: BTreeSet<&Path> = changes.iter().filter_map(deleted_path).collect();
    changes
        .iter()
        .filter(|change| is_refused_text_change(session, change, &deleted))
        .map(|change| change_path(change).to_owned())
        .collect()
}

fn is_refused_text_change(session: &Session, change: &Change, deleted: &BTreeSet<&Path>) -> bool {
    let path = change_path(change);
    if session.validate_path(path).is_err() {
        return true;
    }
    if session.is_directory(path) {
        return !removes_whole_directory(session, change, deleted);
    }
    validate_source_path(path).is_err()
}

fn removes_whole_directory(session: &Session, change: &Change, deleted: &BTreeSet<&Path>) -> bool {
    match change {
        Change::Delete { path } => {
            validate_directory_path(path).is_ok()
                && session.subtree_deleted_by(Path::new(path), deleted)
        }
        Change::Upsert { .. } => false,
    }
}

fn deleted_path(change: &Change) -> Option<&Path> {
    match change {
        Change::Delete { path } => Some(Path::new(path)),
        Change::Upsert { .. } => None,
    }
}

fn change_path(change: &Change) -> &str {
    match change {
        Change::Upsert { path, .. } | Change::Delete { path } => path,
    }
}

pub(super) fn refuse_directories<'a>(
    session: &Session,
    paths: impl Iterator<Item = &'a str>,
) -> Result<(), Rejection> {
    let refused: Vec<String> = paths
        .filter(|path| session.is_directory(path))
        .map(str::to_owned)
        .collect();
    if refused.is_empty() {
        Ok(())
    } else {
        Err(Rejection::RefusedPaths(refused))
    }
}
