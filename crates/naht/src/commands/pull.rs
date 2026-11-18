use super::error::CommandError;
use super::files::canonical;
use crate::config::Config;
use crate::daemon_port::{self, Recorded};
use naht_core::protocol::{
    CLIENT_HEADER, PLACE_ID_HEADER, ReportedLimits, ServerInfo, from_msgpack,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

const NUDGE_TIMEOUT: Duration = Duration::from_secs(2);

const CLI_CLIENT: &str = "cli";

const INFO_LIMIT: usize = 64 * 1024;

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
    OlderNaht,
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
            Self::OlderNaht => {
                "answered as an older naht that records no daemon id, so it cannot be checked"
                    .to_owned()
            }
        }
    }
}

/// Asks the daemon recorded for the project at `root` to re-sync, within the nudge timeout.
pub async fn resync_running(root: &Path) -> ResyncOutcome {
    let Some(recorded) = daemon_port::read(root) else {
        return ResyncOutcome::NotRunning;
    };
    let config = match Config::load(root) {
        Ok(config) => config,
        Err(error) => return ResyncOutcome::Failed(format!("could not be checked ({error})")),
    };
    match fetch_info(recorded.port, config.serve.place_id).await {
        Ok(info) => match ownership(
            &info,
            &project_name(root, &config),
            config.serve.place_id,
            Some(&recorded),
        ) {
            Ownership::Ours => ResyncOutcome::Resynced,
            Ownership::Foreign { project } => ResyncOutcome::ForeignDaemon { project },
            Ownership::Stale { project } => ResyncOutcome::Failed(format!(
                "answers as another daemon named `{project}`, not the one recorded in `.naht/daemon.port`"
            )),
            Ownership::Unverified => ResyncOutcome::Failed(
                "has no daemon id recorded in `.naht/daemon.port`, so it cannot be checked"
                    .to_owned(),
            ),
            Ownership::Missing => ResyncOutcome::NotRunning,
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
    let recorded = daemon_port::read(root);
    let port = recorded
        .as_ref()
        .map_or_else(|| config.port(), |recorded| recorded.port);
    let project = project_name(root, config);
    match fetch_info(port, config.serve.place_id).await {
        Ok(info) => accept_verdict(
            ownership(&info, &project, config.serve.place_id, recorded.as_ref()),
            port,
        ),
        Err(InfoFailure::Status(status)) => Err(CommandError::DaemonStatus { status }),
        Err(InfoFailure::Request(source)) => Err(CommandError::DaemonUnreachable {
            url: info_url(port),
            source,
        }),
        Err(InfoFailure::NotNaht) => Err(CommandError::NotNahtDaemon {
            url: info_url(port),
        }),
        Err(InfoFailure::OlderNaht) => Err(CommandError::OlderDaemon { port }),
    }
}

async fn fetch_info(port: u16, place_id: Option<u64>) -> Result<ServerInfo, InfoFailure> {
    let client = nudge_client().map_err(InfoFailure::Request)?;
    let mut request = client.get(info_url(port)).header(CLIENT_HEADER, CLI_CLIENT);
    if let Some(place_id) = place_id {
        request = request.header(PLACE_ID_HEADER, place_id.to_string());
    }
    let response = request.send().await.map_err(InfoFailure::Request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(InfoFailure::Status(status));
    }
    let bytes = read_capped(response).await?;
    decode_info(&bytes)
}

async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, InfoFailure> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(InfoFailure::Request)? {
        if bytes.len().saturating_add(chunk.len()) > INFO_LIMIT {
            return Err(InfoFailure::NotNaht);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct Answer {
    protocol_version: u32,
    server_version: String,
    project_name: String,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    token_required: bool,
    session_id: String,
    daemon_id: Option<String>,
    serve_place_id: Option<u64>,
    #[serde(default)]
    conflicted: Vec<String>,
    #[serde(default)]
    terrain_sync: bool,
    #[serde(default)]
    limits: Option<ReportedLimits>,
}

impl Answer {
    fn into_info(self) -> Option<ServerInfo> {
        Some(ServerInfo {
            protocol_version: self.protocol_version,
            server_version: self.server_version,
            project_name: self.project_name,
            project_id: self.project_id,
            token_required: self.token_required,
            session_id: self.session_id,
            daemon_id: self.daemon_id?,
            serve_place_id: self.serve_place_id,
            conflicted: self.conflicted,
            terrain_sync: self.terrain_sync,
            limits: self.limits,
        })
    }
}

fn decode_info(bytes: &[u8]) -> Result<ServerInfo, InfoFailure> {
    let answer: Answer = from_msgpack(bytes).map_err(|_decode| InfoFailure::NotNaht)?;
    answer.into_info().ok_or(InfoFailure::OlderNaht)
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

#[derive(Debug, PartialEq, Eq)]
enum Ownership {
    Ours,
    Foreign { project: String },
    Stale { project: String },
    Missing,
    Unverified,
}

fn ownership(
    info: &ServerInfo,
    project: &str,
    place_id: Option<u64>,
    recorded: Option<&Recorded>,
) -> Ownership {
    let same_place = place_id.is_none_or(|configured| info.serve_place_id == Some(configured));
    if info.project_name != project || !same_place {
        return Ownership::Foreign {
            project: info.project_name.clone(),
        };
    }
    match recorded {
        None => Ownership::Missing,
        Some(Recorded {
            daemon_id: None, ..
        }) => Ownership::Unverified,
        Some(Recorded {
            daemon_id: Some(id),
            ..
        }) if *id == info.daemon_id => Ownership::Ours,
        Some(Recorded { .. }) => Ownership::Stale {
            project: info.project_name.clone(),
        },
    }
}

fn accept_verdict(verdict: Ownership, port: u16) -> Result<(), CommandError> {
    match verdict {
        Ownership::Ours => Ok(()),
        Ownership::Foreign { project } => Err(CommandError::ForeignDaemon { port, project }),
        Ownership::Stale { project } => Err(CommandError::StaleDaemon { port, project }),
        Ownership::Missing => Err(CommandError::MissingPortFile { port }),
        Ownership::Unverified => Err(CommandError::UnverifiedDaemon { port }),
    }
}

#[cfg(test)]
mod tests;
