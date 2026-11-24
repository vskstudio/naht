use std::path::PathBuf;
use std::sync::Weak;
use std::sync::atomic::{AtomicUsize, Ordering};

use notify_debouncer_full::notify;
use tracing::dispatcher;

use super::BatchHandler;
use crate::test_logs::captured_logs;

#[test]
fn a_watch_error_batch_is_logged_once_with_its_count() {
    let batches = AtomicUsize::new(0);
    let handler = BatchHandler {
        root: PathBuf::from("/project"),
        watches: Weak::new(),
        observe: |_: &[_]| {
            batches.fetch_add(1, Ordering::SeqCst);
        },
        on_project_batch: |_: &[_]| {
            batches.fetch_add(1, Ordering::SeqCst);
        },
    };
    let (logs, dispatch) = captured_logs();
    dispatcher::with_default(&dispatch, || {
        handler.handle(Err(vec![
            notify::Error::new(notify::ErrorKind::MaxFilesWatch),
            notify::Error::generic("queue overflow"),
        ]));
    });
    let text = logs.text();
    assert_eq!(text.matches("ERROR").count(), 1, "{text}");
    assert!(text.contains("count=2"), "{text}");
    assert!(text.contains("MaxFilesWatch"), "{text}");
    assert_eq!(batches.load(Ordering::SeqCst), 0);
}

#[test]
fn an_empty_error_batch_logs_nothing() {
    let handler = BatchHandler {
        root: PathBuf::from("/project"),
        watches: Weak::new(),
        observe: |_: &[_]| {},
        on_project_batch: |_: &[_]| {},
    };
    let (logs, dispatch) = captured_logs();
    dispatcher::with_default(&dispatch, || handler.handle(Err(Vec::new())));
    assert_eq!(logs.text(), "");
}
