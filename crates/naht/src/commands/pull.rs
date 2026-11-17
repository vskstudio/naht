use super::error::CommandError;
use super::files::canonical;
use crate::config::Config;
use crate::daemon_port::{self, Recorded};
use naht_core::protocol::{ServerInfo, from_msgpack};
use reqwest::StatusCode;
use serde::Deserialize;
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
    match fetch_info(recorded.port).await {
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
    match fetch_info(port).await {
        Ok(info) => match ownership(&info, &project, config.serve.place_id, recorded.as_ref()) {
            Ownership::Ours => Ok(()),
            Ownership::Foreign { project } => Err(CommandError::ForeignDaemon { port, project }),
            Ownership::Stale { project } => Err(CommandError::StaleDaemon { port, project }),
            Ownership::Missing => Err(CommandError::MissingPortFile { port }),
            Ownership::Unverified => Err(CommandError::UnverifiedDaemon { port }),
        },
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
    decode_info(&bytes)
}

#[derive(Deserialize)]
struct Answer {
    protocol_version: u32,
    server_version: String,
    project_name: String,
    session_id: String,
    daemon_id: Option<String>,
    serve_place_id: Option<u64>,
    #[serde(default)]
    conflicted: Vec<String>,
}

impl Answer {
    fn into_info(self) -> Option<ServerInfo> {
        Some(ServerInfo {
            protocol_version: self.protocol_version,
            server_version: self.server_version,
            project_name: self.project_name,
            session_id: self.session_id,
            daemon_id: self.daemon_id?,
            serve_place_id: self.serve_place_id,
            conflicted: self.conflicted,
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

#[cfg(test)]
mod tests {
    use super::{InfoFailure, Ownership, decode_info, ownership};
    use crate::daemon_port::Recorded;
    use naht_core::protocol::{PROTOCOL_VERSION, ServerInfo, to_msgpack};

    fn info(project: &str, place: Option<u64>, daemon_id: &str) -> ServerInfo {
        ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: "test".to_owned(),
            project_name: project.to_owned(),
            session_id: "test".to_owned(),
            daemon_id: daemon_id.to_owned(),
            serve_place_id: place,
            conflicted: Vec::new(),
        }
    }

    fn recorded(daemon_id: Option<&str>) -> Recorded {
        Recorded {
            port: 34872,
            daemon_id: daemon_id.map(str::to_owned),
        }
    }

    #[derive(serde::Serialize)]
    struct OlderInfo {
        protocol_version: u32,
        server_version: String,
        project_name: String,
        session_id: String,
        serve_place_id: Option<u64>,
    }

    #[test]
    fn a_daemon_recording_this_id_is_ours() {
        let verdict = ownership(
            &info("demo", None, "d-1"),
            "demo",
            None,
            Some(&recorded(Some("d-1"))),
        );
        assert_eq!(verdict, Ownership::Ours);
    }

    #[test]
    fn a_same_named_daemon_with_another_id_is_stale() {
        let verdict = ownership(
            &info("game", None, "d-2"),
            "game",
            None,
            Some(&recorded(Some("d-1"))),
        );
        assert_eq!(
            verdict,
            Ownership::Stale {
                project: "game".to_owned()
            }
        );
    }

    #[test]
    fn a_recorded_port_without_a_daemon_id_is_unverified() {
        let verdict = ownership(
            &info("demo", None, "d-1"),
            "demo",
            None,
            Some(&recorded(None)),
        );
        assert_eq!(verdict, Ownership::Unverified);
    }

    #[test]
    fn a_missing_port_file_is_missing_not_unverified() {
        let verdict = ownership(&info("demo", None, "d-1"), "demo", None, None);
        assert_eq!(verdict, Ownership::Missing);
    }

    #[test]
    fn a_different_project_name_is_foreign() {
        let verdict = ownership(
            &info("other", None, "d-1"),
            "demo",
            None,
            Some(&recorded(Some("d-1"))),
        );
        assert!(matches!(verdict, Ownership::Foreign { .. }));
    }

    #[test]
    fn a_configured_place_must_match_the_daemon_place() {
        let matching = ownership(
            &info("demo", Some(1), "d-1"),
            "demo",
            Some(1),
            Some(&recorded(Some("d-1"))),
        );
        let other = ownership(
            &info("demo", Some(2), "d-1"),
            "demo",
            Some(1),
            Some(&recorded(Some("d-1"))),
        );
        assert_eq!(matching, Ownership::Ours);
        assert!(matches!(other, Ownership::Foreign { .. }));
    }

    #[test]
    fn an_unconfigured_place_accepts_any_daemon_place() {
        let verdict = ownership(
            &info("demo", Some(2), "d-1"),
            "demo",
            None,
            Some(&recorded(Some("d-1"))),
        );
        assert_eq!(verdict, Ownership::Ours);
    }

    #[test]
    fn a_current_info_payload_decodes() {
        let bytes = to_msgpack(&info("demo", None, "d-1")).unwrap();
        assert_eq!(decode_info(&bytes).ok(), Some(info("demo", None, "d-1")));
    }

    #[test]
    fn an_info_payload_carries_the_frozen_paths_it_reports() {
        let frozen = ServerInfo {
            conflicted: vec!["Frozen.luau".to_owned()],
            ..info("demo", None, "d-1")
        };
        let bytes = to_msgpack(&frozen).unwrap();
        assert_eq!(decode_info(&bytes).ok(), Some(frozen));
    }

    #[test]
    fn an_info_payload_without_a_daemon_id_is_from_an_older_naht() {
        let bytes = to_msgpack(&OlderInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: "0.1.0".to_owned(),
            project_name: "demo".to_owned(),
            session_id: "s-1".to_owned(),
            serve_place_id: None,
        })
        .unwrap();
        assert!(matches!(decode_info(&bytes), Err(InfoFailure::OlderNaht)));
    }

    #[test]
    fn an_answer_that_is_not_naht_info_is_not_naht() {
        let bytes = to_msgpack(&"not a daemon").unwrap();
        assert!(matches!(decode_info(&bytes), Err(InfoFailure::NotNaht)));
    }
}
