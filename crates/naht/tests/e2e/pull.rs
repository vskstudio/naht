use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_silent_daemon_does_not_hang_the_pull() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    let silent = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    write_config_port(&root, free_port());
    write_port_file(&root, silent_port);

    let started = Instant::now();
    let out = run_pull(&root).await;
    let elapsed = started.elapsed();
    drop(silent);

    assert!(!out.status.success(), "a silent daemon must fail the pull");
    assert!(
        elapsed < Duration::from_secs(5),
        "the pull waited {elapsed:?} for a silent daemon"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(&format!("http://127.0.0.1:{silent_port}/info")),
        "the pull did not use the daemon.port file: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_follows_the_daemon_port_file_over_the_config_port() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    write_config_port(&root, free_port());

    let daemon = Daemon::start(&root).await;
    let out = run_pull(&root).await;

    assert!(
        out.status.success(),
        "naht pull failed: {:?} {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "pulled, daemon re-synced\n"
    );
    drop(daemon);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_refuses_a_daemon_on_the_config_port_when_no_port_file_records_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);

    let daemon = Daemon::start(&root).await;
    let port: u16 = daemon.base.rsplit(':').next().unwrap().parse().unwrap();
    drop(fs::remove_file(root.join(".naht").join("daemon.port")));
    write_config_port(&root, port);
    let out = run_pull(&root).await;

    assert!(
        !out.status.success(),
        "pull must not trust an unrecorded daemon"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("`.naht/daemon.port` is missing"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    drop(daemon);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_daemon_removes_its_port_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);

    let mut daemon = Daemon::start(&root).await;
    let port_file = root.join(".naht").join("daemon.port");
    assert!(port_file.exists(), "a running daemon must record its port");

    let pid = daemon.child.id().to_string();
    let signalled = Command::new("sh")
        .arg("-c")
        .arg(format!("kill -TERM {pid}"))
        .status()
        .unwrap();
    assert!(
        signalled.success(),
        "could not signal the daemon: {signalled}"
    );
    let started = Instant::now();
    while daemon.child.try_wait().unwrap().is_none() {
        assert!(
            started.elapsed() < READY_DEADLINE,
            "the daemon did not stop within {READY_DEADLINE:?}"
        );
        sleep(POLL_INTERVAL).await;
    }
    assert!(
        !port_file.exists(),
        "a stopped daemon left its port file behind"
    );
}
