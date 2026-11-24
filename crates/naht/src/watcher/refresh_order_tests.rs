use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use super::watch_project;

#[test]
fn a_file_written_while_the_batch_that_created_its_directory_is_handled_is_seen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    let nested = root.join("src").join("Nested");
    let written = nested.join("B.luau");
    let (sender, received) = mpsc::channel::<Vec<PathBuf>>();
    let wrote = AtomicBool::new(false);
    let nested_seen = nested.clone();
    let written_by_handler = written.clone();
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
            sender.send(paths).unwrap();
        },
    )
    .unwrap();

    fs::create_dir(&nested).unwrap();

    let seen = (0..50).any(|_| {
        received
            .recv_timeout(Duration::from_millis(100))
            .is_ok_and(|paths| paths.contains(&written))
    });
    assert!(
        seen,
        "the write made while the handler ran was not seen, so the watch on the new directory came after the handler"
    );
}
