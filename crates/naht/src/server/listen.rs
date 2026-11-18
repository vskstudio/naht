use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time;
use tokio_util::sync::CancellationToken;

const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

const MAX_CONNECTIONS: usize = 64;

pub(super) async fn accept_until_cancelled(
    listener: TcpListener,
    router: Router,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let mut builder = Builder::new(TokioExecutor::new()).http1_only();
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    let graceful = GracefulShutdown::new();
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));

    loop {
        let slot = tokio::select! {
            () = shutdown.cancelled() => break,
            slot = Arc::clone(&slots).acquire_owned() => match slot {
                Ok(slot) => slot,
                Err(_) => break,
            },
        };
        tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let connection = builder.serve_connection(
                        TokioIo::new(stream),
                        TowerToHyperService::new(router.clone()),
                    );
                    let watched = graceful.watch(connection.into_owned());
                    tokio::spawn(async move {
                        let _slot = slot;
                        if let Err(error) = watched.await {
                            tracing::debug!(target: "naht::server", %error, "connection closed");
                        }
                    });
                }
                Err(error) => {
                    tracing::warn!(target: "naht::server", %error, "accept failed");
                    time::sleep(ACCEPT_BACKOFF).await;
                }
            },
        }
    }

    graceful.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests;
