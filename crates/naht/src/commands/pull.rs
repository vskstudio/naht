use super::error::CommandError;
use super::files::canonical;
use crate::config::Config;
use crate::daemon_port;
use naht_core::protocol::{ServerInfo, from_msgpack};
use reqwest::StatusCode;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

const NUDGE_TIMEOUT: Duration = Duration::from_secs(2);

/// What a re-sync nudge found at the daemon recorded for a project.
#[derive(Debug, PartialEq, Eq)]
pub enum ResyncOutcome {
    /// No daemon is recorded for the project, or the recorded port refuses connections.
    NotRunning,
    /// The running daemon is this project's, answered, and re-synced.
    Resynced,
    /// The recorded port belongs to a daemon of another project.
    ForeignDaemon {
        /// The project name the other daemon reports.
        project: String,
    },
    /// A daemon is recorded but did not re-sync; the reason completes "the running daemon ...".
    Failed(String),
}

enum InfoFailure {
    Request(reqwest::Error),
    Status(StatusCode),
    NotNaht,
}

impl InfoFailure {
    fn describe(&self) -> String {
        match self {
            Self::Request(error) if error.is_timeout() => {
                format!("did not answer within {} s", NUDGE_TIMEOUT.as_secs())
            }
            Self::Request(error) => format!("could not be reached ({error})"),
            Self::Status(status) => format!("answered {status}"),
            Self::NotNaht => "answered, but not as a naht daemon".to_owned(),
        }
    }
}

/// Asks the daemon recorded for the project at `root` to re-sync, within the nudge timeout.
pub async fn resync_running(root: &Path) -> ResyncOutcome {
    let Some(port) = daemon_port::read(root) else {
        return ResyncOutcome::NotRunning;
    };
    let config = match Config::load(root) {
        Ok(config) => config,
        Err(error) => return ResyncOutcome::Failed(format!("could not be checked ({error})")),
    };
    match fetch_info(port).await {
        Ok(info)
            if belongs_to_project(&info, &project_name(root, &config), config.serve.place_id) =>
        {
            ResyncOutcome::Resynced
        }
        Ok(info) => ResyncOutcome::ForeignDaemon {
            project: info.project_name,
        },
        Err(InfoFailure::Request(error)) if error.is_connect() => ResyncOutcome::NotRunning,
        Err(failure) => ResyncOutcome::Failed(failure.describe()),
    }
}

/// Ask a running daemon to re-sync now (Studio ↔ filesystem). Full on-demand Studio → filesystem
/// pull lands with the Stage 6 plugin; for now this nudges the daemon's reconnect re-diff.
///
/// # Errors
///
/// Returns [`CommandError`] when the daemon is unreachable within the nudge timeout, fails, or
/// belongs to another project.
pub async fn pull(root: &Path, config: &Config) -> Result<(), CommandError> {
    let port = daemon_port::read(root).unwrap_or_else(|| config.port());
    let project = project_name(root, config);
    match fetch_info(port).await {
        Ok(info) if belongs_to_project(&info, &project, config.serve.place_id) => Ok(()),
        Ok(info) => Err(CommandError::ForeignDaemon {
            port,
            project: info.project_name,
        }),
        Err(InfoFailure::Status(status)) => Err(CommandError::DaemonStatus { status }),
        Err(InfoFailure::Request(source)) => Err(CommandError::DaemonUnreachable {
            url: info_url(port),
            source,
        }),
        Err(InfoFailure::NotNaht) => Err(CommandError::NotNahtDaemon {
            url: info_url(port),
        }),
    }
}

async fn fetch_info(port: u16) -> Result<ServerInfo, InfoFailure> {
    let client = nudge_client().map_err(InfoFailure::Request)?;
    let response = client
        .get(info_url(port))
        .send()
        .await
        .map_err(InfoFailure::Request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(InfoFailure::Status(status));
    }
    let bytes = response.bytes().await.map_err(InfoFailure::Request)?;
    from_msgpack(&bytes).map_err(|_decode| InfoFailure::NotNaht)
}

fn info_url(port: u16) -> String {
    format!("http://{}:{port}/info", Ipv4Addr::LOCALHOST)
}

fn nudge_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(NUDGE_TIMEOUT)
        .timeout(NUDGE_TIMEOUT)
        .build()
}

fn project_name(root: &Path, config: &Config) -> String {
    let root = canonical(root).unwrap_or_else(|_| root.to_path_buf());
    config.project_name(&root)
}

fn belongs_to_project(info: &ServerInfo, project: &str, place_id: Option<u64>) -> bool {
    info.project_name == project
        && place_id.is_none_or(|configured| info.serve_place_id == Some(configured))
}

#[cfg(test)]
mod tests {
    use super::belongs_to_project;
    use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo};

    fn info(project: &str, place: Option<u64>) -> ServerInfo {
        ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: "test".to_owned(),
            project_name: project.to_owned(),
            session_id: "test".to_owned(),
            serve_place_id: place,
        }
    }

    #[test]
    fn a_configured_place_must_match_the_daemon_place() {
        assert!(belongs_to_project(&info("demo", Some(1)), "demo", Some(1)));
        assert!(!belongs_to_project(&info("demo", Some(2)), "demo", Some(1)));
        assert!(!belongs_to_project(&info("demo", None), "demo", Some(1)));
    }

    #[test]
    fn an_unconfigured_place_accepts_any_daemon_place() {
        assert!(belongs_to_project(&info("demo", Some(2)), "demo", None));
        assert!(belongs_to_project(&info("demo", None), "demo", None));
    }

    #[test]
    fn a_different_project_name_never_belongs() {
        assert!(!belongs_to_project(&info("other", None), "demo", None));
    }
}
