use std::io;

use tokio::signal;
use tokio_util::sync::CancellationToken;

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
