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

#[cfg(not(target_os = "linux"))]
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::notify::{self, EventKind, RecommendedWatcher};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};
use thiserror::Error;

use crate::server::AppState;
use crate::session::Session;
#[cfg(target_os = "linux")]
use directories::Directories;
use naht_core::walk::is_ignored_name;
use tokio::runtime::Handle;

#[cfg(target_os = "linux")]
mod directories;
#[cfg(test)]
mod tests;

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
                    refresh(&slot, &watched_root, &events);
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

fn refresh(shared: &Mutex<Option<Watches>>, root: &Path, events: &[DebouncedEvent]) {
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(watches) = guard.as_mut() {
        watches.refresh(root, events);
    }
}

impl Watches {
    #[cfg(target_os = "linux")]
    fn start(mut debouncer: Debounced, root: &Path) -> Result<Self, WatchError> {
        let directories = directories::start_with(&mut debouncer, root)?;
        Ok(Self {
            debouncer,
            directories,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn start(mut debouncer: Debounced, root: &Path) -> Result<Self, WatchError> {
        debouncer.watch(root, RecursiveMode::Recursive)?;
        Ok(Self { debouncer })
    }

    #[cfg(target_os = "linux")]
    fn refresh(&mut self, root: &Path, events: &[DebouncedEvent]) {
        let touched = directories::touched_paths(events);
        match directories::sync(&mut self.debouncer, &mut self.directories, root, &touched) {
            Ok(failures) => {
                for (directory, error) in failures {
                    tracing::error!(target: "naht::watcher", ?error, path = %directory.display(), "cannot watch directory, its changes are not seen");
                }
            }
            Err(error) => {
                tracing::error!(target: "naht::watcher", ?error, "cannot update directory watches, the watch set is stale");
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn refresh(&mut self, _root: &Path, _events: &[DebouncedEvent]) {}
}

pub(crate) fn is_project_event(root: &Path, event: &DebouncedEvent) -> bool {
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
