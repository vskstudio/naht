use std::fs;
#[cfg(target_os = "linux")]
use std::io;
use std::path::PathBuf;
use std::time::Instant;

use naht_core::state::StateStore;
use notify_debouncer_full::notify::Event;
use notify_debouncer_full::notify::event::{AccessKind, AccessMode};

use super::*;
use crate::commands;
use crate::config::{Config, INTERNAL_DIR};
use crate::token::Token;
use tokio::time;

fn event_at(path: &str) -> DebouncedEvent {
    event_of(EventKind::Any, path)
}

fn event_of(kind: EventKind, path: &str) -> DebouncedEvent {
    let event = Event::new(kind).add_path(PathBuf::from(path));
    DebouncedEvent::new(event, Instant::now())
}

#[test]
fn a_read_of_the_tree_is_not_a_project_event() {
    let event = event_of(
        EventKind::Access(AccessKind::Close(AccessMode::Read)),
        "/project/src/Main.luau",
    );
    assert!(!is_project_event(Path::new("/project"), &event));
}

#[tokio::test]
async fn an_idle_daemon_emits_no_batch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    commands::init(&root, false).unwrap();
    let config = Config::load(&root).unwrap();
    let store = StateStore::open(&root.join(INTERNAL_DIR).join("state.db")).unwrap();
    let session = Session::new(
        root.clone(),
        store,
        config.project_name(&root),
        config.serve.place_id,
        config.terrain_sync(),
    )
    .unwrap();
    let state = AppState::new(session, Duration::from_secs(1), Token::for_tests());
    state.session_lock().await.rescan().unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    state.session_lock().await.rescan().unwrap();
    let (before_write, _) = state.session_lock().await.take_patches(0);
    fs::write(root.join("src").join("Added.luau"), "return {}").unwrap();
    let deadline = time::Instant::now() + Duration::from_secs(5);
    while state.session_lock().await.take_patches(0).0 == before_write {
        assert!(
            time::Instant::now() < deadline,
            "watcher never reacted to a write"
        );
        time::sleep(Duration::from_millis(50)).await;
    }
    let (cursor, _) = state.session_lock().await.take_patches(0);
    time::sleep(Duration::from_secs(1)).await;
    let (after, _) = state.session_lock().await.take_patches(0);
    assert_eq!(cursor, after);
}

#[test]
fn a_write_under_the_internal_dir_is_not_a_project_event() {
    let event = event_at("/project/.naht/state.db-wal");
    assert!(!is_project_event(Path::new("/project"), &event));
}

#[test]
fn a_write_in_the_tree_is_a_project_event() {
    let event = event_at("/project/src/Main.luau");
    assert!(is_project_event(Path::new("/project"), &event));
}

#[test]
fn a_write_under_a_vcs_dependency_or_build_dir_is_not_a_project_event() {
    for path in [
        "/project/.git/index",
        "/project/node_modules/pkg/index.js",
        "/project/target/debug/naht",
        "/project/src/.cache/Main.luau",
    ] {
        assert!(
            !is_project_event(Path::new("/project"), &event_at(path)),
            "{path} should be ignored"
        );
    }
}

#[test]
fn a_sibling_named_like_the_internal_dir_without_the_dot_is_a_project_event() {
    let event = event_at("/project/naht-notes/todo.txt");
    assert!(is_project_event(Path::new("/project"), &event));
}

