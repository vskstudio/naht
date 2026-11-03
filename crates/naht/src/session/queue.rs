use super::Session;
use super::{ACTIVITY_CAP, SessionError};
use naht_core::binary::BlobPatch;
use naht_core::protocol::{EventKind, Outcome, SyncActivity, SyncEvent};
use naht_core::reconciler::{self, Direction, Patch, TextInstance};
use std::time::{SystemTime, UNIX_EPOCH};

impl Session {
    /// The activity snapshot for `GET /activity`: the live pending count (unacked text patches) plus
    /// every retained event past `since` (strictly newer), oldest-first. Seqs are 1-based, so the
    /// default `since = 0` returns everything retained while a caller's cursor never re-delivers the
    /// event it last saw.
    pub fn activity(&self, since: u64) -> SyncActivity {
        let buffer_start_seq = self
            .activity
            .front()
            .map(|event| event.seq)
            .unwrap_or(self.activity_next_seq);
        let start = self.activity.partition_point(|event| event.seq <= since);
        let events = self.activity.range(start..).cloned().collect();
        SyncActivity {
            pending: self.unacked.len() as u32,
            buffer_start_seq,
            events,
        }
    }

    /// Confirm that the plugin applied the patches for `paths`: advance the base and the mirror for
    /// each, so they stop re-diffing. Paths not in `paths` stay pending and re-emit next cycle.
    pub fn ack(&mut self, paths: &[String]) -> Result<(), SessionError> {
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
            self.record_event(
                patch.path.clone(),
                Direction::ToStudio,
                EventKind::Text,
                Outcome::Ok,
            );
        }
        Ok(())
    }

    pub(super) fn enqueue_blob(&mut self, patch: BlobPatch) {
        tracing::info!(
            target: "naht::sync",
            path = %patch.path,
            kind = ?patch.kind,
            "blob patch emitted"
        );
        self.record_event(
            patch.path.clone(),
            Direction::ToStudio,
            EventKind::Blob,
            Outcome::Ok,
        );
        self.blob_queue
            .insert(patch.path.clone(), (self.blob_next_seq, patch));
        self.blob_next_seq = self.blob_next_seq.saturating_add(1);
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

    pub(super) fn enqueue(&mut self, patch: Patch) {
        tracing::info!(
            target: "naht::sync",
            path = %patch.path,
            kind = ?patch.kind,
            "patch emitted"
        );
        self.queue
            .insert(patch.path.clone(), (self.next_seq, patch));
        self.next_seq = self.next_seq.saturating_add(1);
    }

    pub(super) fn record_event(
        &mut self,
        path: String,
        direction: Direction,
        kind: EventKind,
        outcome: Outcome,
    ) {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        self.activity.push_back(SyncEvent {
            seq: self.activity_next_seq,
            unix_ms,
            path,
            direction,
            kind,
            outcome,
        });
        self.activity_next_seq = self.activity_next_seq.saturating_add(1);
        if self.activity.len() > ACTIVITY_CAP {
            self.activity.pop_front();
        }
    }
}
