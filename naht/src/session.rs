//! The daemon's per-run sync session: the in-memory mirror of Studio, the filesystem-bound
//! reconcile, and the patch queue the long-poll drains.
//!
//! The filesystem is read fresh from disk on every reconcile (so a concurrent FS edit always merges
//! against the latest content); only the Studio side is mirrored in memory, because the daemon
//! cannot read the live DataModel.
//!
//! **Filesystem → Studio patches are ack-gated.** The base (and the mirror) advance for a
//! Studio-bound patch only once the plugin confirms it applied (`ack`). Until then the base is held
//! at the last agreed content, so a patch the plugin half-applied keeps re-diffing instead of being
//! treated as synced — and a Studio edit racing an unapplied patch merges against the real ancestor
//! rather than clobbering the filesystem. Filesystem-bound effects (writes the daemon itself
//! performed) advance the base immediately, since they are already durable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use naht_core::binary::{self, BlobInstance, BlobPatch};
use naht_core::protocol::{Change, ServerInfo, PROTOCOL_VERSION};
use naht_core::reconciler::{self, Direction, Patch, PatchKind, TextInstance};
use naht_core::state::{InstanceRecord, StateStore};
use naht_core::vfs::{DiskVfs, RootedVfs};

/// The directory the project's source lives in, relative to the rooted VFS — its own root.
const PROJECT_ROOT: &str = "";

/// A reconcile that deletes at least this many paths at once is flagged loudly. The daemon can't
/// interactively confirm, but an unusually large deletion (e.g. a Studio folder cleared by mistake)
/// should never pass silently.
const MASS_DELETE_WARN: usize = 25;

/// One running sync session.
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
    /// (architecture §9), so — unlike the ack-gated text mirror — this advances as soon as a patch is
    /// emitted rather than on a plugin ack.
    studio_blobs: BTreeMap<String, BlobInstance>,
    /// Queued filesystem → Studio blob patches, on their own sequence/cursor (a separate `/blobs`
    /// channel), keyed by path so a re-diff replaces rather than appends.
    blob_queue: BTreeMap<String, (u64, BlobPatch)>,
    blob_next_seq: u64,
    project_name: String,
    session_id: String,
    serve_place_id: Option<u64>,
}

impl Session {
    /// Open a session rooted at `root`, owning `store`. `serve_place_id` guards which Studio place
    /// may connect (`None` leaves it unguarded).
    pub fn new(
        root: impl Into<PathBuf>,
        store: StateStore,
        project_name: impl Into<String>,
        serve_place_id: Option<u64>,
    ) -> Self {
        Self {
            vfs: RootedVfs::new(root.into(), DiskVfs::new()),
            store,
            studio: BTreeMap::new(),
            queue: BTreeMap::new(),
            unacked: BTreeMap::new(),
            next_seq: 0,
            studio_blobs: BTreeMap::new(),
            blob_queue: BTreeMap::new(),
            blob_next_seq: 0,
            project_name: project_name.into(),
            session_id: new_session_id(),
            serve_place_id,
        }
    }

