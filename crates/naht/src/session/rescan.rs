use super::Session;
use super::transaction::Journaled;
use super::{PROJECT_ROOT, SessionError};
use naht_core::binary::{self};
use naht_core::reconciler::{self, Direction, PatchKind};
use naht_core::state::InstanceRecord;
use naht_core::vfs::Vfs;
use naht_core::walk::Scan;
use std::collections::BTreeMap;
use std::path::Path;

fn warn_skipped<T>(scan: &Scan<T>) {
    for skipped in &scan.skipped {
        tracing::warn!(
            target: "naht::session",
            path = ?skipped.path,
            error = %skipped.error,
            "skipped a file the sync cannot use"
        );
    }
}

impl<V: Vfs> Session<V> {
    /// Re-read the filesystem and reconcile it against the Studio mirror and persisted base. Used at
    /// startup, on a filesystem event, and on an authenticated reconnect (`GET /info`), which is the
    /// re-diff that replaces a blind re-push.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the project cannot be rescanned.
    pub fn rescan(&mut self) -> Result<(), SessionError> {
        self.reconcile_all()
    }

    pub(super) fn reconcile_all(&mut self) -> Result<(), SessionError> {
        let pre_base = self.base_snapshot()?;
        self.reconcile_text(&pre_base)?;
        if self.terrain_sync {
            self.reconcile_blobs()?;
        }
        Ok(())
    }

    fn reconcile_text(
        &mut self,
        pre_base: &BTreeMap<String, InstanceRecord>,
    ) -> Result<(), SessionError> {
        let scan = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        warn_skipped(&scan);
        let patches = reconciler::reconcile(
            &mut Journaled {
                inner: &mut self.vfs,
                journal: &mut self.journal,
            },
            &self.store,
            &scan.instances,
            &self.studio,
            &scan.unseen(),
        )?;
        let removed: Vec<String> = patches
            .iter()
            .filter(|patch| patch.direction == Direction::ToFs && patch.kind == PatchKind::Delete)
            .map(|patch| patch.path.clone())
            .collect();
        self.remove_emptied_dirs(&removed);
        self.absorb(patches, pre_base)
    }

    pub(super) fn reconcile_blobs(&mut self) -> Result<(), SessionError> {
        let scan = binary::scan_blobs(&self.vfs, Path::new(PROJECT_ROOT))?;
        warn_skipped(&scan);
        let patches = binary::reconcile_blobs(
            &mut Journaled {
                inner: &mut self.vfs,
                journal: &mut self.journal,
            },
            &self.store,
            &scan.instances,
            self.studio_blobs.as_ref(),
            &scan.unseen(),
        )?;
        self.absorb_blobs(patches);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
