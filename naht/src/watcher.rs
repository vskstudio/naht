//! The filesystem watcher that feeds the reconciler.
//!
//! `notify-debouncer-full` coalesces bursts of filesystem events (editors write-then-rename, etc.)
//! into a single debounced callback. On each batch the session re-scans and reconciles, and parked
//! long-polls are woken. The returned debouncer must be kept alive — dropping it stops watching.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};

use crate::server::AppState;

/// How long to wait for a burst of filesystem events to settle before reconciling.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Start watching `root` recursively, reconciling into `state` on each debounced batch. The caller
/// must hold onto the returned guard for as long as watching should continue.
pub fn spawn(root: &Path, state: Arc<AppState>) -> Result<impl Drop> {
    let runtime = tokio::runtime::Handle::current();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // A failed batch (e.g. a transient watch error) is skipped; the next event re-syncs, and a
        // reconnect re-diff catches anything missed. We never crash the watch thread.
        if result.is_ok() {
            let state = state.clone();
            runtime.spawn(async move {
                let mut session = state.session_lock().await;
                let _ = session.rescan();
                drop(session);
                state.notify_patches();
            });
        }
    })?;
    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}
