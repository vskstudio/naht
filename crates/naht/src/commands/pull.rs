use super::error::CommandError;
use crate::config::Config;
use crate::daemon_port;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

const NUDGE_TIMEOUT: Duration = Duration::from_secs(2);

/// What a re-sync nudge found at the daemon recorded for a project.
#[derive(Debug, PartialEq, Eq)]
pub enum ResyncOutcome {
    /// No daemon is recorded for the project, or the recorded port refuses connections.
    NotRunning,
    /// The running daemon answered and re-synced.
    Resynced,
    /// A daemon is recorded but did not re-sync; the reason completes "the running daemon ...".
    Failed(String),
}

/// Asks the daemon recorded for the project at `root` to re-sync, within the nudge timeout.
pub async fn resync_running(root: &Path) -> ResyncOutcome {
    let Some(port) = daemon_port::read(root) else {
        return ResyncOutcome::NotRunning;
    };
    let url = format!("http://{}:{port}/info", Ipv4Addr::LOCALHOST);
    let client = match nudge_client() {
        Ok(client) => client,
        Err(error) => return ResyncOutcome::Failed(format!("could not be reached ({error})")),
    };
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => ResyncOutcome::Resynced,
        Ok(response) => ResyncOutcome::Failed(format!("answered {}", response.status())),
        Err(error) if error.is_timeout() => ResyncOutcome::Failed(format!(
            "did not answer within {} s",
            NUDGE_TIMEOUT.as_secs()
        )),
        Err(error) if error.is_connect() => ResyncOutcome::NotRunning,
        Err(error) => ResyncOutcome::Failed(format!("could not be reached ({error})")),
    }
}

fn nudge_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(NUDGE_TIMEOUT)
        .timeout(NUDGE_TIMEOUT)
        .build()
}

/// Ask a running daemon to re-sync now (Studio ↔ filesystem). Full on-demand Studio → filesystem
/// pull lands with the Stage 6 plugin; for now this nudges the daemon's reconnect re-diff.
///
/// # Errors
///
/// Returns [`CommandError`] when the daemon is unreachable within the nudge timeout or fails.
pub async fn pull(root: &Path, config: &Config) -> Result<(), CommandError> {
    let port = daemon_port::read(root).unwrap_or_else(|| config.port());
    let url = format!("http://{}:{port}/info", Ipv4Addr::LOCALHOST);
    let client = nudge_client().map_err(|source| CommandError::DaemonUnreachable {
        url: url.clone(),
        source,
    })?;
    let response =
        client
            .get(&url)
            .send()
            .await
            .map_err(|source| CommandError::DaemonUnreachable {
                url: url.clone(),
                source,
            })?;
    if !response.status().is_success() {
        return Err(CommandError::DaemonStatus {
            status: response.status(),
        });
    }
    Ok(())
}
