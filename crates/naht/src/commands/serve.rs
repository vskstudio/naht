use super::error::CommandError;
use super::files::{canonical, open_store};
use super::inspect::{serve_warnings, unsyncable_lines};
use crate::config::Config;
use crate::daemon_port;
use crate::output::Output;
use crate::server;
use crate::server::AppState;
use crate::session::Session;
use crate::shutdown;
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
    let root = canonical(root)?;
    let _instance = take_instance_lock(&root)?;
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
    session.protect_source_roots(
        config
            .tree
            .iter()
            .filter_map(|node| node.path.as_deref())
            .map(PathBuf::from),
    );
    let daemon_id = session.daemon_id().to_owned();
    let state = AppState::new(session, LONG_POLL);
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};
    use std::time::Duration;

    use super::{CommandError, Config, Output, serve};
    use tokio::time;

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
