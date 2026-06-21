//! The daemon's per-run sync session: the in-memory mirror of Studio, the filesystem-bound
//! reconcile, and the patch queue the long-poll drains.
//!
//! The filesystem is read fresh from disk on every reconcile (so a concurrent FS edit always merges
//! against the latest content); only the Studio side is mirrored in memory, because the daemon
//! cannot read the live DataModel. After each reconcile the mirror is advanced to the converged
//! state the patches imply, so repeated reconciles settle instead of ping-ponging.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use naht_core::protocol::{Change, ServerInfo, PROTOCOL_VERSION};
use naht_core::reconciler::{self, Direction, Patch, PatchKind, TextInstance};
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};

/// The directory the project's source lives in, relative to the rooted VFS — its own root.
const PROJECT_ROOT: &str = "";

/// One running sync session.
pub struct Session {
    vfs: RootedVfs<DiskVfs>,
    store: StateStore,
    /// The last-known Studio side, keyed by project-relative path.
    studio: BTreeMap<String, TextInstance>,
    /// Queued filesystem → Studio patches, each tagged with a monotonically increasing sequence so
    /// the long-poll can resume from a cursor.
    queue: Vec<(u64, Patch)>,
    next_seq: u64,
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
        let root = root.into();
        Self {
            vfs: RootedVfs::new(root, DiskVfs::new()),
            store,
            studio: BTreeMap::new(),
            queue: Vec::new(),
            next_seq: 0,
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
        let fs = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches);
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
        let fs = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches);
        Ok(())
    }

    /// Whether any queued patch has a sequence at or beyond `cursor`.
    pub fn has_patches_after(&self, cursor: u64) -> bool {
        self.queue.iter().any(|(seq, _)| *seq >= cursor)
    }

    /// Every queued patch at or beyond `cursor`, with the cursor to send next time.
    pub fn take_patches(&self, cursor: u64) -> (u64, Vec<Patch>) {
        let patches = self
            .queue
            .iter()
            .filter(|(seq, _)| *seq >= cursor)
            .map(|(_, patch)| patch.clone())
            .collect();
        (self.next_seq, patches)
    }

    /// Record the result of a reconcile: advance the Studio mirror to the converged state and queue
    /// the filesystem → Studio patches for the long-poll.
    fn absorb(&mut self, patches: Vec<Patch>) {
        for patch in patches {
            self.mirror(&patch);
            if patch.direction == Direction::ToStudio {
                self.queue.push((self.next_seq, patch));
                self.next_seq += 1;
            }
        }
    }

    /// Advance the Studio mirror to the state a patch leaves behind, so the next reconcile sees no
    /// spurious difference. A conflict freezes the path (skipped until resolved), so its mirror value
    /// is inert; we still mirror the marked content for consistency.
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
}

/// A best-effort unique id for this run, so a restart is visible to the plugin as a changed id.
fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
