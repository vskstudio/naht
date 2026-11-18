use super::error::CommandError;
use super::files::{canonical, open_store};
use super::inspect::{serve_warnings, unsyncable_lines};
use super::rojo::checked_tree_path;
use crate::config::Config;
use crate::daemon_port;
use crate::output::Output;
use crate::server;
use crate::server::AppState;
use crate::session::Session;
use crate::shutdown;
use crate::token;
use crate::watcher;
use naht_core::mapper;
use naht_core::vfs::DiskVfs;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task;

const LONG_POLL: Duration = Duration::from_secs(25);

/// Run the sync daemon for the project at `root` on `port`, using `config` for the project name and
/// place guard. Warnings and the serving line go to `out` before the session starts.
///
/// # Errors
///
/// Returns [`CommandError`] when the server cannot bind or fails while running.
pub async fn serve<Stdout: Write, Stderr: Write>(
    config: Config,
    root: &Path,
    port: u16,
    out: &mut Output<Stdout, Stderr>,
) -> Result<(), CommandError> {
    checked_tree_paths(&config)?;
    let limits = config.resolved_limits()?;
    let root = canonical(root)?;
    let _instance = take_instance_lock(&root)?;
    let loaded = token::load_or_create(&root)?;
    let startup_snapshot = {
        let root = root.clone();
        task::spawn_blocking(move || mapper::snapshot_dir(&DiskVfs::new(), &root)).await
    };
    match startup_snapshot {
        Ok(Ok(snapshot)) => {
            for line in unsyncable_lines(&serve_warnings(&config, &snapshot)) {
                out.warn(line).map_err(CommandError::Output)?;
            }
        }
        Ok(Err(error)) => {
            tracing::warn!(target: "naht::serve", ?error, "startup warnings skipped: tree unreadable");
        }
        Err(error) => {
            tracing::warn!(target: "naht::serve", %error, "startup warnings skipped: snapshot task failed");
        }
    }
    let store = open_store(&root)?;
    let mut session = Session::new(
        root.clone(),
        store,
        config.project_name(&root),
        config.serve.place_id,
        config.terrain_sync(),
    )?;
    session.set_limits(limits);
    session.protect_source_roots(
        config
            .tree
            .iter()
            .filter_map(|node| node.path.as_deref())
            .map(PathBuf::from),
    )?;
    let daemon_id = session.daemon_id().to_owned();
    let state = AppState::new(session, LONG_POLL, loaded.token);
    state
        .tasks()
        .spawn(shutdown::cancel_on_signal(state.shutdown().clone()));

    let watcher = watcher::spawn(&root, Arc::clone(&state))?;

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| CommandError::Bind { addr, source })?;
    daemon_port::write(&root, port, &daemon_id).map_err(|source| CommandError::Write {
        path: daemon_port::path(&root),
        source,
    })?;
    if loaded.created {
        out.say(format_args!(
            "naht created the token at {}: run `naht token` to print it for the Studio plugin",
            token::path(&root).display()
        ))
        .map_err(CommandError::Output)?;
    }
    tracing::info!(target: "naht::server", %addr, root = %root.display(), "naht serving");
    out.say(format_args!(
        "naht serving {} on http://{addr}",
        root.display()
    ))
    .map_err(CommandError::Output)?;
    let served = server::serve(listener, Arc::clone(&state))
        .await
        .map_err(CommandError::Serve);
    let unpublished = daemon_port::remove(&root).map_err(|source| CommandError::Write {
        path: daemon_port::path(&root),
        source,
    });
    state.shutdown().cancel();
    drop(watcher);
    state.tasks().close();
    state.tasks().wait().await;
    served.and(unpublished)
}

fn take_instance_lock(root: &Path) -> Result<File, CommandError> {
    let state_dir = root.join(".naht");
    fs::create_dir_all(&state_dir).map_err(|source| CommandError::CreateDir {
        path: state_dir.clone(),
        source,
    })?;
    let path = state_dir.join("daemon.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| CommandError::Lock {
            path: path.clone(),
            source,
        })?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(CommandError::AlreadyRunning {
            path: root.to_path_buf(),
        }),
        Err(TryLockError::Error(source)) => Err(CommandError::Lock { path, source }),
    }
}

