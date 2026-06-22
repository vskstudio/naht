//! Open Cloud asset upload (architecture §9, Stage 12): turn a *local* binary referenced by a
//! property (a mesh, an image) into an asset id, so the property can become `rbxassetid://…`.
//!
//! `naht-core` defines the [`AssetUploader`] interface and the **upload-once, cache-by-hash** policy;
//! it does no network I/O itself (the real Open Cloud client lives in the `naht` binary). Disabled by
//! default — when off, properties keep their original values, so behavior is unchanged.

use std::path::Path;

use rbx_dom_weak::types::Variant;

use crate::hash::content_hash;
use crate::snapshot::{Snapshot, SOURCE_PROPERTY};
use crate::state::{StateError, StateStore};
use crate::vfs::{Vfs, VfsError};

/// Uploads a local asset blob and returns its asset id (e.g. `rbxassetid://123` or a numeric id).
///
/// Network-bound, so it is a trait: the binary supplies the real Open Cloud client, tests a fake.
pub trait AssetUploader {
    /// Upload `content` (named `name` for diagnostics) and return its asset id.
    fn upload(&self, name: &str, content: &[u8]) -> Result<String, AssetError>;
}

/// Errors from resolving an asset.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// The upload itself failed (network, auth, quota …). Pauses only this asset's path.
    #[error("asset upload failed: {0}")]
    Upload(String),
    /// The asset-id cache (state store) failed.
    #[error(transparent)]
    State(#[from] StateError),
    /// Reading the local asset file failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
}

/// Resolve a local asset blob to an asset id, uploading it **once** and caching the result by content
/// hash. An unchanged blob (same hash) is served from the cache without re-uploading. An upload
/// failure surfaces here and pauses only this asset — the caller keeps syncing everything else.
pub fn resolve_asset(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    name: &str,
    content: &[u8],
) -> Result<String, AssetError> {
    let hash = content_hash(content);
    if let Some(asset_id) = store.cached_asset(&hash)? {
        return Ok(asset_id);
    }
    let asset_id = uploader.upload(name, content)?;
    store.cache_asset(&hash, &asset_id)?;
    Ok(asset_id)
}

/// Resolve a property *value* that may reference a local asset file.
///
/// Returns `Some(asset_id)` when `value` points to an existing file in `vfs` (so the caller rewrites
/// the property), or `None` when it is already an asset id or not a local file (left unchanged). This
/// is the rewrite hook the build/sync pipeline calls when `[assets]` is enabled.
pub fn resolve_asset_ref(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    vfs: &impl Vfs,
    value: &str,
) -> Result<Option<String>, AssetError> {
    // Already an asset reference, or plainly not a local path — leave it untouched.
    if value.starts_with("rbxassetid://") || value.contains("://") {
        return Ok(None);
    }
    let path = Path::new(value);
    if !vfs.exists(path) {
        return Ok(None);
    }
    let content = vfs.read(path)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(value);
    Ok(Some(resolve_asset(uploader, store, name, &content)?))
}

/// One asset property that could not be resolved, so it was left at its original reference.
///
/// A failed upload (architecture §8) pauses **only** that property's path; the rest of the snapshot
/// is still rewritten. The caller surfaces these — and, in a live session, pauses the named paths.
#[derive(Debug)]
pub struct AssetFailure {
    /// The instance path, by name, from the scanned root (e.g. `Model/Rock`).
    pub path: String,
    /// The property that held the local asset reference (e.g. `MeshId`).
    pub property: String,
    /// Why it could not be resolved.
    pub error: AssetError,
}

/// Walk a snapshot and rewrite every string property that references a local asset file to its
/// uploaded asset id (uploading once, cached). Properties that are already asset ids — or aren't
/// local files — are left untouched. The build/sync pipeline calls this when `[assets]` is enabled.
///
/// A single asset that fails to resolve does **not** abort the walk (architecture §8): that property
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
    rewrite_into(uploader, store, vfs, snapshot, path, &mut failures);
    failures
}

