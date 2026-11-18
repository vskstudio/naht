use super::{Session, SessionError};
use naht_core::hash::source_fingerprint;
use naht_core::inventory::InventoryEntry;
use naht_core::mapper::{flat_init_path, init_flat_path};
use naht_core::protocol::Change;
use naht_core::reconciler::TextInstance;
use naht_core::vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet};

type ContentKey = (String, String);

impl<V: Vfs> Session<V> {
    /// Reconcile the scripts Studio lists on attach against the mirror. Only renames are applied:
    /// a listed script the mirror lacks is paired with an unlisted mirror script of the same class
    /// and source fingerprint, and the pair becomes a delete of the old path plus an upsert of the
    /// new one. Creates, deletes and content edits without a partner are left alone and counted.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the reconciled rename cannot be applied.
    pub fn apply_inventory(&mut self, listed: &[InventoryEntry]) -> Result<(), SessionError> {
        if listed.is_empty() {
            return Ok(());
        }
        let mut claimed = BTreeSet::new();
        let mut creates = Vec::new();
        let mut left_alone: Vec<&str> = Vec::new();
        for entry in listed {
            if self.has_pending_patch(&entry.path) {
                continue;
            }
            let mirror_path = self.mirror_path_for(&entry.path);
            match self.studio.get(&mirror_path) {
                Some(known) => {
                    claimed.insert(mirror_path);
                    if known.class != entry.class || fingerprint(&known.content) != entry.hash {
                        left_alone.push(&entry.path);
                    }
                }
                None => creates.push(entry),
            }
        }
        let mut unlisted: BTreeMap<ContentKey, Vec<&TextInstance>> = BTreeMap::new();
        for (path, known) in &self.studio {
            if !claimed.contains(path) && !self.has_pending_patch(path) {
                unlisted
                    .entry((known.class.clone(), fingerprint(&known.content)))
                    .or_default()
                    .push(known);
            }
        }
        let mut changes = Vec::new();
        for entry in creates {
            let key = (entry.class.clone(), entry.hash.clone());
            match unlisted.get_mut(&key).and_then(Vec::pop) {
                Some(old) => {
                    changes.push(Change::Delete {
                        path: old.path.clone(),
                    });
                    changes.push(Change::Upsert {
                        path: entry.path.clone(),
                        class: entry.class.clone(),
                        content: old.content.clone(),
                    });
                }
                None => left_alone.push(&entry.path),
            }
        }
        left_alone.extend(unlisted.values().flatten().map(|known| known.path.as_str()));
        if !left_alone.is_empty() {
            tracing::info!(
                target: "naht::sync",
                count = left_alone.len(),
                "inventory differences left alone: only renames are applied from Studio's listing"
            );
        }
        if changes.is_empty() {
            return Ok(());
        }
        self.apply_changes(changes)
    }

    fn mirror_path_for(&self, listed: &str) -> String {
        if self.studio.contains_key(listed) {
            return listed.to_owned();
        }
        init_flat_path(listed)
            .into_iter()
            .chain(flat_init_path(listed))
            .find(|alternative| self.studio.contains_key(alternative))
            .unwrap_or_else(|| listed.to_owned())
    }
}

fn fingerprint(content: &str) -> String {
    source_fingerprint(content.as_bytes())
}
