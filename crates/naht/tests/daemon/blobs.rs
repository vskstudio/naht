use naht_core::binary::BlobInstance;
use naht_core::protocol::Change;
use naht_core::reconciler::Direction;
use reqwest::StatusCode;

use super::support::Harness;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_blobs_rejects_paths_that_escape_the_project_root() {
    let h = Harness::start(None).await;
    let sentinel = format!(
        "outside-{}.terrain",
        h.root.file_name().unwrap().to_string_lossy()
    );

    let status = h
        .post_blobs(vec![BlobInstance::new(
            format!("../{sentinel}"),
            "Terrain",
            vec![1, 2, 3],
        )])
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!h.root.parent().unwrap().join(sentinel).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_sync_disabled_does_not_read_or_write_blobs() {
    let h = Harness::start_with_terrain(None, false).await;
    h.write("Terrain.terrain", b"disk-terrain");

    assert!(h.info(None).await.status().is_success());
    assert!(h.blobs(0).await.patches.is_empty());

    let status = h
        .post_blobs(vec![BlobInstance::new(
            "Studio.terrain",
            "Terrain",
            b"studio-terrain".to_vec(),
        )])
        .await;
    assert!(status.is_success());
    assert!(!h.root.join("Studio.terrain").exists());
    assert_eq!(h.read_bytes("Terrain.terrain"), b"disk-terrain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_blob_syncs_both_ways_byte_for_byte() {
    let h = Harness::start(None).await;
    let blob: Vec<u8> = (0u16..400).map(|i| (i % 256) as u8).collect();

    h.write("Terrain.terrain", &blob);
    assert!(h.info(None).await.status().is_success());
    let batch = h.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Terrain.terrain");
    assert_eq!(patch.class, "Terrain");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.content.as_deref(), Some(blob.as_slice()));

    let other: Vec<u8> = (0u16..256).rev().map(|i| (i % 256) as u8).collect();
    let status = h
        .post_blobs(vec![BlobInstance::new(
            "World.terrain",
            "Terrain",
            other.clone(),
        )])
        .await;
    assert!(status.is_success());
    assert_eq!(h.read_bytes("World.terrain"), other);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_both_sides_blob_change_freezes_through_the_live_daemon() {
    let h = Harness::start(None).await;

    h.write("t.terrain", b"base");
    h.info(None).await;
    let drained = h.blobs(0).await.cursor;

    h.write("t.terrain", b"fs-edit");
    let status = h
        .post_blobs(vec![BlobInstance::new(
            "t.terrain",
            "Terrain",
            b"studio-edit".to_vec(),
        )])
        .await;
    assert!(status.is_success());

    assert_eq!(h.read_bytes("t.terrain"), b"fs-edit");
    assert!(h.blobs(drained).await.patches.is_empty());

    h.write("t.terrain", b"fs-2");
    h.info(None).await;
    assert!(h.blobs(drained).await.patches.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_studio_text_edit_drives_the_blob_reconcile() {
    let h = Harness::start(None).await;
    let blob: Vec<u8> = (0u16..300).map(|i| (i % 256) as u8).collect();

    h.write("Zone.terrain", &blob);
    let status = h
        .post_changes(vec![Change::Upsert {
            path: "Script.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    let batch = h.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    assert_eq!(batch.patches[0].path, "Zone.terrain");
    assert_eq!(batch.patches[0].content.as_deref(), Some(blob.as_slice()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_text_edit_with_no_blob_change_queues_no_blob_patch() {
    let h = Harness::start(None).await;

    let status = h
        .post_changes(vec![Change::Upsert {
            path: "Only.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    assert!(h.blobs(0).await.patches.is_empty());
}
