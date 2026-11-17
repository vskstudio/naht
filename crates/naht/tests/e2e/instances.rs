use std::fs;
use std::process::Command;

use super::{
    Daemon, free_port, frozen_with_a_resolved_file, init, run_bounded, run_pull, run_resolve,
    write_port_file, write_recorded_port,
};

fn port_of(daemon: &Daemon) -> u16 {
    daemon.base.rsplit(':').next().unwrap().parse().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_serve_on_the_same_project_fails_and_leaves_the_first_running() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    let first = Daemon::start(&root).await;
    let port_file = root.join(".naht").join("daemon.port");
    let recorded = fs::read_to_string(&port_file).unwrap();

    let mut second = Command::new(env!("CARGO_BIN_EXE_naht"));
    second
        .arg("serve")
        .arg(&root)
        .arg("--port")
        .arg(free_port().to_string());
    let out = run_bounded(second).await;

    assert!(
        !out.status.success(),
        "a second serve on the same project must fail"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already running"),
        "the refusal is not clear: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&port_file).unwrap(),
        recorded,
        "the second serve rewrote the first daemon's port file"
    );
    first.handshake().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_refuses_a_port_file_that_belongs_to_another_project() {
    let dir = tempfile::tempdir().unwrap();
    let project_a = dir.path().join("demo-a");
    let project_b = dir.path().join("demo-b");
    init(&project_a);
    init(&project_b);
    let daemon_a = Daemon::start(&project_a).await;
    let daemon_b = Daemon::start(&project_b).await;
    write_port_file(&project_b, port_of(&daemon_a));

    let out = run_pull(&project_b).await;
    assert!(
        !out.status.success(),
        "a pull must not nudge another project's daemon"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("belongs to another project") && stderr.contains("demo-a"),
        "the mismatch is not explained: {stderr}"
    );

    let own = run_pull(&project_a).await;
    assert!(
        own.status.success(),
        "the daemon of its own project must still answer: {}",
        String::from_utf8_lossy(&own.stderr)
    );
    drop(daemon_b);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resolve_with_another_projects_port_says_so_and_still_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let project_a = dir.path().join("demo-a");
    let project_b = dir.path().join("demo-b");
    init(&project_a);
    let daemon_a = Daemon::start(&project_a).await;
    frozen_with_a_resolved_file(&project_b);
    write_port_file(&project_b, port_of(&daemon_a));

    let out = run_resolve(&project_b).await;
    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "resolved src/A.luau, but the recorded port belongs to another project (demo-a), so it was not re-synced\n"
    );
    daemon_a.handshake().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_refuses_a_same_named_daemon_that_is_not_the_recorded_one() {
    let dir = tempfile::tempdir().unwrap();
    let project_a = dir.path().join("a").join("game");
    let project_b = dir.path().join("b").join("game");
    init(&project_a);
    init(&project_b);
    let daemon_a = Daemon::start(&project_a).await;
    write_recorded_port(&project_b, port_of(&daemon_a), "not-the-daemon-id");

    let out = run_pull(&project_b).await;
    assert!(
        !out.status.success(),
        "a pull must not accept a daemon whose id is not the recorded one"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("not the one recorded"),
        "the refusal is not explained: {stderr}"
    );
    daemon_a.handshake().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_accepts_the_daemon_whose_id_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    let daemon = Daemon::start(&root).await;

    let out = run_pull(&root).await;
    assert!(
        out.status.success(),
        "the daemon whose id is recorded must answer: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    daemon.handshake().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_names_the_missing_daemon_id_of_a_port_file_from_an_older_naht() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    let daemon = Daemon::start(&root).await;
    write_port_file(&root, port_of(&daemon));

    let out = run_pull(&root).await;
    assert!(
        !out.status.success(),
        "a port file with no daemon id must not be trusted"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("records no daemon id"),
        "the missing id is not explained: {stderr}"
    );
    daemon.handshake().await;
}
