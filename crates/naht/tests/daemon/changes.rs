use naht_core::protocol::Change;
use naht_core::reconciler::{Direction, PatchKind};

use super::support::Harness;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::time;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fs_change_appears_via_patches() {
    let harness = Harness::start(None).await;
    harness.write("Greeter.luau", "return 1");

    assert!(harness.info(None).await.status().is_success());

    let batch = harness.patches(0).await;
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
    let harness = Harness::start(None).await;

    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "New.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .await;

    assert!(status.is_success());
    assert_eq!(harness.read("New.luau"), "return 5");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_naming_a_path_windows_forbids_is_refused_400_and_touches_nothing() {
    let harness = Harness::start(None).await;

    let refused = harness
        .post_changes_refused(vec![
            Change::Upsert {
                path: "Good.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 1".to_owned(),
            },
            Change::Upsert {
                path: "src/a?b.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 2".to_owned(),
            },
        ])
        .await;

    assert_eq!(refused, vec!["src/a?b.luau".to_owned()]);
    assert!(!harness.root.join("Good.luau").exists());
    assert!(!harness.root.join("src").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_reports_paths_that_escape_the_project_root() {
    let harness = Harness::start(None).await;
    let sentinel = format!(
        "outside-{}.luau",
        harness.root.file_name().unwrap().to_string_lossy()
    );
    let escaping = format!("../{sentinel}");

    let refused = harness
        .post_changes_refused(vec![Change::Upsert {
            path: escaping.clone(),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .await;

    assert_eq!(refused, vec![escaping]);
    assert!(!harness.root.parent().unwrap().join(sentinel).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changes_body_nested_too_deep_is_refused_and_the_daemon_keeps_answering() {
    let harness = Harness::start(None).await;

    let status = harness.post_changes_nested(100_000).await;

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{status}");
    assert!(harness.info(None).await.status().is_success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mixed_batch_is_refused_whole_and_names_each_refused_path() {
    let harness = Harness::start(None).await;

    let refused = harness
        .post_changes_refused(vec![
            Change::Upsert {
                path: "Good.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 1".to_owned(),
            },
            Change::Upsert {
                path: "Bad.txt".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 2".to_owned(),
            },
        ])
        .await;

    assert_eq!(refused, vec!["Bad.txt".to_owned()]);
    assert!(!harness.root.join("Good.luau").exists());
    assert!(!harness.root.join("Bad.txt").exists());
}

fn upsert(path: &str) -> Change {
    Change::Upsert {
        path: path.to_owned(),
        class: "ModuleScript".to_owned(),
        content: "return 1".to_owned(),
    }
}

fn delete(path: &str) -> Change {
    Change::Delete {
        path: path.to_owned(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_deleting_a_folder_and_all_its_children_removes_the_directory() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness
            .post_changes(vec![upsert("src/Folder/A.luau")])
            .await,
        reqwest::StatusCode::OK
    );

    let status = harness
        .post_changes(vec![delete("src/Folder"), delete("src/Folder/A.luau")])
        .await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!harness.root.join("src/Folder/A.luau").exists());
    assert!(!harness.root.join("src/Folder").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_deleting_the_last_child_of_a_folder_removes_the_emptied_folder() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness
            .post_changes(vec![upsert("src/Folder/A.luau")])
            .await,
        reqwest::StatusCode::OK
    );

    let status = harness
        .post_changes(vec![delete("src/Folder/A.luau")])
        .await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!harness.root.join("src/Folder/A.luau").exists());
    assert!(!harness.root.join("src/Folder").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_last_file_deleted_under_a_top_level_source_root_keeps_the_root() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness.post_changes(vec![upsert("src/A.luau")]).await,
        reqwest::StatusCode::OK
    );

    let status = harness.post_changes(vec![delete("src/A.luau")]).await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!harness.root.join("src/A.luau").exists());
    assert!(harness.root.join("src").is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_emptied_folder_that_still_holds_a_non_synced_file_is_kept_with_200() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness
            .post_changes(vec![upsert("src/Folder/A.luau")])
            .await,
        reqwest::StatusCode::OK
    );
    fs::write(harness.root.join("src/Folder/notes.txt"), "keep").unwrap();

    let status = harness
        .post_changes(vec![delete("src/Folder/A.luau")])
        .await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!harness.root.join("src/Folder/A.luau").exists());
    assert!(harness.root.join("src/Folder").is_dir());
    assert!(harness.root.join("src/Folder/notes.txt").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_the_disk_refuses_to_remove_keeps_the_batch_and_answers_200() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness
            .post_changes(vec![upsert("src/Folder/A.luau"), upsert("lib/Keep.luau")])
            .await,
        reqwest::StatusCode::OK
    );
    let src = harness.root.join("src");
    fs::set_permissions(&src, fs::Permissions::from_mode(0o555)).unwrap();
    let refused_writes = fs::write(src.join("probe.txt"), "probe").is_err();

    let status = harness
        .post_changes(vec![delete("src/Folder/A.luau"), upsert("lib/New.luau")])
        .await;

    fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(refused_writes, "this test needs a user without root rights");
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!harness.root.join("src/Folder/A.luau").exists());
    assert!(harness.root.join("lib/New.luau").exists());
    assert!(harness.root.join("src/Folder").is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_deleting_a_folder_while_its_children_remain_is_refused_naming_it() {
    let harness = Harness::start(None).await;
    assert_eq!(
        harness
            .post_changes(vec![upsert("src/Folder/A.luau")])
            .await,
        reqwest::StatusCode::OK
    );

    let refused = harness
        .post_changes_refused(vec![delete("src/Folder")])
        .await;

    assert_eq!(refused, vec!["src/Folder".to_owned()]);
    assert!(harness.root.join("src/Folder/A.luau").exists());
    assert!(harness.root.join("src/Folder").is_dir());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_reports_symlink_paths_that_escape_the_project_root() {
    use std::os::unix::fs::symlink;

    let harness = Harness::start(None).await;
    let outside = harness.root.parent().unwrap().join(format!(
        "outside-link-{}",
        harness.root.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&outside, "original").unwrap();
    symlink(&outside, harness.root.join("linked.luau")).unwrap();

    let refused = harness
        .post_changes_refused(vec![Change::Upsert {
            path: "linked.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "overwritten".to_owned(),
        }])
        .await;

    assert_eq!(refused, vec!["linked.luau".to_owned()]);
    assert_eq!(fs::read_to_string(outside).unwrap(), "original");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflicting_change_over_the_wire_writes_markers_and_freezes() {
    let harness = Harness::start(None).await;

    harness.write("m.luau", "local a = 1\n");
    harness.info(None).await;
    let _drain = harness.patches(0).await;

    harness.write("m.luau", "local a = 11\n");
    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "m.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "local a = 22\n".to_owned(),
        }])
        .await;
    assert!(status.is_success());

    let on_disk = harness.read("m.luau");
    assert!(
        on_disk.contains("<<<<<<<"),
        "expected markers, got: {on_disk}"
    );
    assert!(on_disk.contains("local a = 11"));
    assert!(on_disk.contains("local a = 22"));

    let cursor = harness.patches(0).await.cursor;
    harness.write("m.luau", "local a = 33\n");
    harness.info(None).await;
    assert_eq!(harness.patches(cursor).await.patches.len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_rediffs_without_clobbering() {
    let harness = Harness::start(None).await;
    harness.write("a.luau", "v1");

    harness.info(None).await;
    let first = harness.patches(0).await;
    assert_eq!(first.patches.len(), 1);
    harness.ack_all(&first).await;

    harness.info(None).await;
    assert_eq!(harness.patches(first.cursor).await.patches.len(), 0);
    assert_eq!(harness.read("a.luau"), "v1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_half_acked_batch_re_emits_only_the_unacked_path() {
    let harness = Harness::start(None).await;
    harness.write("A.luau", "return 1");
    harness.write("B.luau", "return 2");

    harness.info(None).await;
    let first = harness.patches(0).await;
    assert_eq!(first.patches.len(), 2);

    assert!(harness.ack(vec!["A.luau".to_owned()]).await.is_success());

    harness.info(None).await;
    let again = harness.patches(first.cursor).await;
    let paths: Vec<_> = again
        .patches
        .iter()
        .map(|patch| patch.path.as_str())
        .collect();
    assert_eq!(paths, vec!["B.luau"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_changes_refuses_paths_outside_the_source_allowlist() {
    let harness = Harness::start(None).await;

    for path in ["naht.toml", ".naht/x", "x.txt"] {
        let refused = harness
            .post_changes_refused(vec![Change::Upsert {
                path: path.to_owned(),
                class: "ModuleScript".to_owned(),
                content: "owned".to_owned(),
            }])
            .await;

        assert_eq!(refused, vec![path.to_owned()], "{path}");
        assert!(!harness.root.join(path).exists(), "{path} was written");
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_valid_batch_is_applied_while_a_symlink_sits_in_the_tree() {
    use std::os::unix::fs::symlink;

    let harness = Harness::start(None).await;
    let outside = harness.root.parent().unwrap().join(format!(
        "outside-tree-{}",
        harness.root.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&outside, "original").unwrap();
    symlink(&outside, harness.root.join("linked.luau")).unwrap();

    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "Good.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 1".to_owned(),
        }])
        .await;

    assert!(status.is_success(), "{status}");
    assert_eq!(harness.read("Good.luau"), "return 1");
    assert!(
        fs::symlink_metadata(harness.root.join("linked.luau"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "original");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_that_fails_is_never_written_by_a_later_batch() {
    use std::os::unix::fs::PermissionsExt;

    let harness = Harness::start(None).await;
    assert!(harness.info(None).await.status().is_success());
    let _drain = harness.patches(0).await;

    fs::set_permissions(&harness.root, fs::Permissions::from_mode(0o100)).unwrap();
    let failed = harness
        .post_changes(vec![Change::Upsert {
            path: "Ghost.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 'ghost'".to_owned(),
        }])
        .await;
    fs::set_permissions(&harness.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!failed.is_success(), "{failed}");

    let status = harness
        .post_changes(vec![Change::Upsert {
            path: "Other.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 2".to_owned(),
        }])
        .await;

    assert!(status.is_success(), "{status}");
    assert_eq!(harness.read("Other.luau"), "return 2");
    assert!(!harness.root.join("Ghost.luau").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn posting_the_same_batch_twice_changes_nothing_the_second_time() {
    let harness = Harness::start(None).await;
    let batch = || {
        vec![Change::Upsert {
            path: "Twice.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 7".to_owned(),
        }]
    };

    assert!(harness.post_changes(batch()).await.is_success());
    let events_after_first = harness.activity(0).await.events.len();
    let patches_after_first = harness.patches(0).await.patches.len();

    assert!(harness.post_changes(batch()).await.is_success());

    assert_eq!(harness.read("Twice.luau"), "return 7");
    assert_eq!(harness.activity(0).await.events.len(), events_after_first);
    assert_eq!(harness.patches(0).await.patches.len(), patches_after_first);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_replayed_after_its_answer_was_lost_changes_nothing() {
    let harness = Harness::start(None).await;
    let batch = || {
        vec![Change::Upsert {
            path: "Lost.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 9".to_owned(),
        }]
    };

    let held = harness.state.session_lock().await;
    let answer = time::timeout(Duration::from_millis(300), harness.post_changes(batch())).await;
    assert!(
        answer.is_err(),
        "the first answer arrived while the session lock was held"
    );
    drop(held);
    drop(harness.state.session_lock().await);

    let applied = settled_state(&harness).await;
    assert_eq!(applied.0, "return 9", "the first batch was not applied");

    assert!(harness.post_changes(batch()).await.is_success());
    assert_eq!(settled_state(&harness).await, applied);
}

async fn settled_state(harness: &Harness) -> (String, usize, usize) {
    assert!(harness.info(None).await.status().is_success());
    (
        harness.read("Lost.luau"),
        harness.activity(0).await.events.len(),
        harness.patches(0).await.patches.len(),
    )
}
