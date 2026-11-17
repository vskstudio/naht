//! Open Cloud asset upload (architecture §9, Stage 12): turn a *local* binary referenced by a
//! property (a mesh, an image) into an asset id, so the property can become `rbxassetid://…`.
//!
//! `naht-core` defines the [`AssetUploader`] interface and the **upload-once, cache-by-hash** policy;
//! it does no network I/O itself (the real Open Cloud client lives in the `naht` binary). Disabled by
//! default. When off, properties keep their original values, so behavior is unchanged.

use std::error::Error;
use std::path::Path;

use rbx_dom_weak::types::{Variant, VariantType};
use rbx_reflection::DataType;

use crate::snapshot::{SOURCE_PROPERTY, Snapshot};
use crate::state::{StateError, StateStore};
use crate::vfs::{EntryKind, Vfs, VfsError};

fn asset_cache_key(content: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(content).to_hex())
}

/// Uploads a local asset blob and returns its asset id (e.g. `rbxassetid://123` or a numeric id).
///
/// Network-bound, so it is a trait: the binary supplies the real Open Cloud client, tests a fake.
pub trait AssetUploader {
    /// Upload `content` (named `name` for diagnostics) and return its asset id.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::Upload`] when the upload request fails.
    fn upload(&self, name: &str, content: &[u8]) -> Result<String, AssetError>;
}

