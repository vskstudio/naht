use super::error::CommandError;
use super::files::{canonical, open_store};
use super::inspect::{serve_warnings, unsyncable_lines};
use crate::config::Config;
use crate::output::Output;
use crate::server::AppState;
use crate::session::Session;
use naht_core::mapper;
use naht_core::vfs::DiskVfs;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

const LONG_POLL: Duration = Duration::from_secs(25);

/// Run the sync daemon for the project at `root` on `port`, using `config` for the project name and
/// place guard. Warnings and the serving line go to `out` before the session starts.
pub async fn serve<Out: Write, Err: Write>(
    config: Config,
    root: &Path,
    port: u16,
    out: &mut Output<Out, Err>,
) -> Result<(), CommandError> {
    let root = canonical(root)?;
    if let Ok(snapshot) = mapper::snapshot_dir(&DiskVfs::new(), &root) {
        for line in unsyncable_lines(&serve_warnings(&config, &snapshot)) {
            out.warn(line).map_err(CommandError::Output)?;
        }
    }
    let store = open_store(&root)?;
    let session = Session::new(
        root.clone(),
        store,
        config.project_name(&root),
        config.serve.place_id,
        config.terrain_sync(),
    );
    let state = AppState::new(session, LONG_POLL);
    state
        .tasks()
        .spawn(crate::shutdown::cancel_on_signal(state.shutdown().clone()));

    let watcher = crate::watcher::spawn(&root, state.clone())?;

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|source| CommandError::Bind { addr, source })?;
    tracing::info!(target: "naht::server", %addr, root = %root.display(), "naht serving");
    out.say(format_args!(
        "naht serving {} on http://{addr}",
        root.display()
    ))
    .map_err(CommandError::Output)?;
    let served = crate::server::serve(listener, state.clone())
        .await
        .map_err(CommandError::Serve);
    drop(watcher);
    state.tasks().close();
    state.tasks().wait().await;
    served
}
