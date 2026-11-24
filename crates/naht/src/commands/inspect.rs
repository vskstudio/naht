use super::error::CommandError;
use super::files::{open_store, rooted};
use crate::config::Config;
use crate::uploader::OpenCloudUploader;
use naht_core::assets;
use naht_core::binary::{self, Keep};
use naht_core::reconciler::ReconcileError;
use naht_core::snapshot::Snapshot;
use naht_core::vfs::{DiskVfs, RootedVfs};
use naht_core::walk::project_key;
use naht_core::{limits, reconciler};
use std::path::{Path, PathBuf};

/// The project's conflict state: the paths frozen pending `resolve`.
///
/// # Errors
///
/// Returns [`CommandError`] when the project cannot be read.
pub fn status(root: &Path) -> Result<reconciler::Status, CommandError> {
    let store = open_store(root)?;
    Ok(reconciler::status(&store)?)
}

/// Clear a conflict at `path`: a text conflict once its markers are removed by hand, or a frozen
/// binary conflict once `keep` names the side to keep.
///
/// # Errors
///
/// Returns [`CommandError`] when `path` cannot be resolved, or when `keep` is missing for a binary
/// conflict or given for a text file.
pub fn resolve(root: &Path, path: &str, keep: Option<Keep>) -> Result<(), CommandError> {
    let path = project_key(Path::new(path));
    let path = path.as_str();
    let store = open_store(root)?;
    let vfs = rooted(root)?;
    let frozen_binary = store
        .get(path)?
        .is_some_and(|record| record.conflicted && record.base_content.is_none());
    match (frozen_binary, keep) {
        (true, Some(keep)) => binary::resolve(&vfs, &store, path, keep)?,
        (true, None) => return Err(ReconcileError::KeepRequired(PathBuf::from(path)).into()),
        (false, None) => reconciler::resolve(&vfs, &store, path)?,
        (false, Some(_)) => return Err(ReconcileError::KeepNotBinary(PathBuf::from(path)).into()),
    }
    Ok(())
}

/// Upload local asset files referenced by properties and rewrite them to `rbxassetid://…`.
///
/// Runs only when `[assets]` is enabled. Disabled by default, so the common case does no network
/// I/O and leaves every property untouched. Returns one warning per asset that stayed at its
/// original reference, so a single bad asset never aborts the whole build.
///
/// # Errors
///
/// Returns [`CommandError`] when an asset cannot be uploaded or the state store fails.
pub fn resolve_assets(root: &Path, snapshot: &mut Snapshot) -> Result<Vec<String>, CommandError> {
    let config = Config::load(root)?;
    if !config.assets.is_enabled() {
        return Ok(Vec::new());
    }
    let store = open_store(root)?;
    let uploader = OpenCloudUploader::from_config(&config.assets)?;
    let vfs = RootedVfs::new(root.to_path_buf(), DiskVfs::new());
    let failures = assets::rewrite_snapshot_assets(&uploader, &store, &vfs, snapshot);
    if failures.is_empty() {
        return Ok(Vec::new());
    }
    let mut warnings = vec![format!(
        "{} asset(s) could not be uploaded:",
        failures.len()
    )];
    warnings.extend(failures.iter().map(|failure| {
        format!(
            "{} ({}) left at its original reference: {}",
            failure.path, failure.property, failure.error
        )
    }));
    Ok(warnings)
}

/// The instances that cannot round-trip, reported as warnings for a `serve` session.
///
/// Honors the project's enabled capabilities, notably `[serve] terrain_sync`, which drives the
/// daemon's blob channel and so stops terrain from being flagged. Uses the same options the running
/// session uses, so the warning matches what actually syncs.
#[must_use]
pub fn serve_warnings(config: &Config, snapshot: &Snapshot) -> Vec<limits::Warning> {
    limits::scan_with(
        snapshot,
        limits::Options {
            terrain_sync: config.terrain_sync(),
        },
    )
}

pub(super) fn unsyncable_lines(warnings: &[limits::Warning]) -> Vec<String> {
    if warnings.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!("{} item(s) cannot live-sync:", warnings.len())];
    lines.extend(warnings.iter().map(limits::Warning::message));
    lines
}
