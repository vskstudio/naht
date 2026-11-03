//! The daemon's per-run sync session: the in-memory mirror of Studio, the filesystem-bound
//! reconcile, and the patch queue the long-poll drains.
//!
//! The filesystem is read fresh from disk on every reconcile (so a concurrent FS edit always merges
//! against the latest content); only the Studio side is mirrored in memory, because the daemon
//! cannot read the live `DataModel`.
//!
//! **Filesystem → Studio patches are ack-gated.** The base (and the mirror) advance for a
//! Studio-bound patch only once the plugin confirms it applied (`ack`). Until then the base is held
//! at the last agreed content, so a patch the plugin half-applied keeps re-diffing instead of being
//! treated as synced, and a Studio edit racing an unapplied patch merges against the real ancestor
//! rather than clobbering the filesystem. Filesystem-bound effects (writes the daemon itself
//! performed) advance the base immediately, since they are already durable.

mod inbound;
mod queue;
mod rescan;
mod state;

use naht_core::binary::{BlobInstance, BlobPatch};
use naht_core::protocol::SyncEvent;
use naht_core::reconciler::{Patch, ReconcileError, TextInstance};
use naht_core::state::{StateError, StateStore};
use naht_core::vfs::{DiskVfs, RootedVfs, VfsError};
use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Why a sync session could not read its state or reconcile the filesystem.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The persisted base or asset cache failed.
    #[error(transparent)]
    State(#[from] StateError),
    /// A reconcile step failed, as the filesystem or the store reported it.
    #[error(transparent)]
    Reconcile(#[from] ReconcileError),
    /// A path from the protocol was refused by the rooted filesystem.
    #[error(transparent)]
    Vfs(#[from] VfsError),
}

const PROJECT_ROOT: &str = "";

const MASS_DELETE_WARN: usize = 25;

const ACTIVITY_CAP: usize = 256;

/// One running sync session.
#[derive(Debug)]
pub struct Session {
    vfs: RootedVfs<DiskVfs>,
    store: StateStore,
    /// The last-known Studio side, keyed by project-relative path. Advances on ack, never on send.
    studio: BTreeMap<String, TextInstance>,
    /// Queued filesystem → Studio patches, keyed by path so a re-diff replaces rather than appends.
    /// Each carries a monotonically increasing sequence so the long-poll can resume from a cursor.
    queue: BTreeMap<String, (u64, Patch)>,
    /// Studio-bound patches sent but not yet acked; their base stays at the agreed content.
    unacked: BTreeMap<String, Patch>,
    next_seq: u64,
    /// The last-known Studio side of binary blobs (terrain), keyed by path. Blobs are last-writer-wins
    /// (architecture §9), so, unlike the ack-gated text mirror, this advances as soon as a patch is
    /// emitted rather than on a plugin ack.
    studio_blobs: BTreeMap<String, BlobInstance>,
    /// Queued filesystem → Studio blob patches, on their own sequence/cursor (a separate `/blobs`
    /// channel), keyed by path so a re-diff replaces rather than appends.
    blob_queue: BTreeMap<String, (u64, BlobPatch)>,
    blob_next_seq: u64,
    /// Recent completed sync operations, newest at the back, capped at `ACTIVITY_CAP`.
    activity: VecDeque<SyncEvent>,
    /// The seq to stamp on the next recorded event; monotonic within this session.
    activity_next_seq: u64,
    project_name: String,
    id: String,
    serve_place_id: Option<u64>,
    terrain_sync: bool,
}

fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{nanos:x}")
}
