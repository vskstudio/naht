use super::Session;
use super::{PROJECT_ROOT, SessionError};
use naht_core::binary::{self};
use naht_core::reconciler::{self};
use naht_core::walk::Scan;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn readable<T>(scan: Scan<T>) -> BTreeMap<String, T> {
    for skipped in &scan.skipped {
        tracing::warn!(
            target: "naht::session",
            path = %skipped.path.display(),
            error = %skipped.error,
            "skipped a file that could not be read"
        );
    }
    scan.instances
}

impl Session {
    /// Re-read the filesystem and reconcile it against the Studio mirror and persisted base. Used on
    /// a filesystem event and on reconnect (`GET /info`), the latter is the re-diff that replaces a
    /// blind re-push.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the project cannot be rescanned.
    pub fn rescan(&mut self) -> Result<(), SessionError> {
        let pre_base = self.base_snapshot()?;
        let fs = readable(reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?);
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches, &pre_base)?;
        if self.terrain_sync {
            self.reconcile_blobs()?;
        }
        Ok(())
    }

    pub(super) fn reconcile_blobs(&mut self) -> Result<(), SessionError> {
        let fs = readable(binary::scan_blobs(&self.vfs, Path::new(PROJECT_ROOT))?);
        let patches = binary::reconcile_blobs(&mut self.vfs, &self.store, &fs, &self.studio_blobs)?;
        self.absorb_blobs(patches);
        Ok(())
    }
}
