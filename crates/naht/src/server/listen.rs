use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time;
use tokio_util::sync::CancellationToken;

use super::slot::ConnectionSlot;

const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

pub(super) const ANONYMOUS_CONNECTIONS: usize = 16;

pub(super) const ANONYMOUS_DEADLINE: Duration = Duration::from_secs(2);

pub(super) async fn accept_until_cancelled(
    listener: TcpListener,
    router: Router,
    shutdown: CancellationToken,
    connections: usize,
) -> io::Result<()> {
    let mut builder = Builder::new(TokioExecutor::new()).http1_only();
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    let graceful = GracefulShutdown::new();
    let anonymous = Arc::new(Semaphore::new(ANONYMOUS_CONNECTIONS));
    let authenticated = Arc::new(Semaphore::new(connections));

    loop {
        let seat = tokio::select! {
            () = shutdown.cancelled() => break,
            seat = Arc::clone(&anonymous).acquire_owned() => match seat {
                Ok(seat) => seat,
                Err(_) => break,
            },
        };
        tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => serve_accepted(
                    stream,
                    seat,
                    &router,
                    &builder,
                    &graceful,
                    Arc::clone(&authenticated),
                ),
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

fn serve_accepted(
    stream: TcpStream,
    seat: OwnedSemaphorePermit,
    router: &Router,
    builder: &Builder<TokioExecutor>,
    graceful: &GracefulShutdown,
    authenticated: Arc<Semaphore>,
) {
    let slot = ConnectionSlot::new(seat, authenticated);
    let service = TowerToHyperService::new(router.clone().layer(Extension(Arc::clone(&slot))));
    let connection = builder.serve_connection(TokioIo::new(stream), service);
    let watched = graceful.watch(connection.into_owned());
    tokio::spawn(async move {
        tokio::select! {
            result = watched => {
                if let Err(error) = result {
                    tracing::debug!(target: "naht::server", %error, "connection closed");
                }
            }
            () = slot.expired(ANONYMOUS_DEADLINE) => {}
        }
    });
}

#[cfg(test)]
mod tests;
