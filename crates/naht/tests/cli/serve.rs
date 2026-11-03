use naht::commands;
use naht::config::Config;
use naht_core::Snapshot;
use naht_core::limits::Reason;
use std::fs;

#[test]
fn serve_warning_honors_the_terrain_sync_config_flag() {
    let tree = Snapshot::new("Folder", "game")
        .with_child(Snapshot::new("Terrain", "Terrain"))
        .with_child(Snapshot::new("MeshPart", "Rock"));

    let enabled_dir = tempfile::tempdir().unwrap();
    fs::write(
        enabled_dir.path().join("naht.toml"),
        "[serve]\nterrain_sync = true\n",
    )
    .unwrap();
    let enabled = Config::load(enabled_dir.path()).unwrap();
    let reasons: Vec<_> = commands::serve_warnings(&enabled, &tree)
        .into_iter()
        .map(|w| w.reason)
        .collect();
    assert!(!reasons.contains(&Reason::Terrain));
    assert!(reasons.contains(&Reason::MeshBinary));

    let disabled_dir = tempfile::tempdir().unwrap();
    let disabled = Config::load(disabled_dir.path()).unwrap();
    let reasons: Vec<_> = commands::serve_warnings(&disabled, &tree)
        .into_iter()
        .map(|w| w.reason)
        .collect();
    assert!(reasons.contains(&Reason::Terrain));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_fails_clearly_when_no_daemon_is_running() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("naht.toml"), "[serve]\nport = 1\n").unwrap();
    let config = Config::load(dir.path()).unwrap();
    assert!(commands::pull(&config).await.is_err());
}
