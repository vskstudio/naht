//! The user-facing commands behind the CLI. Each is a thin orchestration over `naht-core`: the
//! sync brain, the state store, and the model builder. The binary's `main` parses arguments and
//! dispatches here; the integration tests drive these functions directly.

mod build;
mod error;
mod files;
mod init;
mod inspect;
mod pull;
mod release;
mod rojo;
mod serve;

pub use build::{Built, build, build_watch, spawn_build_watcher};
pub use error::CommandError;
pub use init::{Initialized, init};
pub use inspect::{resolve, resolve_assets, serve_warnings, status};
pub use pull::pull;
pub use release::{check_release_version, package_plugin};
pub use rojo::migrate_from_rojo;
pub use serve::serve;
