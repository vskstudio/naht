//! Drives the real `naht` binary through a daemon restart and a CLI resolve over HTTP.

#![cfg(test)]

use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use naht::session::Session;
use naht_core::protocol::{CLIENT_HEADER, Change, PatchBatch, Pong, SequencedPatch};
use naht_core::reconciler::{Direction, Patch, PatchKind};
use naht_core::state::StateStore;
use reqwest::Client;
use tokio::time::sleep;

mod instances;
#[cfg(test)]
#[path = "../wire/mod.rs"]
mod wire;

const FILE: &str = "src/A.luau";
const READY_DEADLINE: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const START_ATTEMPTS: usize = 5;
const PENDING_WINDOW: Duration = Duration::from_secs(2);
const RESOLVE_LIMIT: Duration = Duration::from_secs(10);

enum Window {
    Patches(Vec<SequencedPatch>),
    Quiet,
}

impl Window {
    fn sequenced(self) -> Vec<SequencedPatch> {
        match self {
            Self::Patches(patches) => patches,
            Self::Quiet => Vec::new(),
        }
    }

    fn patches(self) -> Vec<Patch> {
        self.sequenced()
            .into_iter()
            .map(|sequenced| sequenced.patch)
            .collect()
    }
}

struct Daemon {
    child: Child,
    client: Client,
    quick: Client,
    base: String,
    session: String,
}

impl Daemon {
    async fn start(root: &Path) -> Self {
        for _ in 0..START_ATTEMPTS {
            if let Some(daemon) = Self::try_start(root).await {
                return daemon;
            }
        }
        panic!("naht serve did not start after {START_ATTEMPTS} attempts");
    }

    async fn try_start(root: &Path) -> Option<Self> {
        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_naht"))
            .arg("serve")
            .arg(root)
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut daemon = Self {
            child,
            client: Client::new(),
            quick: Client::builder().timeout(PENDING_WINDOW).build().unwrap(),
            base: format!("http://{}:{port}", Ipv4Addr::LOCALHOST),
            session: String::new(),
        };
        let session = daemon.wait_ready().await?;
        daemon.session = session;
        Some(daemon)
    }

    async fn wait_ready(&mut self) -> Option<String> {
        let started = Instant::now();
        loop {
            if self.child.try_wait().unwrap().is_some() {
                return None;
            }
            if let Ok(resp) = wire::heartbeat(&self.client, &self.base).await
                && resp.status().is_success()
            {
                let pong: Pong = wire::decode(resp).await;
                return Some(pong.session_id);
            }
            assert!(
                started.elapsed() < READY_DEADLINE,
                "naht serve was not ready within {READY_DEADLINE:?}"
            );
            sleep(POLL_INTERVAL).await;
        }
    }

    async fn handshake(&self) {
        let resp = wire::info(&self.client, &self.base, None).await;
        assert!(resp.status().is_success(), "GET /info: {}", resp.status());
    }

    async fn pending_patches(&self) -> Window {
        match self
            .quick
            .get(format!("{}/patches", self.base))
            .header(CLIENT_HEADER, "plugin")
            .query(&[("cursor", "0"), ("session", self.session.as_str())])
            .send()
            .await
        {
            Ok(resp) => Window::Patches(wire::decode::<PatchBatch>(resp).await.patches),
            Err(err) if err.is_timeout() => Window::Quiet,
            Err(err) => panic!("GET /patches failed: {err}"),
        }
    }

    async fn ack(&self, seqs: Vec<u64>) {
        let status = wire::ack(&self.client, &self.base, &self.session, seqs).await;
        assert!(status.is_success(), "POST /ack: {status}");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn init(root: &Path) {
    let status = Command::new(env!("CARGO_BIN_EXE_naht"))
        .arg("init")
        .arg(root)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "naht init failed: {status}");
}

async fn run_bounded(mut command: Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() >= RESOLVE_LIMIT {
            drop(child.kill());
            panic!("naht did not return within {RESOLVE_LIMIT:?}");
        }
        sleep(POLL_INTERVAL).await;
    }
    child.wait_with_output().unwrap()
}

async fn run_resolve(root: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_naht"));
    command.arg("resolve").arg(FILE).arg("--project").arg(root);
    run_bounded(command).await
}

async fn run_pull(root: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_naht"));
    command.arg("pull").arg(root);
    run_bounded(command).await
}

