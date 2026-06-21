//! `naht` — the CLI and localhost sync daemon.
//!
//! This binary owns the HTTP server the Studio plugin talks to, the file watcher, and the SQLite
//! session state; all sync decisions are delegated to [`naht_core`]. The full CLI (clap subcommands,
//! layered config) lands in Stage 5 — for now `naht serve [path]` runs the daemon and any other
//! invocation prints the version.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use naht::server::{self, AppState};
use naht::session::Session;
use naht::watcher;

/// The default localhost port the daemon listens on.
const DEFAULT_PORT: u16 = 34872;

/// How long a long-poll is held open before returning empty, prompting the plugin to re-poll.
const LONG_POLL: Duration = Duration::from_secs(25);

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => {
            let root = args
                .next()
                .map_or_else(|| PathBuf::from("."), PathBuf::from);
            serve(&root).await
        }
        _ => {
            println!(
                "naht {} (naht-core {})",
                env!("CARGO_PKG_VERSION"),
                naht_core::version()
            );
            Ok(())
        }
    }
}

async fn serve(root: &Path) -> Result<()> {
    let root = std::fs::canonicalize(root)
        .with_context(|| format!("project directory not found: {}", root.display()))?;
    let project_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("naht")
        .to_string();

    let store = naht_core::state::StateStore::open(&root.join(".naht").join("state.db"))
        .context("opening the state database")?;
    // The serve-place guard is configured in Stage 5; unguarded for now.
    let session = Session::new(root.clone(), store, project_name, None);
    let state = AppState::new(session, LONG_POLL);

    let _watcher = watcher::spawn(&root, state.clone()).context("starting the file watcher")?;

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    println!("naht serving {} on http://{addr}", root.display());
    server::serve(listener, state).await.context("serving")
}
