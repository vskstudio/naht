use naht_core::protocol::Change;
use naht_core::reconciler::{Direction, PatchKind};
use reqwest::StatusCode;

use super::support::Harness;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fs_change_appears_via_patches() {
    let h = Harness::start(None).await;
    h.write("Greeter.luau", "return 1");

    assert!(h.info(None).await.status().is_success());

    let batch = h.patches(0).await;
    assert_eq!(batch.patches.len(), 1);
    let patch = &batch.patches[0];
    assert_eq!(patch.path, "Greeter.luau");
    assert_eq!(patch.class, "ModuleScript");
    assert_eq!(patch.direction, Direction::ToStudio);
    assert_eq!(patch.kind, PatchKind::Create);
    assert_eq!(patch.content.as_deref(), Some("return 1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_writes_the_file() {
    let h = Harness::start(None).await;

    let status = h
        .post_changes(vec![Change::Upsert {
            path: "New.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .await;

    assert!(status.is_success());
    assert_eq!(h.read("New.luau"), "return 5");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_rejects_paths_that_escape_the_project_root() {
    let h = Harness::start(None).await;
    let sentinel = format!(
        "outside-{}.luau",
        h.root.file_name().unwrap().to_string_lossy()
    );

    let status = h
        .post_changes(vec![Change::Upsert {
            path: format!("../{sentinel}"),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!h.root.parent().unwrap().join(sentinel).exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_rejects_symlink_paths_that_escape_the_project_root() {
    use std::os::unix::fs::symlink;

    let h = Harness::start(None).await;
    let outside = h.root.parent().unwrap().join(format!(
        "outside-link-{}",
        h.root.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(&outside, "original").unwrap();
    symlink(&outside, h.root.join("linked.luau")).unwrap();

    let status = h
        .post_changes(vec![Change::Upsert {
            path: "linked.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "overwritten".to_owned(),
        }])
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "original");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflicting_change_over_the_wire_writes_markers_and_freezes() {
    let h = Harness::start(None).await;

    h.write("m.luau", "local a = 1\n");
    h.info(None).await;
    let _drain = h.patches(0).await;

    h.write("m.luau", "local a = 11\n");
    let status = h
        .post_changes(vec![Change::Upsert {
            path: "m.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "local a = 22\n".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    let on_disk = h.read("m.luau");
    assert!(
        on_disk.contains("<<<<<<<"),
        "expected markers, got: {on_disk}"
    );
    assert!(on_disk.contains("local a = 11"));
    assert!(on_disk.contains("local a = 22"));

    let cursor = h.patches(0).await.cursor;
    h.write("m.luau", "local a = 33\n");
    h.info(None).await;
    assert!(h.patches(cursor).await.patches.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_rediffs_without_clobbering() {
    let h = Harness::start(None).await;
    h.write("a.luau", "v1");

    h.info(None).await;
    let first = h.patches(0).await;
    assert_eq!(first.patches.len(), 1);
    h.ack_all(&first).await;

    h.info(None).await;
    assert!(h.patches(first.cursor).await.patches.is_empty());
    assert_eq!(h.read("a.luau"), "v1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_half_acked_batch_re_emits_only_the_unacked_path() {
    let h = Harness::start(None).await;
    h.write("A.luau", "return 1");
    h.write("B.luau", "return 2");

    h.info(None).await;
    let first = h.patches(0).await;
    assert_eq!(first.patches.len(), 2);

    assert!(h.ack(vec!["A.luau".to_owned()]).await.is_success());

    h.info(None).await;
    let again = h.patches(first.cursor).await;
    let paths: Vec<_> = again.patches.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, vec!["B.luau"]);
}
