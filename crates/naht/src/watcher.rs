//! The filesystem watcher that feeds the reconciler.
//!
//! `notify-debouncer-full` coalesces bursts of filesystem events (editors write-then-rename, etc.)
//! into a single debounced callback. On each batch the session re-scans and reconciles, and parked
//! long-polls are woken. The returned guard must be kept alive, dropping it stops watching.
//!
//! On Linux each directory the walk keeps gets its own watch, so `node_modules`, `target` and
//! dot-directories are never registered with the kernel. Other platforms keep one recursive watch
//! on the root, which is native and cheap there.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use notify_debouncer_full::notify::{self, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use thiserror::Error;

use crate::server::AppState;
use crate::session::Session;
#[cfg(target_os = "linux")]
use directories::{Directories, directories_to_watch};
use naht_core::walk::is_ignored_name;
use tokio::runtime::Handle;

#[cfg(target_os = "linux")]
mod directories;

/// Why the filesystem watcher could not start watching the project.
#[derive(Debug, Error)]
#[error("cannot watch the project directory")]
pub struct WatchError(#[from] notify::Error);

const DEBOUNCE: Duration = Duration::from_millis(200);

type Debounced = Debouncer<RecommendedWatcher, RecommendedCache>;

#[derive(Debug)]
struct Watches {
    debouncer: Debounced,
    #[cfg(target_os = "linux")]
    directories: Directories,
}

/// Keeps the filesystem watch alive for as long as it is held.
#[derive(Debug)]
pub struct WatchGuard {
    _watches: Arc<Mutex<Option<Watches>>>,
}

/// Start watching `root`, reconciling into `state` on each debounced batch. The caller must hold
/// onto the returned guard for as long as watching should continue.
///
/// # Errors
///
/// Returns [`WatchError`] when the watcher cannot be started.
pub fn spawn(root: &Path, state: Arc<AppState>) -> Result<WatchGuard, WatchError> {
    spawn_observed(root, state, |_| {})
}

pub(crate) fn spawn_observed(
    root: &Path,
    state: Arc<AppState>,
    observe: impl Fn(&[DebouncedEvent]) + Send + 'static,
) -> Result<WatchGuard, WatchError> {
    let runtime = Handle::current();
    let watched_root = root.to_path_buf();
    let shared: Arc<Mutex<Option<Watches>>> = Arc::new(Mutex::new(None));
    let weak_shared: Weak<Mutex<Option<Watches>>> = Arc::downgrade(&shared);
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    let debouncer = new_debouncer(
        DEBOUNCE,
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                observe(&events);
                let project_events = events
                    .iter()
                    .filter(|event| is_project_event(&watched_root, event))
                    .count();
                if project_events == 0 {
                    return;
                }
                tracing::debug!(target: "naht::watcher", batch = project_events, "filesystem batch");
                if let Some(slot) = weak_shared.upgrade() {
                    refresh(&slot, &watched_root);
                }
                let state = Arc::clone(&state);
                let tasks = state.tasks().clone();
                tasks.spawn_on(
                    async move {
                        match state.with_session(Session::rescan).await {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                tracing::warn!(target: "naht::watcher", ?error, "rescan failed");
                            }
                            Err(error) => {
                                tracing::warn!(target: "naht::watcher", %error, "rescan task failed");
                            }
                        }
                        state.notify_patches();
                        state.notify_blobs();
                    },
                    &runtime,
                );
            }
            Err(errors) => {
                tracing::warn!(target: "naht::watcher", count = errors.len(), "watch error batch");
            }
        },
    )?;
    *guard = Some(Watches::start(debouncer, root)?);
    drop(guard);
    Ok(WatchGuard { _watches: shared })
}

fn refresh(shared: &Mutex<Option<Watches>>, root: &Path) {
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(watches) = guard.as_mut() {
        watches.refresh(root);
    }
}

impl Watches {
    #[cfg(target_os = "linux")]
    fn start(mut debouncer: Debounced, root: &Path) -> Result<Self, WatchError> {
        debouncer.watch(root, RecursiveMode::NonRecursive)?;
        let mut watches = Self {
            debouncer,
            directories: Directories::from([root.to_path_buf()]),
        };
        watches.sync(root)?;
        Ok(watches)
    }

    #[cfg(not(target_os = "linux"))]
    fn start(mut debouncer: Debounced, root: &Path) -> Result<Self, WatchError> {
        debouncer.watch(root, RecursiveMode::Recursive)?;
        Ok(Self { debouncer })
    }

    #[cfg(target_os = "linux")]
    fn refresh(&mut self, root: &Path) {
        if let Err(error) = self.sync(root) {
            tracing::warn!(target: "naht::watcher", ?error, "cannot update directory watches");
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn refresh(&mut self, _root: &Path) {}

    #[cfg(target_os = "linux")]
    fn sync(&mut self, root: &Path) -> notify::Result<()> {
        let wanted = directories_to_watch(root).map_err(notify::Error::io)?;
        let stale: Vec<_> = self.directories.difference(&wanted).cloned().collect();
        for directory in stale {
            if let Err(error) = self.debouncer.unwatch(&directory) {
                tracing::debug!(target: "naht::watcher", ?error, path = %directory.display(), "watch already released");
            }
            self.directories.remove(&directory);
        }
        for directory in wanted {
            if self.directories.contains(&directory) {
                continue;
            }
            match self
                .debouncer
                .watch(&directory, RecursiveMode::NonRecursive)
            {
                Ok(()) => {
                    self.directories.insert(directory);
                }
                Err(error) => {
                    tracing::warn!(target: "naht::watcher", ?error, path = %directory.display(), "cannot watch directory");
                }
            }
        }
        Ok(())
    }
}

fn is_project_event(root: &Path, event: &DebouncedEvent) -> bool {
    !matches!(event.kind, EventKind::Access(_))
        && event
            .paths
            .iter()
            .any(|path| !is_under_ignored_entry(root, path))
}

fn is_under_ignored_entry(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        relative
            .components()
            .any(|component| component.as_os_str().to_str().is_some_and(is_ignored_name))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
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
        let wanted = directories_to_watch(&root).unwrap();
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
}
