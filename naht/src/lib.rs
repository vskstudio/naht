//! `naht` — the localhost sync daemon, as a library.
//!
//! The [`session`] module owns the per-run sync state (the Studio mirror, the patch queue, and the
//! filesystem-bound reconcile); [`server`] is the pure-transport HTTP layer; [`watcher`] feeds
//! filesystem events into the session. The `naht` binary is a thin wrapper that wires these together
//! and runs them. Exposing them as a library lets the integration tests drive a real server end to
//! end through a fake Studio client.

pub mod commands;
pub mod config;
pub mod logging;
pub mod server;
pub mod session;
pub mod watcher;
