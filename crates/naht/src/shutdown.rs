use std::io;

use tokio::signal;
use tokio_util::sync::CancellationToken;

/// Resolve once the process is asked to stop: Ctrl-C everywhere, and SIGTERM on unix. Fails only if
/// the signal handlers cannot be installed.
pub async fn terminated() -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        signal::ctrl_c().await
    }
}

/// Cancel `token` when the process is asked to stop, or as soon as it is cancelled elsewhere,
/// whichever comes first.
pub async fn cancel_on_signal(token: CancellationToken) {
    tokio::select! {
        () = token.cancelled() => {}
        result = terminated() => {
            if let Err(error) = result {
                tracing::warn!(target: "naht::shutdown", %error, "signal listener failed");
            }
            token.cancel();
        }
    }
}