fn write_config_port(root: &Path, port: u16) {
    fs::write(root.join("naht.toml"), format!("[serve]\nport = {port}\n")).unwrap();
}

fn write_port_file(root: &Path, port: u16) {
    fs::create_dir_all(root.join(".naht")).unwrap();
    fs::write(root.join(".naht").join("daemon.port"), port.to_string()).unwrap();
}

fn write_recorded_port(root: &Path, port: u16, daemon_id: &str) {
    fs::create_dir_all(root.join(".naht")).unwrap();
    fs::write(
        root.join(".naht").join("daemon.port"),
        format!("{port}\n{daemon_id}\n"),
    )
    .unwrap();
}

fn freeze_offline(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(FILE), "local a = 1\n").unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let mut session = Session::new(root, store, "demo", None, false).unwrap();
    session.rescan().unwrap();
    let (_, first) = session.take_patches(0);
    let seqs: Vec<u64> = first.iter().map(|patch| patch.seq).collect();
    session.ack(&seqs).unwrap();
    fs::write(root.join(FILE), "local a = 11\n").unwrap();
    session
        .apply_changes(vec![Change::Upsert {
            path: FILE.to_owned(),
            class: "ModuleScript".to_owned(),
            content: "local a = 22\n".to_owned(),
        }])
        .unwrap();
}

fn frozen_with_a_resolved_file(root: &Path) {
    init(root);
    freeze_offline(root);
    fs::write(root.join(FILE), "local a = 99\n").unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_daemon_restart_keeps_a_synced_file_and_emits_no_delete() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(FILE), "return 1").unwrap();

    let first = Daemon::start(&root).await;
    first.handshake().await;
    let patches = first.pending_patches().await.sequenced();
    let seqs: Vec<u64> = patches.iter().map(|patch| patch.seq).collect();
    let paths: Vec<String> = patches
        .iter()
        .map(|patch| patch.patch.path.clone())
        .collect();
    assert!(
        paths.iter().any(|path| path == FILE),
        "the first run did not sync {FILE}: {paths:?}"
    );
    first.ack(seqs).await;
    drop(first);

    let second = Daemon::start(&root).await;
    second.handshake().await;
    let restarted = second.pending_patches().await.patches();

    assert!(root.join(FILE).exists(), "restart deleted a synced file");
    assert!(
        restarted
            .iter()
            .all(|patch| patch.kind != PatchKind::Delete),
        "restart emitted a delete: {restarted:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resolve_from_the_cli_reaches_studio_without_another_action() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    freeze_offline(&root);

    fs::write(root.join(FILE), "local a = 99\n").unwrap();
    let daemon = Daemon::start(&root).await;
    let out = run_resolve(&root).await;
    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );

    let patches = daemon.pending_patches().await.patches();
    assert_eq!(patches.len(), 1, "expected one ToStudio patch: {patches:?}");
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some("local a = 99\n"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_silent_daemon_does_not_hang_the_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    frozen_with_a_resolved_file(&root);
    let silent = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    write_config_port(&root, silent_port);
    write_port_file(&root, silent_port);

    let out = run_resolve(&root).await;
    drop(silent);

    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "resolved src/A.luau, but the running daemon did not answer within 2 s, so it was not re-synced\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_daemon_on_a_non_default_port_gets_the_resolve_nudge() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    frozen_with_a_resolved_file(&root);
    write_config_port(&root, free_port());

    let daemon = Daemon::start(&root).await;
    let out = run_resolve(&root).await;
    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "resolved src/A.luau, daemon re-synced\n"
    );

    let patches = daemon.pending_patches().await.patches();
    assert_eq!(patches.len(), 1, "expected one ToStudio patch: {patches:?}");
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some("local a = 99\n"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resolve_with_no_daemon_file_says_only_that_the_file_is_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    frozen_with_a_resolved_file(&root);

    let out = run_resolve(&root).await;
    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "resolved src/A.luau\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resolve_with_a_dead_daemon_port_says_only_that_the_file_is_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    frozen_with_a_resolved_file(&root);
    write_port_file(&root, free_port());

    let out = run_resolve(&root).await;
    assert!(
        out.status.success(),
        "naht resolve failed: {:?}",
        out.status
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "resolved src/A.luau\n"
    );
}

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