fn rewrite_into(
    uploader: &dyn AssetUploader,
    store: &StateStore,
    vfs: &impl Vfs,
    snapshot: &mut Snapshot,
    path: String,
    failures: &mut Vec<AssetFailure>,
) {
    for (key, value) in snapshot.properties.iter_mut() {
        // A script's source is code, not an asset reference — never upload it.
        if key == SOURCE_PROPERTY {
            continue;
        }
        if let Variant::String(text) = value {
            match resolve_asset_ref(uploader, store, vfs, text) {
                Ok(Some(asset_id)) => *value = Variant::String(asset_id),
                Ok(None) => {}
                // Pause only this property's path: leave the reference as-is and record the error,
                // so the siblings below still get uploaded.
                Err(error) => failures.push(AssetFailure {
                    path: path.clone(),
                    property: key.clone(),
                    error,
                }),
            }
        }
    }
    for child in &mut snapshot.children {
        let child_path = format!("{path}/{}", child.name);
        rewrite_into(uploader, store, vfs, child, child_path, failures);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A fake uploader: hands out sequential ids, counts uploads per name, and can be set to fail —
    /// either every upload (`fail`) or only an asset with a given name (`fail_on`).
    #[derive(Default)]
    struct FakeUploader {
        uploads: RefCell<Vec<String>>,
        fail: bool,
        fail_on: Option<String>,
    }

    impl AssetUploader for FakeUploader {
        fn upload(&self, name: &str, _content: &[u8]) -> Result<String, AssetError> {
            if self.fail || self.fail_on.as_deref() == Some(name) {
                return Err(AssetError::Upload(format!("simulated failure for {name}")));
            }
            self.uploads.borrow_mut().push(name.to_string());
            let id = self.uploads.borrow().len();
            Ok(format!("rbxassetid://{id}"))
        }
    }

    #[test]
    fn a_new_asset_is_uploaded_once_and_its_id_cached_and_reused() {
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();

        let first = resolve_asset(&uploader, &store, "mesh.obj", b"mesh-bytes").unwrap();
        // Resolving the same content again must hit the cache, not re-upload.
        let second = resolve_asset(&uploader, &store, "mesh.obj", b"mesh-bytes").unwrap();

        assert_eq!(first, "rbxassetid://1");
        assert_eq!(second, first);
        assert_eq!(uploader.uploads.borrow().len(), 1, "uploaded once");
    }

    #[test]
    fn changed_content_uploads_again_unchanged_content_does_not() {
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();

        let v1 = resolve_asset(&uploader, &store, "img.png", b"v1").unwrap();
        let v2 = resolve_asset(&uploader, &store, "img.png", b"v2").unwrap();
        let v1_again = resolve_asset(&uploader, &store, "img.png", b"v1").unwrap();

        assert_ne!(v1, v2, "different content gets a fresh upload");
        assert_eq!(v1_again, v1, "the original content is still cached");
        assert_eq!(uploader.uploads.borrow().len(), 2);
    }

    #[test]
    fn an_upload_failure_is_isolated_to_that_asset() {
        let store = StateStore::open_in_memory().unwrap();
        let failing = FakeUploader {
            fail: true,
            ..Default::default()
        };
        let working = FakeUploader::default();

        // One asset's upload fails — surfaced as an error, nothing cached for it.
        let failed = resolve_asset(&failing, &store, "broken.obj", b"x");
        assert!(matches!(failed, Err(AssetError::Upload(_))));
        assert!(store.cached_asset(&content_hash(b"x")).unwrap().is_none());

        // A different asset still resolves fine — the failure didn't block the rest.
        let ok = resolve_asset(&working, &store, "fine.obj", b"y").unwrap();
        assert_eq!(ok, "rbxassetid://1");
    }

    #[test]
    fn resolve_ref_uploads_a_local_file_but_leaves_existing_ids_and_missing_paths_alone() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("meshes/rock.obj", "obj-bytes");

        // A local file is uploaded and the caller gets an asset id to write back.
        let resolved = resolve_asset_ref(&uploader, &store, &vfs, "meshes/rock.obj").unwrap();
        assert_eq!(resolved.as_deref(), Some("rbxassetid://1"));

        // An existing asset id is left untouched; a path with no file is left untouched.
        assert_eq!(
            resolve_asset_ref(&uploader, &store, &vfs, "rbxassetid://42").unwrap(),
            None
        );
        assert_eq!(
            resolve_asset_ref(&uploader, &store, &vfs, "meshes/missing.obj").unwrap(),
            None
        );
        assert_eq!(
            uploader.uploads.borrow().len(),
            1,
            "only the real file uploaded"
        );
    }

    #[test]
    fn rewrite_replaces_a_local_asset_property_with_its_uploaded_id() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("assets/rock.obj", "obj");

        let mut snapshot = Snapshot::new("MeshPart", "Rock")
            .with_property("MeshId", Variant::String("assets/rock.obj".to_string()))
            .with_property("Name", Variant::String("Rock".to_string()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);
        assert!(failures.is_empty());

        assert_eq!(
            snapshot.properties.get("MeshId"),
            Some(&Variant::String("rbxassetid://1".to_string()))
        );
        // A non-asset string property is untouched.
        assert_eq!(
            snapshot.properties.get("Name"),
            Some(&Variant::String("Rock".to_string()))
        );
    }

    #[test]
    fn a_failing_asset_is_isolated_while_its_siblings_still_upload() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        // The uploader fails only on `a.obj`; `b.obj` and `c.obj` upload fine.
        let uploader = FakeUploader {
            fail_on: Some("a.obj".to_string()),
            ..Default::default()
        };
        let vfs = MemoryVfs::new()
            .with_file("a.obj", "aaa")
            .with_file("b.obj", "bbb")
            .with_file("c.obj", "ccc");

        let mesh = |name: &str, file: &str| {
            Snapshot::new("MeshPart", name)
                .with_property("MeshId", Variant::String(file.to_string()))
        };
        let mut root = Snapshot::new("Model", "Root")
            .with_child(mesh("A", "a.obj"))
            .with_child(mesh("B", "b.obj"))
            .with_child(mesh("C", "c.obj"));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut root);

        let mesh_id = |name: &str| {
            root.children
                .iter()
                .find(|c| c.name == name)
                .and_then(|c| c.properties.get("MeshId"))
                .cloned()
        };
        // A keeps its original reference; B and C were rewritten to uploaded ids.
        assert_eq!(mesh_id("A"), Some(Variant::String("a.obj".to_string())));
        assert_eq!(
            mesh_id("B"),
            Some(Variant::String("rbxassetid://1".to_string()))
        );
        assert_eq!(
            mesh_id("C"),
            Some(Variant::String("rbxassetid://2".to_string()))
        );

        // The one failure is reported against A's path and property, and is an upload error.
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, "Root/A");
        assert_eq!(failures[0].property, "MeshId");
        assert!(matches!(failures[0].error, AssetError::Upload(_)));

        // The failure didn't skip any sibling: exactly the two good assets uploaded.
        assert_eq!(uploader.uploads.borrow().len(), 2);
    }
}