fn checked_tree_paths(config: &Config) -> Result<(), CommandError> {
    for path in config.tree.iter().filter_map(|node| node.path.as_deref()) {
        checked_tree_path(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};
    use std::path::Path;
    use std::time::Duration;

    use super::{CommandError, Config, Output, checked_tree_paths, serve};
    use crate::config::{ConfigError, LimitsConfig, TreeNode};
    use tokio::time;

    fn config_with_tree_path(path: &str) -> Config {
        Config {
            tree: vec![TreeNode {
                instance: "ReplicatedStorage/Src".to_owned(),
                path: Some(path.to_owned()),
                ..TreeNode::default()
            }],
            ..Config::default()
        }
    }

    #[test]
    fn serve_refuses_a_tree_path_with_a_parent_component() {
        for path in ["a/../src", "../src", "src/.."] {
            assert!(
                matches!(
                    checked_tree_paths(&config_with_tree_path(path)),
                    Err(CommandError::TreePathOutsideRoot { .. })
                ),
                "serve accepted the tree path `{path}`"
            );
        }
    }

    #[test]
    fn serve_refuses_an_absolute_tree_path() {
        assert!(
            matches!(
                checked_tree_paths(&config_with_tree_path("/abs/src")),
                Err(CommandError::TreePathOutsideRoot { .. })
            ),
            "serve accepted an absolute tree path"
        );
    }

    #[test]
    fn serve_accepts_tree_paths_that_stay_inside_the_root() {
        for path in ["src", "./src", "packages/Shared"] {
            assert!(
                checked_tree_paths(&config_with_tree_path(path)).is_ok(),
                "serve refused the tree path `{path}`"
            );
        }
    }

    #[test]
    fn serve_refuses_a_tree_path_naming_a_windows_device_or_a_forbidden_character() {
        for path in ["src/NUL", "CON/src", "src/lpt9.txt", "src/a<b", "src\\lib"] {
            assert!(
                matches!(
                    checked_tree_paths(&config_with_tree_path(path)),
                    Err(CommandError::TreePathNotPortable { .. })
                ),
                "serve accepted the tree path `{path}`"
            );
        }
    }

    #[tokio::test]
    async fn a_dot_tree_path_is_accepted_and_serve_reaches_the_bind() {
        for path in [".", "./"] {
            let root = tempfile::tempdir().unwrap();
            let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = taken.local_addr().unwrap().port();
            let mut out = Output::new(Vec::new(), Vec::new());
            let outcome = time::timeout(
                Duration::from_secs(10),
                serve(config_with_tree_path(path), root.path(), port, &mut out),
            )
            .await;
            assert!(
                matches!(outcome, Ok(Err(CommandError::Bind { .. }))),
                "serve refused the tree path `{path}`: {outcome:?}"
            );
        }
    }

    async fn serve_for_a_moment(root: &Path) -> String {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut stdout = Vec::new();
        let mut out = Output::new(&mut stdout, Vec::new());
        let outcome = time::timeout(
            Duration::from_secs(3),
            serve(Config::default(), root, port, &mut out),
        )
        .await;
        assert!(outcome.is_err(), "serve returned early: {outcome:?}");
        String::from_utf8(stdout).unwrap()
    }

    #[tokio::test]
    async fn the_first_serve_says_where_the_token_went_and_a_later_serve_stays_quiet() {
        let root = tempfile::tempdir().unwrap();
        let first = serve_for_a_moment(root.path()).await;
        let later = serve_for_a_moment(root.path()).await;
        assert_eq!(
            first.matches("naht created the token").count(),
            1,
            "{first}"
        );
        assert!(!later.contains("naht created the token"), "{later}");
    }

    #[tokio::test]
    async fn a_refused_tree_path_is_refused_before_the_instance_lock_is_taken() {
        let root = tempfile::tempdir().unwrap();
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        let mut out = Output::new(Vec::new(), Vec::new());
        let outcome = time::timeout(
            Duration::from_secs(10),
            serve(
                config_with_tree_path("a/../src"),
                root.path(),
                port,
                &mut out,
            ),
        )
        .await;
        assert!(
            matches!(outcome, Ok(Err(CommandError::TreePathOutsideRoot { .. }))),
            "serve did not refuse the tree path first: {outcome:?}"
        );
        assert!(
            !root.path().join(".naht").exists(),
            "serve wrote .naht before refusing the tree path"
        );
    }

    #[tokio::test]
    async fn an_out_of_range_limit_is_refused_before_the_instance_lock_is_taken() {
        let root = tempfile::tempdir().unwrap();
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        let config = Config {
            limits: LimitsConfig {
                connections: Some(0),
                ..LimitsConfig::default()
            },
            ..Config::default()
        };
        let mut out = Output::new(Vec::new(), Vec::new());
        let outcome = time::timeout(
            Duration::from_secs(10),
            serve(config, root.path(), port, &mut out),
        )
        .await;
        assert!(
            matches!(
                outcome,
                Ok(Err(CommandError::Config(ConfigError::LimitOutOfRange {
                    key: "connections",
                    value: 0,
                    ..
                })))
            ),
            "serve did not refuse the limit first: {outcome:?}"
        );
        assert!(
            !root.path().join(".naht").exists(),
            "serve wrote .naht before refusing the limit"
        );
    }

    #[tokio::test]
    async fn a_taken_port_fails_instead_of_hanging() {
        let root = tempfile::tempdir().unwrap();
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        let mut out = Output::new(Vec::new(), Vec::new());
        let outcome = time::timeout(
            Duration::from_secs(10),
            serve(Config::default(), root.path(), port, &mut out),
        )
        .await;
        assert!(matches!(outcome, Ok(Err(CommandError::Bind { .. }))));
    }
}