/// Why an upload to Open Cloud failed, one variant per outcome a caller may tell apart.
#[derive(Debug, thiserror::Error)]
pub enum UploadFailure {
    /// The request got no answer (connection, TLS, timeout).
    #[error("request failed")]
    Transport {
        /// The transport failure as the HTTP client reported it.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The service answered with a non-success HTTP status (auth, quota, validation).
    #[error("Open Cloud returned HTTP {0}")]
    Rejected(u16),
    /// The answer was not the JSON document the upload expects.
    #[error("unreadable response")]
    Unreadable {
        /// The decoding failure as the parser reported it.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The answer parsed but carried no asset id.
    #[error("no asset id in the Open Cloud response")]
    MissingAssetId,
    /// The thread that performed the upload panicked before it answered.
    #[error("upload thread panicked")]
    WorkerPanicked,
}

/// Errors from resolving an asset.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// The upload itself failed (network, auth, quota). Only this property keeps its reference.
    #[error("asset upload failed")]
    Upload(#[source] UploadFailure),
    /// The asset-id cache (state store) failed.
    #[error(transparent)]
    State(#[from] StateError),
    /// Reading the local asset file failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// The property names a local file but reflection does not declare it as a Content property,
    /// so the file is not uploaded and the reference is left as written.
    #[error("{class}.{property} names a local file but is not a Content property")]
    NotAContentProperty {
        /// The class of the instance that holds the property.
        class: String,
        /// The property that names the file.
        property: String,
    },
}

/// Resolve a local asset blob to an asset id, uploading it once per content hash.
///
/// An unchanged blob (same hash) is served from the cache without re-uploading. An upload
/// failure is returned to the caller, which reports it and resolves every other asset.
///
/// # Errors
///
/// Returns an error when the blob cannot be read, the cache fails or the upload fails.
pub fn resolve_asset(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    name: &str,
    content: &[u8],
) -> Result<String, AssetError> {
    let key = asset_cache_key(content);
    if let Some(asset_id) = store.cached_asset(&key)? {
        return Ok(asset_id);
    }
    let asset_id = uploader.upload(name, content)?;
    store.cache_asset(&key, &asset_id)?;
    Ok(asset_id)
}

/// Resolve a property *value* that may reference a local asset file.
///
/// Returns `Some(asset_id)` when `value` points to an existing file in `vfs` (so the caller rewrites
/// the property), or `None` when it is already an asset id or not a local file (left unchanged). This
/// is the rewrite hook the build pipeline calls when `[assets]` is enabled.
///
/// # Errors
///
/// Returns an error when the blob cannot be read, the cache fails or the upload fails.
pub fn resolve_asset_ref(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    vfs: &impl Vfs,
    value: &str,
) -> Result<Option<String>, AssetError> {
    if is_already_resolved(value) {
        return Ok(None);
    }
    let path = Path::new(value);
    if !vfs.exists(path) {
        return Ok(None);
    }
    let content = vfs.read(path)?;
    let name = path
        .file_name()
        .and_then(|os_name| os_name.to_str())
        .unwrap_or(value);
    Ok(Some(resolve_asset(uploader, store, name, &content)?))
}

fn is_already_resolved(value: &str) -> bool {
    value.starts_with("rbxassetid://") || value.contains("://")
}

/// One asset property that could not be resolved, so it was left at its original reference.
///
/// A failed upload leaves **only** that property at its original reference; the rest of the snapshot
/// is still rewritten. `naht build` reports these as warnings and keeps going.
#[derive(Debug)]
pub struct AssetFailure {
    /// The instance path, by name, from the scanned root (e.g. `Model/Rock`).
    pub path: String,
    /// The property that held the local asset reference (e.g. `MeshId`).
    pub property: String,
    /// Why it could not be resolved.
    pub error: AssetError,
}

/// Rewrite every local asset reference in a snapshot to its uploaded asset id.
///
/// Uploads once and caches each file. Properties that are already asset ids, or are not local
/// files, are left untouched. The build pipeline calls this when `[assets]` is enabled.
///
/// A single asset that fails to resolve does **not** abort the walk: that property
/// is left at its original reference and recorded in the returned failures, while every other asset
/// is still uploaded and rewritten. The returned vector is empty when everything resolved.
#[must_use]
pub fn rewrite_snapshot_assets(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    vfs: &impl Vfs,
    snapshot: &mut Snapshot,
) -> Vec<AssetFailure> {
    let mut failures = Vec::new();
    let path = snapshot.name.clone();
    rewrite_into(uploader, store, vfs, snapshot, &path, &mut failures);
    failures
}

fn is_content_property(class: &str, property: &str) -> bool {
    let Ok(database) = rbx_reflection_database::get() else {
        return false;
    };
    let mut current = Some(class);
    while let Some(class_name) = current {
        let Some(descriptor) = database.classes.get(class_name) else {
            return false;
        };
        if let Some(property_descriptor) = descriptor.properties.get(property) {
            return matches!(
                property_descriptor.data_type,
                DataType::Value(VariantType::Content | VariantType::ContentId)
            );
        }
        current = descriptor.superclass;
    }
    false
}

fn reject_undeclared_file(
    vfs: &impl Vfs,
    class: &str,
    property: &str,
    value: &str,
) -> Result<(), AssetError> {
    if is_already_resolved(value) || !matches!(vfs.kind(Path::new(value)), Ok(EntryKind::File)) {
        return Ok(());
    }
    Err(AssetError::NotAContentProperty {
        class: class.to_owned(),
        property: property.to_owned(),
    })
}

fn rewrite_into(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    vfs: &impl Vfs,
    snapshot: &mut Snapshot,
    path: &str,
    failures: &mut Vec<AssetFailure>,
) {
    let class = snapshot.class.as_str();
    for (key, value) in &mut snapshot.properties {
        if key == SOURCE_PROPERTY {
            continue;
        }
        let Variant::String(text) = value else {
            continue;
        };
        let outcome = if is_content_property(class, key) {
            resolve_asset_ref(uploader, store, vfs, text)
        } else {
            reject_undeclared_file(vfs, class, key, text).map(|()| None)
        };
        match outcome {
            Ok(Some(asset_id)) => *value = Variant::String(asset_id),
            Ok(None) => {}
            Err(error) => failures.push(AssetFailure {
                path: path.to_owned(),
                property: key.clone(),
                error,
            }),
        }
    }
    for child in &mut snapshot.children {
        let child_path = format!("{path}/{}", child.name);
        rewrite_into(uploader, store, vfs, child, &child_path, failures);
    }
}

#[cfg(test)]
mod tests;
