//! `naht-core` — the brain of Naht.
//!
//! This crate holds everything that decides *what* to sync and *how* to merge, with **zero network
//! I/O**: the virtual filesystem, the file↔instance path mapping, the reconciler, the 3-way merge,
//! the persisted state store, and the wire protocol types. The [`naht`] binary wires these into a
//! daemon and CLI; the Studio plugin stays thin because the logic lives here.
//!
//! It is intentionally testable in isolation — the reconciler runs against an in-memory VFS so its
//! behavior can be verified without touching the disk or the network.

pub mod build;
pub mod frontmatter;
pub mod hash;
pub mod mapper;
pub mod merge;
pub mod protocol;
pub mod reconciler;
pub mod snapshot;
pub mod state;
pub mod vfs;

pub use snapshot::Snapshot;

/// The crate version, as reported by Cargo at build time.
///
/// Exposed so the daemon can surface a single source of truth for the core version in its handshake.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
