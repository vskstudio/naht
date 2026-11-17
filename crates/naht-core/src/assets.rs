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
    /// The upload itself failed (network, auth, quota). Pauses only this asset's path.
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
/// failure surfaces here and pauses only this asset, so the caller keeps syncing everything else.
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
/// is the rewrite hook the build/sync pipeline calls when `[assets]` is enabled.
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
/// A failed upload (architecture §8) pauses **only** that property's path; the rest of the snapshot
/// is still rewritten. The caller surfaces these and, in a live session, pauses the named paths.
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
/// files, are left untouched. The build/sync pipeline calls this when `[assets]` is enabled.
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
mod tests {
    use super::*;
    use crate::hash::content_hash;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeUploader {
        uploads: RefCell<Vec<String>>,
        fail: bool,
        fail_on: Option<String>,
    }

    impl AssetUploader for FakeUploader {
        fn upload(&self, name: &str, _content: &[u8]) -> Result<String, AssetError> {
            if self.fail || self.fail_on.as_deref() == Some(name) {
                return Err(AssetError::Upload(UploadFailure::Transport {
                    source: format!("simulated failure for {name}").into(),
                }));
            }
            self.uploads.borrow_mut().push(name.to_owned());
            let id = self.uploads.borrow().len();
            Ok(format!("rbxassetid://{id}"))
        }
    }

    #[test]
    fn a_new_asset_is_uploaded_once_and_its_id_cached_and_reused() {
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();

        let first = resolve_asset(&uploader, &store, "mesh.obj", b"mesh-bytes").unwrap();
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

        let failed = resolve_asset(&failing, &store, "broken.obj", b"x");
        assert!(matches!(failed, Err(AssetError::Upload(_))));
        assert!(
            store
                .cached_asset(&asset_cache_key(b"x"))
                .unwrap()
                .is_none()
        );

        let ok = resolve_asset(&working, &store, "fine.obj", b"y").unwrap();
        assert_eq!(ok, "rbxassetid://1");
    }

    #[test]
    fn resolve_ref_uploads_a_local_file_but_leaves_existing_ids_and_missing_paths_alone() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("meshes/rock.obj", "obj-bytes");

        let resolved = resolve_asset_ref(&uploader, &store, &vfs, "meshes/rock.obj").unwrap();
        assert_eq!(resolved.as_deref(), Some("rbxassetid://1"));

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
            .with_property("MeshId", Variant::String("assets/rock.obj".to_owned()))
            .with_property("Name", Variant::String("Rock".to_owned()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);
        assert!(failures.is_empty());

        assert_eq!(
            snapshot.properties.get("MeshId"),
            Some(&Variant::String("rbxassetid://1".to_owned()))
        );
        assert_eq!(
            snapshot.properties.get("Name"),
            Some(&Variant::String("Rock".to_owned()))
        );
    }

    #[test]
    fn a_failing_asset_is_isolated_while_its_siblings_still_upload() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader {
            fail_on: Some("a.obj".to_owned()),
            ..Default::default()
        };
        let vfs = MemoryVfs::new()
            .with_file("a.obj", "aaa")
            .with_file("b.obj", "bbb")
            .with_file("c.obj", "ccc");

        let mesh = |name: &str, file: &str| {
            Snapshot::new("MeshPart", name)
                .with_property("MeshId", Variant::String(file.to_owned()))
        };
        let mut root = Snapshot::new("Model", "Root")
            .with_child(mesh("A", "a.obj"))
            .with_child(mesh("B", "b.obj"))
            .with_child(mesh("C", "c.obj"));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut root);

        let mesh_id = |name: &str| {
            root.children
                .iter()
                .find(|child| child.name == name)
                .and_then(|child| child.properties.get("MeshId"))
                .cloned()
        };
        assert_eq!(mesh_id("A"), Some(Variant::String("a.obj".to_owned())));
        assert_eq!(
            mesh_id("B"),
            Some(Variant::String("rbxassetid://1".to_owned()))
        );
        assert_eq!(
            mesh_id("C"),
            Some(Variant::String("rbxassetid://2".to_owned()))
        );

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, "Root/A");
        assert_eq!(failures[0].property, "MeshId");
        assert!(matches!(failures[0].error, AssetError::Upload(_)));

        assert_eq!(uploader.uploads.borrow().len(), 2);
    }

    #[test]
    fn a_content_property_absent_from_the_old_name_list_is_rewritten() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("textures/paint.png", "png");

        let mut snapshot = Snapshot::new("SurfaceAppearance", "Paint")
            .with_property("ColorMap", Variant::String("textures/paint.png".to_owned()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);

        assert!(failures.is_empty());
        assert_eq!(
            snapshot.properties.get("ColorMap"),
            Some(&Variant::String("rbxassetid://1".to_owned()))
        );
    }

    #[test]
    fn a_content_property_inherited_from_a_superclass_is_rewritten() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("meshes/bolt.obj", "obj");

        let mut snapshot = Snapshot::new("SpecialMesh", "Bolt")
            .with_property("MeshId", Variant::String("meshes/bolt.obj".to_owned()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);

        assert!(failures.is_empty());
        assert_eq!(
            snapshot.properties.get("MeshId"),
            Some(&Variant::String("rbxassetid://1".to_owned()))
        );
    }

    #[test]
    fn a_local_file_under_a_non_content_property_is_reported_and_never_uploaded() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("notes.txt", "private notes");

        let mut snapshot = Snapshot::new("StringValue", "Notes")
            .with_property("Value", Variant::String("notes.txt".to_owned()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, "Notes");
        assert_eq!(failures[0].property, "Value");
        assert!(matches!(
            failures[0].error,
            AssetError::NotAContentProperty { .. }
        ));
        assert_eq!(
            snapshot.properties.get("Value"),
            Some(&Variant::String("notes.txt".to_owned()))
        );
        assert!(uploader.uploads.borrow().is_empty());
    }

    #[test]
    fn a_non_content_property_naming_a_directory_is_not_reported() {
        use crate::vfs::MemoryVfs;
        let store = StateStore::open_in_memory().unwrap();
        let uploader = FakeUploader::default();
        let vfs = MemoryVfs::new().with_file("Scripts/Main.luau", "print(1)");

        let mut snapshot = Snapshot::new("StringValue", "Notes")
            .with_property("Value", Variant::String("Scripts".to_owned()));

        let failures = rewrite_snapshot_assets(&uploader, &store, &vfs, &mut snapshot);

        assert!(failures.is_empty());
    }

    #[test]
    fn a_cache_row_keyed_by_the_legacy_64_bit_hash_is_not_served() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .cache_asset(&content_hash(b"v1"), "rbxassetid://777")
            .unwrap();
        let uploader = FakeUploader::default();

        let id = resolve_asset(&uploader, &store, "img.png", b"v1").unwrap();

        assert_ne!(id, "rbxassetid://777");
        assert_eq!(uploader.uploads.borrow().len(), 1);
    }
}
