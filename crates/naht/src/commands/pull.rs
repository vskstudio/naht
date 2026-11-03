use super::error::CommandError;
use crate::config::Config;
use std::net::Ipv4Addr;

/// Ask a running daemon to re-sync now (Studio ↔ filesystem). Full on-demand Studio → filesystem
/// pull lands with the Stage 6 plugin; for now this nudges the daemon's reconnect re-diff.
pub async fn pull(config: &Config) -> Result<(), CommandError> {
    let url = format!("http://{}:{}/info", Ipv4Addr::LOCALHOST, config.port());
    let response = reqwest::Client::new()
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
