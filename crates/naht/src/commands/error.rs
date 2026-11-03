use crate::config::ConfigError;
use crate::uploader::UploaderError;
use crate::watcher::WatchError;
use naht_core::build::BuildError;
use naht_core::mapper::MapError;
use naht_core::reconciler::ReconcileError;
use naht_core::state::StateError;
use reqwest::StatusCode;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use thiserror::Error;
use toml::ser;

/// Why a CLI or daemon command failed, one variant per failure a caller may tell apart. The
/// message names the layer's failure; the binary joins the chain of causes when it reports it.
#[derive(Debug, Error)]
pub enum CommandError {
    /// `init` refuses to overwrite an existing `naht.toml`.
    #[error("`{}` already exists; refusing to overwrite", path.display())]
    ProjectExists {
        /// The existing project file.
        path: PathBuf,
    },
    /// A directory the command needs could not be created.
    #[error("cannot create the directory `{}`", path.display())]
    CreateDir {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// An input file could not be read.
    #[error("cannot read `{}`", path.display())]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A file could not be written.
    #[error("cannot write `{}`", path.display())]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The build output could not be created.
    #[error("cannot create the output `{}`", path.display())]
    Create {
        /// The output file that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The project directory does not exist or cannot be resolved.
    #[error("project directory not found: `{}`", path.display())]
    ProjectDir {
        /// The directory as the operator gave it.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A project's source tree could not be read as Roblox instances.
    #[error("cannot read the project tree at `{}`", path.display())]
    Tree {
        /// The tree root that failed.
        path: PathBuf,
        /// The mapping failure.
        #[source]
        source: MapError,
    },
    /// A Rojo project file is not valid JSON for the fields Naht migrates.
    #[error("cannot parse the Rojo project `{}`", path.display())]
    RojoParse {
        /// The Rojo project file.
        path: PathBuf,
        /// The JSON error, with its position.
        #[source]
        source: serde_json::Error,
    },
    /// The migrated `naht.toml` could not be rendered.
    #[error("cannot render naht.toml")]
    RenderConfig(#[source] ser::Error),
    /// The state database could not be opened at its path.
    #[error("cannot open the state database `{}`", path.display())]
    StateOpen {
        /// The state database path.
        path: PathBuf,
        /// The store failure.
        #[source]
        source: StateError,
    },
    /// A state store operation failed after the database was opened.
    #[error(transparent)]
    State(#[from] StateError),
    /// A reconcile step failed.
    #[error(transparent)]
    Reconcile(#[from] ReconcileError),
    /// The layered configuration is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The asset uploader could not be built from the configuration.
    #[error(transparent)]
    Uploader(#[from] UploaderError),
    /// A model or place could not be encoded.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// The filesystem watcher could not start.
    #[error(transparent)]
    Watch(#[from] WatchError),
    /// A running daemon could not be reached on its port.
    #[error("cannot reach the daemon at `{url}` (is `naht serve` running?)")]
    DaemonUnreachable {
        /// The address that was tried.
        url: String,
        /// The transport failure.
        #[source]
        source: reqwest::Error,
    },
    /// A running daemon answered with a failure status.
    #[error("the daemon answered with {status}")]
    DaemonStatus {
        /// The status the daemon returned.
        status: StatusCode,
    },
    /// The daemon's port could not be bound.
    #[error("cannot bind {addr}")]
    Bind {
        /// The address that was requested.
        addr: SocketAddr,
        /// The bind failure.
        #[source]
        source: io::Error,
    },
    /// The HTTP server stopped with an error.
    #[error("the server stopped with an error")]
    Serve(#[source] io::Error),
    /// A line of command output could not be written.
    #[error("cannot write command output")]
    Output(#[source] io::Error),
    /// A release tag does not name the workspace version.
    #[error("release tag `{tag}` does not match workspace version `{want}`")]
    ReleaseMismatch {
        /// The tag as given.
        tag: String,
        /// The workspace version it should name.
        want: String,
    },
}
