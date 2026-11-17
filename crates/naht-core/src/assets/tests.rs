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
        Snapshot::new("MeshPart", name).with_property("MeshId", Variant::String(file.to_owned()))
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
