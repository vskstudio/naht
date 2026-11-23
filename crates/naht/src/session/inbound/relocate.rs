use crate::session::transaction::Journaled;
use crate::session::{Session, SessionError};
use naht_core::reconciler::{self, TextInstance};
use naht_core::vfs::Vfs;
use std::collections::BTreeSet;
use std::mem;
use std::path::Path;

impl<V: Vfs> Session<V> {
    pub(super) fn relocate_pending(
        &mut self,
        upserted: &BTreeSet<String>,
    ) -> Result<Vec<String>, SessionError> {
        let mut relocations = mem::take(&mut self.pending_relocations);
        let mut vacated = Vec::new();
        for target in upserted {
            let Some(old) = relocations.remove(target) else {
                continue;
            };
            if !self.studio.contains_key(&old)
                || !self.vfs.exists(Path::new(&old))
                || self.vfs.exists(Path::new(target))
                || self.has_pending_patch(target)
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
            self.rekey_patches(&old, target);
            vacated.push(old);
        }
        Ok(vacated)
    }

    pub(super) fn move_studio_deleted(
        &mut self,
        moves: &[(String, String)],
    ) -> Result<Vec<String>, SessionError> {
        let mut vacated = Vec::new();
        for (old, target) in moves {
            let Some(known) = self.studio.get(old).cloned() else {
                continue;
            };
            if !self.vfs.exists(Path::new(old))
                || self.vfs.exists(Path::new(target))
                || self.has_pending_patch(target)
            {
                continue;
            }
            self.make_parent_folders(target)?;
            Journaled {
                inner: &mut self.vfs,
                journal: &mut self.journal,
            }
            .rename(Path::new(old), Path::new(target))?;
            reconciler::move_base(&self.store, old, target)?;
            self.write_studio(old.clone(), None);
            self.write_studio(
                target.clone(),
                Some(TextInstance::new(
                    target.clone(),
                    known.class,
                    known.content,
                )),
            );
            vacated.push(old.clone());
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
