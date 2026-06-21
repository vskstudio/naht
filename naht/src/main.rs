//! `naht` — the CLI and localhost sync daemon.
//!
//! This binary owns the HTTP server the Studio plugin talks to, the file watcher, and the SQLite
//! session state; all sync decisions are delegated to [`naht_core`]. Stage 0 is scaffolding only —
//! the commands and daemon land in later stages.

fn main() {
    println!(
        "naht {} (naht-core {})",
        env!("CARGO_PKG_VERSION"),
        naht_core::version()
    );
}
