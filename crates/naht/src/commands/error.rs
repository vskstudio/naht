use crate::config::ConfigError;
use crate::session::SessionError;
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
    /// A `[[tree]]` source path, or a migrated Rojo `$path`, names a location outside the project.
    #[error("`[[tree]]` path `{path}` must stay inside the project root")]
    TreePathOutsideRoot {
        /// The path as written in the project configuration.
        path: String,
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
    /// The sync session could not be opened from the persisted state.
    #[error(transparent)]
    Session(#[from] SessionError),
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
    /// The port recorded for this project belongs to a daemon of another project.
    #[error(
        "no daemon is running for this project: the recorded port {port} belongs to another project (`{project}`)"
    )]
    ForeignDaemon {
        /// The port that answered.
        port: u16,
        /// The project name the answering daemon reports.
        project: String,
    },
    /// The port answers as a daemon of this project's name, but not the one recorded in the port file.
    #[error(
        "port {port} answers as another daemon named `{project}`, not the one recorded in `.naht/daemon.port` (stop that daemon, then restart `naht serve` in this project)"
    )]
    StaleDaemon {
        /// The port that answered.
        port: u16,
        /// The project name the answering daemon reports.
        project: String,
    },
    /// There is no port file, so the daemon that answered cannot be checked.
    #[error(
        "`.naht/daemon.port` is missing, so the daemon on port {port} cannot be verified (restart `naht serve` in this project to write it)"
    )]
    MissingPortFile {
        /// The port that answered.
        port: u16,
    },
    /// The port file predates daemon ids, so the daemon that answered cannot be checked.
    #[error(
        "`.naht/daemon.port` was written by an older naht and records no daemon id, so the daemon on port {port} cannot be verified (restart `naht serve` to rewrite it)"
    )]
    UnverifiedDaemon {
        /// The port that answered.
        port: u16,
    },
    /// The daemon answers from an older naht that reports no daemon id, so it cannot be checked.
    #[error(
        "the daemon on port {port} is from an older naht that reports no daemon id, so it cannot be verified (stop it, then run `naht serve` with this naht version)"
    )]
    OlderDaemon {
        /// The port that answered.
        port: u16,
    },
    /// Something answers at the daemon address, but not with naht's handshake.
    #[error("`{url}` did not answer with naht's handshake")]
    NotNahtDaemon {
        /// The address that was tried.
        url: String,
    },
    /// A daemon is already serving this project, so a second one must not start.
    #[error("`naht serve` is already running for `{}` (see `.naht/daemon.lock`)", path.display())]
    AlreadyRunning {
        /// The project directory whose lock is held.
        path: PathBuf,
    },
    /// The single-instance lock could not be taken for a reason other than contention.
    #[error("cannot take the daemon lock `{}`", path.display())]
    Lock {
        /// The lock file.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
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

#[cfg(test)]
mod tests {
    use super::CommandError;

    #[test]
    fn a_stale_daemon_names_the_other_daemon_and_the_next_step() {
        let message = CommandError::StaleDaemon {
            port: 34872,
            project: "game".to_owned(),
        }
        .to_string();
        assert_eq!(
            message,
            "port 34872 answers as another daemon named `game`, not the one recorded in `.naht/daemon.port` (stop that daemon, then restart `naht serve` in this project)"
        );
    }

    #[test]
    fn a_missing_port_file_is_not_reported_as_an_old_one() {
        let missing = CommandError::MissingPortFile { port: 34872 }.to_string();
        assert!(
            missing.starts_with("`.naht/daemon.port` is missing,"),
            "{missing}"
        );
        assert!(!missing.contains("older naht"), "{missing}");
    }

    #[test]
    fn an_old_port_file_names_the_older_naht() {
        let old = CommandError::UnverifiedDaemon { port: 34872 }.to_string();
        assert!(old.contains("written by an older naht"), "{old}");
        assert!(old.contains("records no daemon id"), "{old}");
    }

    #[test]
    fn an_older_daemon_asks_for_this_naht_version() {
        let older = CommandError::OlderDaemon { port: 34872 }.to_string();
        assert!(older.contains("from an older naht"), "{older}");
        assert!(
            older.contains("run `naht serve` with this naht version"),
            "{older}"
        );
    }
}