#[cfg(target_os = "linux")]
#[test]
fn the_directories_watched_skip_ignored_names_and_keep_nested_ones() {
    use std::collections::BTreeSet;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for nested in [
        "src/a/b",
        "node_modules/pkg/lib",
        "target/debug",
        ".git/objects",
        ".naht",
    ] {
        fs::create_dir_all(root.join(nested)).unwrap();
    }
    let wanted = directories::directories_to_watch(&root).unwrap();
    let expected: BTreeSet<PathBuf> = [
        root.clone(),
        root.join("src"),
        root.join("src").join("a"),
        root.join("src").join("a").join("b"),
    ]
    .into_iter()
    .collect();
    assert_eq!(wanted, expected);
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_under_node_modules_reaches_no_watch_batch() {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("node_modules").join("pkg").join("lib")).unwrap();
    let seen = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let sink = Arc::clone(&seen);
    let _watch = spawn_observed(&root, Arc::clone(&state), move |events| {
        let mut paths = sink.lock().unwrap();
        paths.extend(events.iter().flat_map(|event| event.paths.clone()));
    })
    .unwrap();
    time::sleep(Duration::from_millis(300)).await;
    fs::write(
        root.join("node_modules")
            .join("pkg")
            .join("lib")
            .join("index.js"),
        "module.exports = 1",
    )
    .unwrap();
    fs::write(root.join("src").join("Control.luau"), "return 1").unwrap();
    for _ in 0..100 {
        let paths = seen.lock().unwrap().clone();
        if paths.iter().any(|path| path.ends_with("Control.luau")) {
            assert!(
                paths.iter().all(|path| !path
                    .components()
                    .any(|part| part.as_os_str() == "node_modules")),
                "an event under node_modules reached the watcher: {paths:?}"
            );
            return;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the control write was never observed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_in_a_new_nested_directory_is_seen() {
    let (_dir, state, root) = watched_project();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    fs::create_dir_all(root.join("src").join("fresh").join("deep")).unwrap();
    fs::write(
        root.join("src")
            .join("fresh")
            .join("deep")
            .join("Early.luau"),
        "return 1",
    )
    .unwrap();
    assert!(
        wait_for_patch(&state, "src/fresh/deep/Early.luau").await,
        "a file written as its directory appeared was never queued"
    );
    fs::write(
        root.join("src")
            .join("fresh")
            .join("deep")
            .join("Late.luau"),
        "return 2",
    )
    .unwrap();
    assert!(
        wait_for_patch(&state, "src/fresh/deep/Late.luau").await,
        "a file written into a new nested directory after the watch was installed was never queued"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_into_a_directory_removed_and_recreated_in_one_batch_is_seen() {
    assert_writes_after_replacement_are_seen(|root| {
        fs::remove_dir_all(root.join("src").join("x")).unwrap();
        fs::create_dir(root.join("src").join("x")).unwrap();
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_into_a_directory_renamed_over_in_one_batch_is_seen() {
    assert_writes_after_replacement_are_seen(|root| {
        fs::rename(root.join("src").join("x"), root.join("src").join("x.old")).unwrap();
        fs::create_dir(root.join("src").join("new")).unwrap();
        fs::rename(root.join("src").join("new"), root.join("src").join("x")).unwrap();
    })
    .await;
}

async fn assert_writes_after_replacement_are_seen(replace: impl FnOnce(&Path)) {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("src").join("x")).unwrap();
    fs::write(root.join("src").join("x").join("Old.luau"), "return 0").unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    time::sleep(Duration::from_millis(300)).await;
    replace(&root);
    fs::write(root.join("src").join("x").join("Early.luau"), "return 1").unwrap();
    assert!(
        wait_for_patch(&state, "src/x/Early.luau").await,
        "a file written into the replaced directory was never queued"
    );
    fs::write(root.join("src").join("x").join("Late.luau"), "return 2").unwrap();
    assert!(
        wait_for_patch(&state, "src/x/Late.luau").await,
        "a write after the replaced directory's batch was never seen"
    );
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct RefusingWatch {
    refused: Option<(PathBuf, fn() -> notify::Error)>,
    released: Vec<PathBuf>,
}

#[cfg(target_os = "linux")]
impl directories::DirectoryWatch for RefusingWatch {
    fn watch_directory(&mut self, path: &Path) -> notify::Result<()> {
        match &self.refused {
            Some((refused, error)) if refused == path => Err(error()),
            _ => Ok(()),
        }
    }

    fn unwatch_directory(&mut self, path: &Path) -> notify::Result<()> {
        self.released.push(path.to_path_buf());
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn out_of_watches() -> notify::Error {
    notify::Error::new(notify::ErrorKind::MaxFilesWatch)
}

#[cfg(target_os = "linux")]
fn vanished() -> notify::Error {
    notify::Error::io(io::Error::from(io::ErrorKind::NotFound))
}

#[cfg(target_os = "linux")]
fn project_with_src() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    (dir, root)
}

#[cfg(target_os = "linux")]
#[test]
fn start_fails_when_the_kernel_refuses_a_directory_for_a_reason_other_than_it_vanished() {
    let (_dir, root) = project_with_src();
    let mut watch = RefusingWatch {
        refused: Some((root.join("src"), out_of_watches)),
        ..RefusingWatch::default()
    };
    let error = directories::start_with(&mut watch, &root).unwrap_err();
    assert!(matches!(error.0.kind, notify::ErrorKind::MaxFilesWatch));
    assert!(
        error.to_string().contains("fs.inotify.max_user_watches"),
        "the watch limit error does not name fs.inotify.max_user_watches: {error}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn start_skips_a_directory_that_vanished_before_its_watch_was_added() {
    let (_dir, root) = project_with_src();
    let mut watch = RefusingWatch {
        refused: Some((root.join("src"), vanished)),
        ..RefusingWatch::default()
    };
    let watched = directories::start_with(&mut watch, &root).unwrap();
    assert!(!watched.contains(&root.join("src")));
    assert!(watched.contains(&root));
}

#[cfg(target_os = "linux")]
#[test]
fn a_refresh_reports_a_directory_the_kernel_refuses_and_leaves_it_unwatched() {
    use std::collections::BTreeSet;

    let (_dir, root) = project_with_src();
    let mut watch = RefusingWatch::default();
    let mut watched = directories::start_with(&mut watch, &root).unwrap();
    watch.refused = Some((root.join("src"), out_of_watches));
    let touched = BTreeSet::from([root.join("src")]);
    let failures = directories::sync(&mut watch, &mut watched, &root, &touched).unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, root.join("src"));
    assert!(!watched.contains(&root.join("src")));
}

#[cfg(target_os = "linux")]
#[test]
fn a_touched_directory_releases_and_rewatches_the_watches_under_it() {
    use std::collections::BTreeSet;

    let (_dir, root) = project_with_src();
    let nested = root.join("src").join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    let mut watch = RefusingWatch::default();
    let mut watched = directories::start_with(&mut watch, &root).unwrap();
    watch.released.clear();
    let touched = BTreeSet::from([root.join("src").join("a")]);
    directories::sync(&mut watch, &mut watched, &root, &touched).unwrap();
    assert!(
        watch.released.contains(&nested),
        "a watch under a touched directory was kept: {:?}",
        watch.released
    );
    assert!(watched.contains(&nested));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_in_a_new_directory_under_a_renamed_parent_is_seen() {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("src").join("a").join("b")).unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    time::sleep(Duration::from_millis(300)).await;
    fs::rename(root.join("src").join("a"), root.join("src").join("z")).unwrap();
    let deep = root.join("src").join("z").join("new").join("deep");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("Early.luau"), "return 1").unwrap();
    assert!(
        wait_for_patch(&state, "src/z/new/deep/Early.luau").await,
        "a file in a new directory under a renamed parent was never queued"
    );
    fs::write(deep.join("Late.luau"), "return 2").unwrap();
    assert!(
        wait_for_patch(&state, "src/z/new/deep/Late.luau").await,
        "a write after the renamed parent's batch was never seen"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_in_a_new_child_of_a_parent_removed_and_recreated_is_seen() {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("src").join("a").join("b")).unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    time::sleep(Duration::from_millis(300)).await;
    fs::remove_dir_all(root.join("src").join("a")).unwrap();
    let child = root.join("src").join("a").join("new");
    fs::create_dir_all(&child).unwrap();
    fs::write(child.join("Early.luau"), "return 1").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/new/Early.luau").await,
        "a file in a new child of a recreated parent was never queued"
    );
    fs::write(child.join("Late.luau"), "return 2").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/new/Late.luau").await,
        "a write after the recreated parent's batch was never seen"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_into_a_recreated_nested_directory_under_a_renamed_parent_is_seen() {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("src").join("a").join("b")).unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    time::sleep(Duration::from_millis(300)).await;
    fs::rename(root.join("src").join("a"), root.join("src").join("z")).unwrap();
    let nested = root.join("src").join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("Early.luau"), "return 1").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/b/Early.luau").await,
        "a file written into a nested directory recreated under a renamed parent was never queued"
    );
    fs::write(nested.join("Late.luau"), "return 2").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/b/Late.luau").await,
        "a write after the recreated nested directory's batch was never seen"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_into_a_recreated_nested_directory_under_a_removed_parent_is_seen() {
    let (_dir, state, root) = watched_project();
    fs::create_dir_all(root.join("src").join("a").join("b")).unwrap();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    time::sleep(Duration::from_millis(300)).await;
    fs::remove_dir_all(root.join("src").join("a")).unwrap();
    let nested = root.join("src").join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("Early.luau"), "return 1").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/b/Early.luau").await,
        "a file written into a nested directory recreated under a removed parent was never queued"
    );
    fs::write(nested.join("Late.luau"), "return 2").unwrap();
    assert!(
        wait_for_patch(&state, "src/a/b/Late.luau").await,
        "a write after the recreated nested directory's batch was never seen"
    );
}

fn watched_project() -> (tempfile::TempDir, Arc<AppState>, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(INTERNAL_DIR).join("state.db")).unwrap();
    let session = Session::new(root.clone(), store, "demo", None, true).unwrap();
    let state = AppState::new(session, Duration::from_secs(1), Token::for_tests());
    (dir, state, root)
}

async fn wait_for_patch(state: &AppState, relative: &str) -> bool {
    for _ in 0..100 {
        let (_, patches) = state.session_lock().await.take_patches(0);
        if patches.iter().any(|entry| entry.patch.path == relative) {
            return true;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[cfg(target_os = "linux")]
fn nested_directories(root: &Path, count: usize) {
    for index in 0..count {
        fs::create_dir_all(
            root.join(format!("g{}", index % 100))
                .join(format!("d{index}")),
        )
        .unwrap();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starting_the_watch_on_ten_thousand_directories_stays_fast() {
    let (_dir, state, root) = watched_project();
    nested_directories(&root, 10_000);
    let started = Instant::now();
    let _watch = spawn(&root, Arc::clone(&state)).unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "starting the watch on 10000 directories took {elapsed:?}, the bound is 5 s"
    );
}
