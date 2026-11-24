use notify_debouncer_full::notify;
use tracing::dispatcher;

use super::log_error_batch;
use crate::test_logs::captured_logs;

#[test]
fn a_watch_error_batch_is_logged_at_error_level_with_each_error() {
    let (logs, dispatch) = captured_logs();
    dispatcher::with_default(&dispatch, || {
        log_error_batch(&[
            notify::Error::new(notify::ErrorKind::MaxFilesWatch),
            notify::Error::generic("queue overflow"),
        ]);
    });
    let text = logs.text();
    assert_eq!(text.matches("ERROR").count(), 2, "{text}");
    assert!(text.contains("MaxFilesWatch"), "{text}");
    assert!(text.contains("queue overflow"), "{text}");
}