    /// The handshake payload for `GET /info`.
    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: naht_core::version().to_string(),
            project_name: self.project_name.clone(),
            session_id: self.session_id.clone(),
            serve_place_id: self.serve_place_id,
        }
    }

    /// The place id this session is bound to, if any.
    pub fn serve_place_id(&self) -> Option<u64> {
        self.serve_place_id
    }

    /// Re-read the filesystem and reconcile it against the Studio mirror and persisted base. Used on
    /// a filesystem event and on reconnect (`GET /info`) — the latter is the re-diff that replaces a
    /// blind re-push.
    pub fn rescan(&mut self) -> Result<()> {
        let pre_base = self.base_snapshot()?;
        let fs = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches, &pre_base);
        self.reconcile_blobs()?;
        Ok(())
    }

    /// Apply a batch of Studio-side edits to the mirror, then reconcile against the current
    /// filesystem. Filesystem writes (and conflict markers) are performed by the reconciler.
    pub fn apply_changes(&mut self, changes: Vec<Change>) -> Result<()> {
        for change in changes {
            match change {
                Change::Upsert {
                    path,
                    class,
                    content,
                } => {
                    self.studio
                        .insert(path.clone(), TextInstance::new(path, class, content));
                }
                Change::Delete { path } => {
                    self.studio.remove(&path);
                }
            }
        }
        let pre_base = self.base_snapshot()?;
        let fs = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches, &pre_base);
        Ok(())
    }

    /// Confirm that the plugin applied the patches for `paths`: advance the base and the mirror for
    /// each, so they stop re-diffing. Paths not in `paths` stay pending and re-emit next cycle.
    pub fn ack(&mut self, paths: &[String]) -> Result<()> {
        for path in paths {
            let Some(patch) = self.unacked.remove(path) else {
                continue;
            };
            match &patch.content {
                Some(content) => {
                    reconciler::set_base(&self.store, &patch.path, &patch.class, content)?;
                    self.studio.insert(
                        patch.path.clone(),
                        TextInstance::new(patch.path.clone(), patch.class.clone(), content.clone()),
                    );
                }
                None => {
                    self.store.remove(&patch.path)?;
                    self.studio.remove(&patch.path);
                }
            }
        }
        Ok(())
    }

    /// Apply a batch of Studio-side binary blobs (terrain) to the blob mirror, then reconcile against
    /// the current filesystem. Blob deletions aren't modeled on the wire — terrain isn't removed in
    /// practice — so a batch carries only the blobs Studio currently holds.
    pub fn apply_blob_changes(&mut self, changes: Vec<BlobInstance>) -> Result<()> {
        for change in changes {
            self.studio_blobs.insert(change.path.clone(), change);
        }
        self.reconcile_blobs()
    }

    /// Re-read the filesystem's binary blobs and reconcile them against the Studio blob mirror and the
    /// persisted (hash-only) base. Filesystem writes are applied by the reconciler; Studio-bound blob
    /// patches are queued for the `/blobs` long-poll.
    fn reconcile_blobs(&mut self) -> Result<()> {
        let fs = binary::scan_blobs(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = binary::reconcile_blobs(&mut self.vfs, &self.store, &fs, &self.studio_blobs)?;
        self.absorb_blobs(patches);
        Ok(())
    }

    /// Record the result of a blob reconcile. Filesystem-bound effects were already applied to disk by
    /// the reconciler. A Studio-bound patch is queued and the blob mirror advanced immediately
    /// (last-writer-wins, not ack-gated). A conflict froze the path; nothing is sent.
    fn absorb_blobs(&mut self, patches: Vec<BlobPatch>) {
        for patch in patches {
            if patch.kind == PatchKind::Conflict {
                tracing::warn!(target: "naht::sync", path = %patch.path, "terrain blob conflict frozen");
                continue;
            }
            match patch.direction {
                // Already written to disk; the mirror already holds the Studio-side value that drove
                // it, so the next reconcile sees the sides agree.
                Direction::ToFs => {}
                Direction::ToStudio => {
                    match (&patch.kind, &patch.content) {
                        (PatchKind::Delete, _) => {
                            self.studio_blobs.remove(&patch.path);
                        }
                        (_, Some(content)) => {
                            self.studio_blobs.insert(
                                patch.path.clone(),
                                BlobInstance::new(
                                    patch.path.clone(),
                                    patch.class.clone(),
                                    content.clone(),
                                ),
                            );
                        }
                        _ => {}
                    }
                    self.enqueue_blob(patch);
                }
            }
        }
    }

    fn enqueue_blob(&mut self, patch: BlobPatch) {
        tracing::info!(
            target: "naht::sync",
            path = %patch.path,
            kind = ?patch.kind,
            "blob patch emitted"
        );
        self.blob_queue
            .insert(patch.path.clone(), (self.blob_next_seq, patch));
        self.blob_next_seq += 1;
    }

    /// Whether any queued blob patch has a sequence at or beyond `cursor`.
    pub fn has_blobs_after(&self, cursor: u64) -> bool {
        self.blob_queue.values().any(|(seq, _)| *seq >= cursor)
    }

    /// Every queued blob patch at or beyond `cursor`, in sequence order, with the cursor to send next.
    pub fn take_blob_patches(&self, cursor: u64) -> (u64, Vec<BlobPatch>) {
        let mut entries: Vec<&(u64, BlobPatch)> = self
            .blob_queue
            .values()
            .filter(|(seq, _)| *seq >= cursor)
            .collect();
        entries.sort_by_key(|(seq, _)| *seq);
        let patches = entries
            .into_iter()
            .map(|(_, patch)| patch.clone())
            .collect();
        (self.blob_next_seq, patches)
    }

    /// Whether any queued patch has a sequence at or beyond `cursor`.
    pub fn has_patches_after(&self, cursor: u64) -> bool {
        self.queue.values().any(|(seq, _)| *seq >= cursor)
    }

    /// Every queued patch at or beyond `cursor`, in sequence order, with the cursor to send next.
    pub fn take_patches(&self, cursor: u64) -> (u64, Vec<Patch>) {
        let mut entries: Vec<&(u64, Patch)> = self
            .queue
            .values()
            .filter(|(seq, _)| *seq >= cursor)
            .collect();
        entries.sort_by_key(|(seq, _)| *seq);
        let patches = entries
            .into_iter()
            .map(|(_, patch)| patch.clone())
            .collect();
        (self.next_seq, patches)
    }

    /// Record the result of a reconcile. Filesystem-bound effects already advanced the base; for a
    /// Studio-bound patch the base is held (ack-gated) until the plugin confirms it.
    fn absorb(&mut self, patches: Vec<Patch>, pre_base: &BTreeMap<String, InstanceRecord>) {
        let fs_bound: BTreeSet<String> = patches
            .iter()
            .filter(|patch| patch.direction == Direction::ToFs)
            .map(|patch| patch.path.clone())
            .collect();

        let deletions = patches
            .iter()
            .filter(|patch| patch.kind == PatchKind::Delete)
            .count();
        if deletions >= MASS_DELETE_WARN {
            tracing::warn!(
                target: "naht::sync",
                count = deletions,
                "large deletion reconciled — verify this was intended"
            );
        }

        for patch in patches {
            if patch.kind == PatchKind::Conflict {
                tracing::warn!(target: "naht::sync", path = %patch.path, "conflict frozen");
            }
            match patch.direction {
                Direction::ToFs => {
                    self.mirror(&patch);
                    tracing::debug!(
                        target: "naht::sync",
                        path = %patch.path,
                        kind = ?patch.kind,
                        "patch applied to filesystem"
                    );
                }
                // A merge also wrote the filesystem, so its base is correct and the mirror must
                // advance to avoid a clobber on the next reconcile; treat it like a durable patch.
                // Drop any earlier hold for this path so a later ack can't revert the base to stale
                // pre-merge content.
                Direction::ToStudio if fs_bound.contains(&patch.path) => {
                    self.unacked.remove(&patch.path);
                    self.mirror(&patch);
                    self.enqueue(patch);
                }
                // A pure Studio-bound patch is ack-gated: hold the base at the agreed content and do
                // not touch the mirror, so it re-diffs until the plugin confirms it.
                Direction::ToStudio => {
                    self.revert_base(&patch.path, pre_base);
                    self.unacked.insert(patch.path.clone(), patch.clone());
                    self.enqueue(patch);
                }
            }
        }
    }

    fn enqueue(&mut self, patch: Patch) {
        tracing::info!(
            target: "naht::sync",
            path = %patch.path,
            kind = ?patch.kind,
            "patch emitted"
        );
        self.queue
            .insert(patch.path.clone(), (self.next_seq, patch));
        self.next_seq += 1;
    }

    /// Undo the base advance the reconciler made for a held Studio-bound patch, restoring the agreed
    /// ancestor (or removing it if the path was new).
    fn revert_base(&mut self, path: &str, pre_base: &BTreeMap<String, InstanceRecord>) {
        let result = match pre_base.get(path) {
            Some(record) => self.store.upsert(record),
            None => self.store.remove(path),
        };
        if let Err(error) = result {
            tracing::warn!(target: "naht::sync", %path, %error, "failed to hold base");
        }
    }

    /// Advance the Studio mirror to the state a durable patch leaves behind.
    fn mirror(&mut self, patch: &Patch) {
        match patch.kind {
            PatchKind::Delete => {
                self.studio.remove(&patch.path);
            }
            PatchKind::Create | PatchKind::Update | PatchKind::Conflict => {
                if let Some(content) = &patch.content {
                    self.studio.insert(
                        patch.path.clone(),
                        TextInstance::new(patch.path.clone(), patch.class.clone(), content.clone()),
                    );
                }
            }
        }
    }

    fn base_snapshot(&self) -> Result<BTreeMap<String, InstanceRecord>> {
        Ok(self
            .store
            .all()?
            .into_iter()
            .map(|record| (record.path.clone(), record))
            .collect())
    }
}

/// A best-effort unique id for this run, so a restart is visible to the plugin as a changed id.
fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
