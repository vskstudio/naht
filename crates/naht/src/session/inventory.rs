use super::{Session, SessionError};
use naht_core::hash::source_fingerprint;
use naht_core::inventory::InventoryEntry;
use naht_core::mapper::{flat_init_path, init_flat_path};
use naht_core::protocol::Change;
use naht_core::reconciler::TextInstance;
use naht_core::vfs::Vfs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type ContentKey = (String, String);

#[derive(Default)]
struct Plan {
    changes: Vec<Change>,
    relocations: BTreeMap<String, String>,
    wanted: Vec<String>,
}

impl<V: Vfs> Session<V> {
    /// Reconcile the scripts Studio lists on attach against the mirror. A listed script the mirror
    /// lacks is paired with an unlisted mirror script of the same class and source fingerprint, and
    /// the pair becomes a delete of the old path plus an upsert of the new one. Under a folder the
    /// listing shows renamed (its scripts paired into one new folder, with nothing still listed under
    /// the old one), the old folder's other scripts follow it: a script Studio deleted moves with its
    /// disk bytes, and a script whose Studio content changed moves when the plugin sends that content,
    /// which the returned paths name. Creates and deletes without a folder rename are left alone.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError`] when the reconciled changes cannot be applied.
    pub fn apply_inventory(
        &mut self,
        listed: &[InventoryEntry],
    ) -> Result<Vec<String>, SessionError> {
        let plan = self.plan_inventory(listed);
        self.pending_relocations.clear();
        if !plan.changes.is_empty() {
            self.apply_changes(plan.changes)?;
        }
        self.pending_relocations = plan.relocations;
        Ok(plan.wanted)
    }

    fn plan_inventory(&self, listed: &[InventoryEntry]) -> Plan {
        let mut plan = Plan::default();
        if listed.is_empty() {
            return plan;
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
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        let mut paired_targets = BTreeSet::new();
        for entry in creates {
            let key = (entry.class.clone(), entry.hash.clone());
            match unlisted.get_mut(&key).and_then(Vec::pop) {
                Some(old) => {
                    plan.changes.push(Change::Delete {
                        path: old.path.clone(),
                    });
                    plan.changes.push(Change::Upsert {
                        path: entry.path.clone(),
                        class: entry.class.clone(),
                        content: old.content.clone(),
                    });
                    pairs.push((old.path.as_str(), entry.path.as_str()));
                    paired_targets.insert(entry.path.as_str());
                }
                None => left_alone.push(&entry.path),
            }
        }
        let renames = inferred_folder_renames(&pairs, listed);
        let listed_by_path: BTreeMap<&str, &InventoryEntry> = listed
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        for known in unlisted.values().flatten() {
            let Some(target) = renamed_path(&known.path, &renames) else {
                left_alone.push(&known.path);
                continue;
            };
            if paired_targets.contains(target.as_str())
                || self.studio.contains_key(&target)
                || self.has_pending_patch(&target)
                || !self.vfs.exists(Path::new(&known.path))
            {
                left_alone.push(&known.path);
                continue;
            }
            match listed_by_path.get(target.as_str()) {
                Some(entry) if entry.class == known.class => {
                    plan.relocations.insert(target.clone(), known.path.clone());
                    plan.wanted.push(target);
                }
                Some(_) => left_alone.push(&known.path),
                None => {
                    plan.changes.push(Change::Delete {
                        path: known.path.clone(),
                    });
                    plan.changes.push(Change::Upsert {
                        path: target,
                        class: known.class.clone(),
                        content: known.content.clone(),
                    });
                }
            }
        }
        if !left_alone.is_empty() {
            tracing::info!(
                target: "naht::sync",
                count = left_alone.len(),
                "inventory differences left alone: only renames are applied from Studio's listing"
            );
        }
        plan
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

fn parent_folder(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(folder, _)| folder)
        .filter(|folder| !folder.is_empty())
}

fn inferred_folder_renames(
    pairs: &[(&str, &str)],
    listed: &[InventoryEntry],
) -> BTreeMap<String, String> {
    let mut targets: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    for &(old, new) in pairs {
        let (Some(from), Some(to)) = (parent_folder(old), parent_folder(new)) else {
            continue;
        };
        if from == to {
            continue;
        }
        let target = targets.entry(from).or_insert(Some(to));
        if *target != Some(to) {
            *target = None;
        }
    }
    targets
        .into_iter()
        .filter_map(|(from, to)| {
            let to = to?;
            let prefix = format!("{from}/");
            let still_listed = listed.iter().any(|entry| entry.path.starts_with(&prefix));
            (!still_listed).then(|| (from.to_owned(), to.to_owned()))
        })
        .collect()
}

fn renamed_path(path: &str, renames: &BTreeMap<String, String>) -> Option<String> {
    renames
        .iter()
        .filter_map(|(from, to)| {
            let rest = path.strip_prefix(from.as_str())?.strip_prefix('/')?;
            Some((from.len(), format!("{to}/{rest}")))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, target)| target)
}
