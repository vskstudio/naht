use super::Session;
use super::{PROJECT_ROOT, SessionError};
use naht_core::binary::{self};
use naht_core::reconciler::{self};
use std::path::Path;

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
        let fs = reconciler::scan_text(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = reconciler::reconcile(&mut self.vfs, &self.store, &fs, &self.studio)?;
        self.absorb(patches, &pre_base)?;
        if self.terrain_sync {
            self.reconcile_blobs()?;
        }
        Ok(())
    }

    pub(super) fn reconcile_blobs(&mut self) -> Result<(), SessionError> {
        let fs = binary::scan_blobs(&self.vfs, Path::new(PROJECT_ROOT))?;
        let patches = binary::reconcile_blobs(&mut self.vfs, &self.store, &fs, &self.studio_blobs)?;
        self.absorb_blobs(patches);
        Ok(())
    }
}
