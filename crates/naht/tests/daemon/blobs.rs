use naht_core::binary::BlobInstance;
use naht_core::protocol::Change;
use naht_core::reconciler::Direction;
use reqwest::StatusCode;

use super::support::Harness;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_blobs_rejects_paths_that_escape_the_project_root() {
    let harness = Harness::start(None).await;
    let sentinel = format!(
        "outside-{}.terrain",
        harness.root.file_name().unwrap().to_string_lossy()
    );

    let status = harness
        .post_blobs(vec![BlobInstance::new(
            format!("../{sentinel}"),
            "Terrain",
            vec![1, 2, 3],
        )])
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!harness.root.parent().unwrap().join(sentinel).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_sync_disabled_does_not_read_or_write_blobs() {
    let harness = Harness::start_with_terrain(None, false).await;
    harness.write("Terrain.terrain", b"disk-terrain");

    assert!(harness.info(None).await.status().is_success());
    assert!(harness.blobs(0).await.patches.is_empty());

    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "Studio.terrain",
            "Terrain",
            b"studio-terrain".to_vec(),
        )])
        .await;
    assert!(status.is_success());
    assert!(!harness.root.join("Studio.terrain").exists());
    assert_eq!(harness.read_bytes("Terrain.terrain"), b"disk-terrain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_blob_syncs_both_ways_byte_for_byte() {
    let harness = Harness::start(None).await;
    let blob: Vec<u8> = (0u16..400).map(|index| (index % 256) as u8).collect();

    harness.write("Terrain.terrain", &blob);
    assert!(harness.info(None).await.status().is_success());
    let batch = harness.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Terrain.terrain");
    assert_eq!(patch.class, "Terrain");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.content.as_deref(), Some(blob.as_slice()));

    let other: Vec<u8> = (0u16..256).rev().map(|index| (index % 256) as u8).collect();
    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "World.terrain",
            "Terrain",
            other.clone(),
        )])
        .await;
    assert!(status.is_success());
    assert_eq!(harness.read_bytes("World.terrain"), other);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_both_sides_blob_change_freezes_through_the_live_daemon() {
    let harness = Harness::start(None).await;

    harness.write("t.terrain", b"base");
    harness.info(None).await;
    let drained = harness.blobs(0).await.cursor;

    harness.write("t.terrain", b"fs-edit");
    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "t.terrain",
            "Terrain",
            b"studio-edit".to_vec(),
        )])
        .await;
    assert!(status.is_success());

    assert_eq!(harness.read_bytes("t.terrain"), b"fs-edit");
    assert!(harness.blobs(drained).await.patches.is_empty());

    harness.write("t.terrain", b"fs-2");
    harness.info(None).await;
    assert!(harness.blobs(drained).await.patches.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_studio_text_edit_drives_the_blob_reconcile() {
    let harness = Harness::start(None).await;
    let blob: Vec<u8> = (0u16..300).map(|index| (index % 256) as u8).collect();

    harness.write("Zone.terrain", &blob);
    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "Script.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    let batch = harness.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    assert_eq!(batch.patches[0].path, "Zone.terrain");
    assert_eq!(batch.patches[0].content.as_deref(), Some(blob.as_slice()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_text_edit_with_no_blob_change_queues_no_blob_patch() {
    let harness = Harness::start(None).await;

    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "Only.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    assert!(harness.blobs(0).await.patches.is_empty());
}
