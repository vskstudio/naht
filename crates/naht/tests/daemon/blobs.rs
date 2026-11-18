use naht_core::binary::BlobInstance;
use naht_core::protocol::Change;
use naht_core::reconciler::Direction;
use reqwest::StatusCode;

use super::support::Harness;

fn terrain_content(tail: &[u8]) -> Vec<u8> {
    let mut content: Vec<u8> = [1_i32, 1, 1]
        .into_iter()
        .flat_map(i32::to_le_bytes)
        .collect();
    content.resize(40, 0);
    content.extend_from_slice(tail);
    content
}

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
    assert_eq!(harness.blobs(0).await.patches.len(), 0);

    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "Terrain.terrain",
            "Terrain",
            terrain_content(b"studio-terrain"),
        )])
        .await;
    assert!(status.is_success());
    assert_eq!(harness.read_bytes("Terrain.terrain"), b"disk-terrain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terrain_blob_syncs_both_ways_byte_for_byte() {
    let harness = Harness::start(None).await;
    let blob: Vec<u8> = (u8::MIN..=u8::MAX).cycle().take(400).collect();

    assert!(harness.post_blobs(vec![]).await.is_success());
    harness.write("Terrain.terrain", &blob);
    assert!(harness.info(None).await.status().is_success());
    let batch = harness.blobs(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Terrain.terrain");
    assert_eq!(patch.class, "Terrain");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.content.as_deref(), Some(blob.as_slice()));

    let other = terrain_content(&(u8::MIN..=u8::MAX).rev().collect::<Vec<u8>>());
    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "Terrain.terrain",
            "Terrain",
            other.clone(),
        )])
        .await;
    assert!(status.is_success());
    assert_eq!(harness.read_bytes("Terrain.terrain"), other);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_blobs_names_each_path_it_refuses_and_writes_none() {
    let harness = Harness::start(None).await;

    let refused = harness
        .post_blobs_refused(vec![BlobInstance::new(
            "World.terrain",
            "Terrain",
            vec![1, 2, 3],
        )])
        .await;

    assert_eq!(refused, vec!["World.terrain".to_owned()]);
    assert!(!harness.root.join("World.terrain").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_both_sides_blob_change_freezes_through_the_live_daemon() {
    let harness = Harness::start(None).await;

    harness.write("Terrain.terrain", b"base");
    harness.info(None).await;
    let drained = harness.blobs(0).await.cursor;

    harness.write("Terrain.terrain", b"fs-edit");
    let status = harness
        .post_blobs(vec![BlobInstance::new(
            "Terrain.terrain",
            "Terrain",
            terrain_content(b"studio-edit"),
        )])
        .await;
    assert!(status.is_success());

    assert_eq!(harness.read_bytes("Terrain.terrain"), b"fs-edit");
    assert_eq!(harness.blobs(drained).await.patches.len(), 0);

    harness.write("Terrain.terrain", b"fs-2");
    harness.info(None).await;
    assert_eq!(harness.blobs(drained).await.patches.len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_studio_text_edit_drives_the_blob_reconcile() {
    let harness = Harness::start(None).await;
    let blob: Vec<u8> = (u8::MIN..=u8::MAX).cycle().take(300).collect();

    assert!(harness.post_blobs(vec![]).await.is_success());
    harness.write("Terrain.terrain", &blob);
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
    assert_eq!(batch.patches[0].path, "Terrain.terrain");
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

    assert_eq!(harness.blobs(0).await.patches.len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_blobs_refuses_paths_outside_the_terrain_allowlist() {
    let harness = Harness::start(None).await;

    for path in ["naht.toml", ".naht/x.terrain", "x.txt"] {
        let status = harness
            .post_blobs(vec![BlobInstance::new(path, "Terrain", vec![1, 2, 3])])
            .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert!(!harness.root.join(path).exists(), "{path} was written");
    }
}
