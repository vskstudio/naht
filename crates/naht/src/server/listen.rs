use std::io;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Extension;
use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::{GracefulShutdown, Watcher};
use hyper_util::service::TowerToHyperService;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time;
use tokio_util::sync::CancellationToken;

use super::slot::ConnectionSlot;

const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

const SHED_REPORT_INTERVAL: Duration = Duration::from_secs(10);

pub(super) const ANONYMOUS_CONNECTIONS: usize = 16;

pub(super) const ANONYMOUS_QUEUE: usize = 256;

pub(super) const ANONYMOUS_QUEUE_WAIT: Duration = Duration::from_secs(20);

pub(super) const ANONYMOUS_DEADLINE: Duration = Duration::from_secs(2);

struct Serving {
    router: Router,
    builder: Builder<TokioExecutor>,
    anonymous: Arc<Semaphore>,
    authenticated: Arc<Semaphore>,
}

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
    let serving = Arc::new(Serving {
        router,
        builder,
        anonymous: Arc::new(Semaphore::new(ANONYMOUS_CONNECTIONS)),
        authenticated: Arc::new(Semaphore::new(connections)),
    });
    let queue = Arc::new(Semaphore::new(ANONYMOUS_QUEUE));
    let graceful = GracefulShutdown::new();
    let mut sheds = ShedLog::default();
    let mut shed_flush = time::interval(SHED_REPORT_INTERVAL);

    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = shed_flush.tick() => report_sheds(sheds.flush(Instant::now())),
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    if !admit(stream, &queue, &serving, graceful.watcher()) {
                        report_sheds(sheds.record(Instant::now()));
                    }
                }
                Err(error) => {
                    tracing::warn!(target: "naht::server", %error, "accept failed");
                    time::sleep(ACCEPT_BACKOFF).await;
                }
            },
        }
    }

    report_sheds(sheds.flush(Instant::now()));
    graceful.shutdown().await;
    Ok(())
}

#[derive(Default)]
struct ShedLog {
    pending: u64,
    last_report: Option<Instant>,
}

impl ShedLog {
    fn record(&mut self, now: Instant) -> Option<u64> {
        self.pending = self.pending.saturating_add(1);
        if self.is_due(now) {
            self.take(now)
        } else {
            None
        }
    }

    fn flush(&mut self, now: Instant) -> Option<u64> {
        self.take(now)
    }

    fn is_due(&self, now: Instant) -> bool {
        self.last_report
            .is_none_or(|last| now.duration_since(last) >= SHED_REPORT_INTERVAL)
    }

    fn take(&mut self, now: Instant) -> Option<u64> {
        if self.pending == 0 {
            return None;
        }
        self.last_report = Some(now);
        Some(mem::take(&mut self.pending))
    }
}

fn report_sheds(count: Option<u64>) {
    if let Some(shed) = count {
        tracing::warn!(
            target: "naht::server",
            shed,
            "anonymous queue full, connections closed at accept"
        );
    }
}

fn admit(
    stream: TcpStream,
    queue: &Arc<Semaphore>,
    serving: &Arc<Serving>,
    watcher: Watcher,
) -> bool {
    let Ok(place) = Arc::clone(queue).try_acquire_owned() else {
        return false;
    };
    tokio::spawn(serve_accepted(stream, place, watcher, Arc::clone(serving)));
    true
}

async fn serve_accepted(
    stream: TcpStream,
    place: OwnedSemaphorePermit,
    watcher: Watcher,
    serving: Arc<Serving>,
) {
    let Ok(Ok(seat)) = time::timeout(
        ANONYMOUS_QUEUE_WAIT,
        Arc::clone(&serving.anonymous).acquire_owned(),
    )
    .await
    else {
        tracing::debug!(target: "naht::server", "no anonymous seat freed in time, connection closed");
        return;
    };
    drop(place);

    let slot = ConnectionSlot::new(seat, Arc::clone(&serving.authenticated));
    let service =
        TowerToHyperService::new(serving.router.clone().layer(Extension(Arc::clone(&slot))));
    let connection = serving
        .builder
        .serve_connection(TokioIo::new(stream), service);
    let finished = watcher.watch(connection.into_owned());
    tokio::select! {
        result = finished => {
            if let Err(error) = result {
                tracing::debug!(target: "naht::server", %error, "connection closed");
            }
        }
        () = slot.expired(ANONYMOUS_DEADLINE) => {}
    }
    slot.release();
}

#[cfg(test)]
mod tests;
