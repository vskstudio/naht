use std::fs;
use std::path::Path;

use super::{Daemon, init};

pub(super) fn names_port(root: &Path, port: u16) -> bool {
    fs::read_to_string(root.join(".naht").join("daemon.port"))
        .ok()
        .and_then(|contents| contents.lines().next()?.trim().parse::<u16>().ok())
        == Some(port)
}

#[tokio::test]
async fn a_daemon_that_does_not_serve_this_project_is_not_adopted() {
    let foreign_dir = tempfile::tempdir().unwrap();
    let foreign_root = foreign_dir.path().join("foreign");
    init(&foreign_root);
    let foreign = Daemon::start(&foreign_root).await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    let mut impostor = Daemon::spawn(&root, foreign.port, None);

    assert!(impostor.wait_ready(&root).await.is_none());
}
