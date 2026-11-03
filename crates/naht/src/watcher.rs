//! The filesystem watcher that feeds the reconciler.
//!
//! `notify-debouncer-full` coalesces bursts of filesystem events (editors write-then-rename, etc.)
//! into a single debounced callback. On each batch the session re-scans and reconciles, and parked
//! long-polls are woken. The returned debouncer must be kept alive, dropping it stops watching.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_full::notify::{self, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent, new_debouncer};
use thiserror::Error;

use crate::config::INTERNAL_DIR;
use crate::server::AppState;
use tokio::runtime::Handle;

/// Why the filesystem watcher could not start watching the project.
#[derive(Debug, Error)]
#[error("cannot watch the project directory")]
pub struct WatchError(#[from] notify::Error);

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Start watching `root` recursively, reconciling into `state` on each debounced batch. The caller
/// must hold onto the returned guard for as long as watching should continue.
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
                        match state.with_session(|session| session.rescan()).await {
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
    event.paths.iter().any(|path| !path.starts_with(&internal))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use notify_debouncer_full::notify::{Event, EventKind};

    use super::*;

    fn event_at(path: &str) -> DebouncedEvent {
        let event = Event::new(EventKind::Any).add_path(PathBuf::from(path));
        DebouncedEvent::new(event, Instant::now())
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
