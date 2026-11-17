use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::Connection;

use super::StateStore;

struct Holder {
    ready: Receiver<()>,
    release: Sender<()>,
    handle: JoinHandle<()>,
}

fn hold_write_transaction(path: &Path, hold_for: Duration) -> Holder {
    let (ready_tx, ready) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let path = path.to_path_buf();
    let handle = thread::spawn(move || {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        ready_tx.send(()).unwrap();
        let _ = released.recv_timeout(hold_for);
        conn.execute_batch("COMMIT").unwrap();
    });
    Holder {
        ready,
        release,
        handle,
    }
}

#[test]
fn a_write_waits_for_a_brief_write_transaction_held_by_another_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let store = StateStore::open(&path).unwrap();
    let holder = hold_write_transaction(&path, Duration::from_millis(300));
    holder.ready.recv().unwrap();

    let written = store.set_project_id("held");
    holder.handle.join().unwrap();
    written.unwrap();
}

#[test]
fn opening_a_current_store_does_not_wait_for_a_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    drop(StateStore::open(&path).unwrap());
    let holder = hold_write_transaction(&path, Duration::from_secs(30));
    holder.ready.recv().unwrap();

    let reopened = StateStore::open(&path);
    holder.release.send(()).unwrap();
    holder.handle.join().unwrap();
    reopened.unwrap();
}
