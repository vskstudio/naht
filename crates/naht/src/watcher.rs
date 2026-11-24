//! The filesystem watcher that feeds the reconciler.
//!
//! `notify-debouncer-full` coalesces bursts of filesystem events (editors write-then-rename, etc.)
//! into a single debounced callback. On each batch the session re-scans and reconciles, and parked
//! long-polls are woken. The returned guard must be kept alive, dropping it stops watching.
//!
//! On Linux each directory the walk keeps gets its own watch, so `node_modules`, `target` and
//! dot-directories are never registered with the kernel. Other platforms keep one recursive watch
//! on the root, which is native and cheap there.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

#[cfg(not(target_os = "linux"))]
use notify_debouncer_full::new_debouncer;
#[cfg(not(target_os = "linux"))]
use notify_debouncer_full::notify::RecommendedWatcher;
#[cfg(not(target_os = "linux"))]
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::notify::{self, EventKind};
use notify_debouncer_full::{
    DebounceEventHandler, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};
use thiserror::Error;

use crate::server::AppState;
use crate::session::Session;
#[cfg(target_os = "linux")]
use directories::{Directories, Inotify};
use naht_core::walk::is_ignored_name;
use tokio::runtime::Handle;

#[cfg(target_os = "linux")]
mod directories;
#[cfg(test)]
mod error_log_tests;
#[cfg(all(test, target_os = "linux"))]
mod refresh_order_tests;
#[cfg(test)]
mod tests;

/// Why the filesystem watcher could not start watching the project.
#[derive(Debug, Error)]
#[error("cannot watch the project directory{}", limit_hint(&.0))]
pub struct WatchError(#[from] notify::Error);

fn limit_hint(error: &notify::Error) -> &'static str {
    if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) {
        ": the kernel's watch limit is reached. On Linux, raise `fs.inotify.max_user_watches` (for example `sudo sysctl fs.inotify.max_user_watches=524288`), then run the command again"
    } else {
        ""
    }
}

const DEBOUNCE: Duration = Duration::from_millis(200);

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct Watches {
    _debouncer: Debouncer<Inotify, RecommendedCache>,
    watch: Inotify,
    directories: Directories,
}

#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
struct Watches {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
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
    watch_project(root, observe, move |_events| {
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
    })
}

/// Watch `root` and call `on_project_batch` with each debounced batch that holds a project event.
///
/// `observe` sees every successful batch first, ignored events included. A batch of watch errors
/// is logged at error level, since changes may have been missed.
///
/// # Errors
///
/// Returns [`WatchError`] when the watcher cannot be started.
pub(crate) fn watch_project(
    root: &Path,
    observe: impl Fn(&[DebouncedEvent]) + Send + 'static,
    on_project_batch: impl Fn(&[DebouncedEvent]) + Send + 'static,
) -> Result<WatchGuard, WatchError> {
    let shared: Arc<Mutex<Option<Watches>>> = Arc::new(Mutex::new(None));
    let batches = BatchHandler {
        root: root.to_path_buf(),
        watches: Arc::downgrade(&shared),
        observe,
        on_project_batch,
    };
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    *guard = Some(Watches::start(move |result| batches.handle(result), root)?);
    drop(guard);
    Ok(WatchGuard { _watches: shared })
}

struct BatchHandler<Observe, OnProjectBatch> {
    root: PathBuf,
    watches: Weak<Mutex<Option<Watches>>>,
    observe: Observe,
    on_project_batch: OnProjectBatch,
}

impl<Observe, OnProjectBatch> BatchHandler<Observe, OnProjectBatch>
where
    Observe: Fn(&[DebouncedEvent]),
    OnProjectBatch: Fn(&[DebouncedEvent]),
{
    fn handle(&self, result: DebounceEventResult) {
        match result {
            Ok(events) => self.handle_events(&events),
            Err(errors) => log_error_batch(&errors),
        }
    }

    fn handle_events(&self, events: &[DebouncedEvent]) {
        (self.observe)(events);
        let project_events = events
            .iter()
            .filter(|event| is_project_event(&self.root, event))
            .count();
        if project_events == 0 {
            return;
        }
        tracing::debug!(target: "naht::watcher", batch = project_events, "filesystem batch");
        if let Some(slot) = self.watches.upgrade() {
            refresh(&slot, &self.root, events);
        }
        (self.on_project_batch)(events);
    }
}

fn log_error_batch(errors: &[notify::Error]) {
    if errors.is_empty() {
        return;
    }
    tracing::error!(
        target: "naht::watcher",
        count = errors.len(),
        first = ?errors.first(),
        "watch error from the file watcher, changes may have been missed"
    );
}

fn refresh(shared: &Mutex<Option<Watches>>, root: &Path, events: &[DebouncedEvent]) {
    let mut guard = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(watches) = guard.as_mut() {
        watches.refresh(root, events);
    }
}

#[cfg(target_os = "linux")]
impl Watches {
    fn start(handler: impl DebounceEventHandler, root: &Path) -> Result<Self, WatchError> {
        let (debouncer, mut watch) = directories::debounce(handler)?;
        let directories = directories::start_with(&mut watch, root)?;
        Ok(Self {
            _debouncer: debouncer,
            watch,
            directories,
        })
    }

    fn refresh(&mut self, root: &Path, events: &[DebouncedEvent]) {
        let touched = directories::touched_paths(events);
        match directories::sync(&mut self.watch, &mut self.directories, root, &touched) {
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
}

#[cfg(not(target_os = "linux"))]
impl Watches {
    fn start(handler: impl DebounceEventHandler, root: &Path) -> Result<Self, WatchError> {
        let mut debouncer = new_debouncer(DEBOUNCE, None, handler)?;
        debouncer.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            _debouncer: debouncer,
        })
    }

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
