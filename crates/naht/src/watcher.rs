//! The filesystem watcher that feeds the reconciler.
//!
//! `notify-debouncer-full` coalesces bursts of filesystem events (editors write-then-rename, etc.)
//! into a single debounced callback. On each batch the session re-scans and reconciles, and parked
//! long-polls are woken. The returned debouncer must be kept alive, dropping it stops watching.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_full::notify::{self, EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent, new_debouncer};
use thiserror::Error;

use crate::config::INTERNAL_DIR;
use crate::server::AppState;
use crate::session::Session;
use tokio::runtime::Handle;

/// Why the filesystem watcher could not start watching the project.
#[derive(Debug, Error)]
#[error("cannot watch the project directory")]
pub struct WatchError(#[from] notify::Error);

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Start watching `root` recursively, reconciling into `state` on each debounced batch. The caller
/// must hold onto the returned guard for as long as watching should continue.
///
/// # Errors
///
/// Returns [`WatchError`] when the watcher cannot be started.
pub fn spawn(root: &Path, state: Arc<AppState>) -> Result<impl Drop + use<>, WatchError> {
    let runtime = Handle::current();
    let watched_root = root.to_path_buf();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        match result {
            Ok(events) => {
                let project_events = events
                    .iter()
                    .filter(|event| is_project_event(&watched_root, event))
                    .count();
                if project_events == 0 {
                    return;
                }
                tracing::debug!(target: "naht::watcher", batch = project_events, "filesystem batch");
                let state = Arc::clone(&state);
                let tasks = state.tasks().clone();
                tasks.spawn_on(
                    async move {
                        match state.with_session(Session::rescan).await {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                tracing::warn!(target: "naht::watcher", %error, "rescan failed");
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
        }
    })?;
    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

fn is_project_event(root: &Path, event: &DebouncedEvent) -> bool {
    let internal = root.join(INTERNAL_DIR);
    !matches!(event.kind, EventKind::Access(_))
        && event.paths.iter().any(|path| !path.starts_with(&internal))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::Instant;

    use naht_core::state::StateStore;
    use notify_debouncer_full::notify::Event;
    use notify_debouncer_full::notify::event::{AccessKind, AccessMode};

    use super::*;
    use crate::commands;
    use crate::config::Config;
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
        );
        let state = AppState::new(session, Duration::from_secs(1));
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
    fn a_sibling_named_like_the_internal_dir_is_a_project_event() {
        let event = event_at("/project/.naht-notes/todo.txt");
        assert!(is_project_event(Path::new("/project"), &event));
    }
}
