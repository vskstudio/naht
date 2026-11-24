use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::watch_project;

#[test]
fn a_file_written_while_the_batch_that_created_its_directory_is_handled_is_seen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    let nested = root.join("src").join("Nested");
    let written = nested.join("B.luau");
    let seen_paths: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let wrote = AtomicBool::new(false);
    let nested_seen = nested.clone();
    let written_by_handler = written.clone();
    let handler_paths = Arc::clone(&seen_paths);
    let _guard = watch_project(
        &root,
        |_| {},
        move |events| {
            let paths: Vec<PathBuf> = events
                .iter()
                .flat_map(|event| event.paths.iter().cloned())
                .collect();
            if paths.contains(&nested_seen) && !wrote.swap(true, Ordering::SeqCst) {
                fs::write(&written_by_handler, "return 2").unwrap();
            }
            handler_paths.lock().unwrap().extend(paths);
        },
    )
    .unwrap();

    fs::create_dir(&nested).unwrap();

    let seen = (0..50).any(|_| {
        thread::sleep(Duration::from_millis(100));
        seen_paths.lock().unwrap().contains(&written)
    });
    assert!(
        seen,
        "the write made while the handler ran was not seen, so the watch on the new directory came after the handler"
    );
}
