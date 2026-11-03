use super::error::CommandError;
use super::files::{open_store, rooted};
use crate::config::Config;
use crate::uploader::OpenCloudUploader;
use naht_core::assets;
use naht_core::snapshot::Snapshot;
use naht_core::vfs::{DiskVfs, RootedVfs};
use naht_core::{limits, reconciler};
use std::path::Path;

/// The project's conflict state: the paths frozen pending `resolve`.
///
/// # Errors
///
/// Returns [`CommandError`] when the project cannot be read.
pub fn status(root: &Path) -> Result<reconciler::Status, CommandError> {
    let store = open_store(root)?;
    Ok(reconciler::status(&store)?)
}

/// Clear a conflict at `path` once its markers have been removed by hand.
///
/// # Errors
///
/// Returns [`CommandError`] when `path` cannot be resolved.
pub fn resolve(root: &Path, path: &str) -> Result<(), CommandError> {
    let store = open_store(root)?;
    let vfs = rooted(root)?;
    reconciler::resolve(&vfs, &store, path)?;
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
