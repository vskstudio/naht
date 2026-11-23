use crate::session::transaction::Journaled;
use crate::session::{Session, SessionError};
use naht_core::reconciler;
use naht_core::vfs::Vfs;
use std::collections::BTreeSet;
use std::path::Path;

impl<V: Vfs> Session<V> {
    pub(super) fn relocate_pending(
        &mut self,
        upserted: &BTreeSet<String>,
    ) -> Result<Vec<String>, SessionError> {
        let mut vacated = Vec::new();
        for target in upserted {
            let Some(old) = self.pending_relocations.remove(target) else {
                continue;
            };
            if self.has_pending_patch(&old)
                || !self.studio.contains_key(&old)
                || !self.vfs.exists(Path::new(&old))
                || self.vfs.exists(Path::new(target))
            {
                continue;
            }
            self.make_parent_folders(target)?;
            Journaled {
                inner: &mut self.vfs,
                journal: &mut self.journal,
            }
            .rename(Path::new(&old), Path::new(target))?;
            reconciler::move_base(&self.store, &old, target)?;
            self.write_studio(old.clone(), None);
            vacated.push(old);
        }
        Ok(vacated)
    }

    fn make_parent_folders(&mut self, path: &str) -> Result<(), SessionError> {
        let mut folders = Vec::new();
        let mut current = Path::new(path).parent();
        while let Some(folder) = current.filter(|folder| !folder.as_os_str().is_empty()) {
            folders.push(folder);
            current = folder.parent();
        }
        let mut journaled = Journaled {
            inner: &mut self.vfs,
            journal: &mut self.journal,
        };
        for folder in folders.into_iter().rev() {
            if !journaled.exists(folder) {
                journaled.create_dir(folder)?;
            }
        }
        Ok(())
    }
}
