use super::Session;
use super::transaction::Outbound;
use super::{ACTIVITY_CAP, SessionError};
use naht_core::binary::BlobPatch;
use naht_core::protocol::{EventKind, Outcome, SequencedPatch, SyncActivity, SyncEvent};
use naht_core::reconciler::{self, Direction, Patch, TextInstance};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests;

impl Session {
    /// The activity snapshot for `GET /activity`: the live pending count (unacked text patches) plus
    /// every retained event past `since` (strictly newer), oldest-first. Seqs are 1-based, so the
    /// default `since = 0` returns everything retained while a caller's cursor never re-delivers the
    /// event it last saw.
    pub fn activity(&self, since: u64) -> SyncActivity {
        let buffer_start_seq = self
            .activity
            .front()
            .map_or(self.activity_next_seq, |event| event.seq);
        let start = self.activity.partition_point(|event| event.seq <= since);
        let events = self.activity.range(start..).cloned().collect();
        SyncActivity {
            pending: u32::try_from(self.unacked.len()).unwrap_or(u32::MAX),
            buffer_start_seq,
            events,
        }
    }

    /// Confirm that the plugin applied the patches with the given sequences: each settled patch leaves
    /// the queue, and a gated one advances the base and the mirror so they stop re-diffing. A sequence
    /// no longer queued was superseded or already acked, so it confirms nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when a base record cannot be written.
    pub fn ack(&mut self, seqs: &[u64]) -> Result<(), SessionError> {
        let acked: BTreeSet<u64> = seqs.iter().copied().collect();
        let settled: Vec<String> = self
            .queue
            .iter()
            .filter(|(_, (seq, _))| acked.contains(seq))
            .map(|(path, _)| path.clone())
            .collect();
        for path in settled {
            let Some((_, patch)) = self.queue.remove(&path) else {
                continue;
            };
            if self.unacked.remove(&path).is_some() {
                self.settle(patch)?;
            }
        }
        Ok(())
    }

    fn settle(&mut self, patch: Patch) -> Result<(), SessionError> {
        if let Some(content) = &patch.content {
            reconciler::set_base(&self.store, &patch.path, &patch.class, content)?;
            self.studio.insert(
                patch.path.clone(),
                TextInstance::new(patch.path.clone(), patch.class.clone(), content.clone()),
            );
        } else {
            self.store.remove(&patch.path)?;
            self.studio.remove(&patch.path);
        }
        self.record_event(
            patch.path,
            Direction::ToStudio,
            EventKind::Text,
            Outcome::Ok,
        );
        Ok(())
    }

    pub(super) fn enqueue_blob(&mut self, patch: BlobPatch) {
        self.emit(Outbound::QueueBlob(patch));
    }

    fn push_blob_patch(&mut self, patch: BlobPatch) {
        tracing::info!(
            target: "naht::sync",
            path = ?patch.path,
            kind = ?patch.kind,
            "blob patch emitted"
        );
        self.push_event(
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

    /// Blob patches from `cursor` on, in order; earlier ones are acked and dropped.
    pub fn take_blob_patches(&mut self, cursor: u64) -> (u64, Vec<BlobPatch>) {
        self.blob_queue.retain(|_, (seq, _)| *seq >= cursor);
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
    pub fn take_patches(&self, cursor: u64) -> (u64, Vec<SequencedPatch>) {
        let mut entries: Vec<&(u64, Patch)> = self
            .queue
            .values()
            .filter(|(seq, _)| *seq >= cursor)
            .collect();
        entries.sort_by_key(|(seq, _)| *seq);
        let patches = entries
            .into_iter()
            .map(|(seq, patch)| SequencedPatch {
                seq: *seq,
                patch: patch.clone(),
            })
            .collect();
        (self.next_seq, patches)
    }

    pub(super) fn enqueue(&mut self, patch: Patch) {
        self.emit(Outbound::Queue(patch));
    }

    fn push_patch(&mut self, patch: Patch) {
        tracing::info!(
            target: "naht::sync",
            path = ?patch.path,
            kind = ?patch.kind,
            "patch emitted"
        );
        self.queue
            .insert(patch.path.clone(), (self.next_seq, patch));
        self.next_seq = self.next_seq.saturating_add(1);
    }

    pub(super) fn set_unacked(&mut self, key: &str, pending: Option<Patch>) {
        self.emit(Outbound::Unacked(key.to_owned(), pending));
    }

    pub(super) fn record_event(
        &mut self,
        path: String,
        direction: Direction,
        kind: EventKind,
        outcome: Outcome,
    ) {
        self.emit(Outbound::Event {
            path,
            direction,
            kind,
            outcome,
        });
    }

    fn emit(&mut self, outbound: Outbound) {
        match self.journal.as_mut() {
            Some(journal) => journal.outbox.push(outbound),
            None => self.apply(outbound),
        }
    }

    pub(super) fn apply(&mut self, outbound: Outbound) {
        match outbound {
            Outbound::Event {
                path,
                direction,
                kind,
                outcome,
            } => self.push_event(path, direction, kind, outcome),
            Outbound::Queue(patch) => self.push_patch(patch),
            Outbound::QueueBlob(patch) => self.push_blob_patch(patch),
            Outbound::Unacked(key, Some(pending)) => {
                self.unacked.insert(key, pending);
            }
            Outbound::Unacked(key, None) => {
                self.unacked.remove(&key);
            }
        }
    }

    fn push_event(
        &mut self,
        path: String,
        direction: Direction,
        kind: EventKind,
        outcome: Outcome,
    ) {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            });
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
