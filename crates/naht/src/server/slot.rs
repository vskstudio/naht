use std::future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time;

const ANONYMOUS: u8 = 0;
const AUTHENTICATED: u8 = 1;
const EXPIRED: u8 = 2;

#[derive(Debug)]
pub(super) struct ConnectionSlot {
    state: AtomicU8,
    permit: Mutex<Option<OwnedSemaphorePermit>>,
    authenticated: Arc<Semaphore>,
}

impl ConnectionSlot {
    pub(super) fn new(anonymous: OwnedSemaphorePermit, authenticated: Arc<Semaphore>) -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(ANONYMOUS),
            permit: Mutex::new(Some(anonymous)),
            authenticated,
        })
    }

    pub(super) async fn promote(&self) -> bool {
        if self.state.load(Ordering::Acquire) == AUTHENTICATED {
            return true;
        }
        let Ok(seat) = Arc::clone(&self.authenticated).acquire_owned().await else {
            return false;
        };
        if self
            .state
            .compare_exchange(
                ANONYMOUS,
                AUTHENTICATED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        self.permit
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(seat);
        true
    }

    pub(super) fn release(&self) {
        *self.permit.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub(super) async fn expired(&self, deadline: Duration) {
        time::sleep(deadline).await;
        if self
            .state
            .compare_exchange(ANONYMOUS, EXPIRED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
        future::pending::<()>().await;
    }
}
