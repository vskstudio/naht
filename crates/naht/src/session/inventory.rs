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
    moves: Vec<(String, String)>,
    relocations: BTreeMap<String, String>,
    wanted: Vec<String>,
}

impl<V: Vfs> Session<V> {
    /// Reconcile the scripts Studio lists on attach against the mirror. A listed script the mirror
    /// lacks is paired with an unlisted mirror script of the same class and source fingerprint, but
    /// only when that key has exactly one candidate on each side. A folder is followed as renamed
    /// only on unambiguous evidence: every pair leaving it lands in one new folder, no listed script
    /// is still under it, no unlisted script under it shares an ambiguous key, and every unpaired
    /// listed script lies under that new folder. The scripts it leaves behind move with it: a script
    /// Studio deleted moves with its disk bytes, and a script whose Studio content changed moves when
    /// the plugin sends that content, which the returned paths name. Everything else is left alone.
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
        if !plan.changes.is_empty() || !plan.moves.is_empty() {
            self.apply_plan(plan.changes, &plan.moves)?;
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
        let mut creates: Vec<&InventoryEntry> = Vec::new();
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
        let unlisted = self.unlisted_scripts(&claimed);
        let (pairs, ambiguous) = unambiguous_pairs(&creates, &unlisted);
        let paired_mirror: BTreeSet<&str> =
            pairs.iter().map(|(old, _)| old.path.as_str()).collect();
        let paired_creates: BTreeSet<&str> =
            pairs.iter().map(|(_, new)| new.path.as_str()).collect();
        for (old, new) in &pairs {
            plan.changes.push(Change::Delete {
                path: old.path.clone(),
            });
            plan.changes.push(Change::Upsert {
                path: new.path.clone(),
                class: new.class.clone(),
                content: old.content.clone(),
            });
        }
        let pair_paths: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(old, new)| (old.path.as_str(), new.path.as_str()))
            .collect();
        let unmatched: Vec<&str> = creates
            .iter()
            .map(|entry| entry.path.as_str())
            .filter(|path| !paired_creates.contains(path))
            .collect();
        left_alone.extend(unmatched.iter().copied());
        let renames =
            inferred_folder_renames(&pair_paths, &unlisted, &ambiguous, &unmatched, listed);
        let listed_by_path: BTreeMap<&str, &InventoryEntry> = listed
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        for known in unlisted
            .iter()
            .filter(|known| !paired_mirror.contains(known.path.as_str()))
        {
            let Some(target) = renamed_path(&known.path, &renames) else {
                left_alone.push(&known.path);
                continue;
            };
            if paired_creates.contains(target.as_str())
                || self.studio.contains_key(&target)
                || self.has_pending_patch(&target)
                || self.vfs.exists(Path::new(&target))
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
                None => plan.moves.push((known.path.clone(), target)),
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

    fn unlisted_scripts(&self, claimed: &BTreeSet<String>) -> Vec<&TextInstance> {
        self.studio
            .iter()
            .filter(|(path, _)| !claimed.contains(*path) && !self.has_pending_patch(path))
            .map(|(_, known)| known)
            .collect()
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

fn group_by_key<T>(
    items: impl IntoIterator<Item = (ContentKey, T)>,
) -> BTreeMap<ContentKey, Vec<T>> {
    let mut groups: BTreeMap<ContentKey, Vec<T>> = BTreeMap::new();
    for (key, item) in items {
        groups.entry(key).or_default().push(item);
    }
    groups
}

fn unambiguous_pairs<'a>(
    creates: &[&'a InventoryEntry],
    unlisted: &[&'a TextInstance],
) -> (
    Vec<(&'a TextInstance, &'a InventoryEntry)>,
    BTreeSet<ContentKey>,
) {
    let creates_by_key = group_by_key(
        creates
            .iter()
            .map(|entry| ((entry.class.clone(), entry.hash.clone()), *entry)),
    );
    let unlisted_by_key = group_by_key(
        unlisted
            .iter()
            .map(|known| ((known.class.clone(), fingerprint(&known.content)), *known)),
    );
    let mut pairs = Vec::new();
    let mut ambiguous = BTreeSet::new();
    for (key, new_entries) in &creates_by_key {
        let Some(old_entries) = unlisted_by_key.get(key) else {
            continue;
        };
        match (new_entries.as_slice(), old_entries.as_slice()) {
            ([new], [old]) => pairs.push((*old, *new)),
            _ => {
                ambiguous.insert(key.clone());
            }
        }
    }
    (pairs, ambiguous)
}

fn parent_folder(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(folder, _)| folder)
        .filter(|folder| !folder.is_empty())
}

fn inferred_folder_renames(
    pairs: &[(&str, &str)],
    unlisted: &[&TextInstance],
    ambiguous: &BTreeSet<ContentKey>,
    unmatched: &[&str],
    listed: &[InventoryEntry],
) -> BTreeMap<String, String> {
    let mut destinations: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for &(old, new) in pairs {
        if let (Some(from), Some(to)) = (parent_folder(old), parent_folder(new)) {
            destinations.entry(from).or_default().insert(to);
        }
    }
    destinations
        .into_iter()
        .filter_map(|(from, targets)| {
            let mut targets = targets.into_iter();
            let (Some(to), None) = (targets.next(), targets.next()) else {
                return None;
            };
            if from == to {
                return None;
            }
            let from_prefix = format!("{from}/");
            let to_prefix = format!("{to}/");
            let still_listed = listed
                .iter()
                .any(|entry| entry.path.starts_with(&from_prefix));
            let ambiguous_member = unlisted.iter().any(|known| {
                known.path.starts_with(&from_prefix)
                    && ambiguous.contains(&(known.class.clone(), fingerprint(&known.content)))
            });
            let stray_create = unmatched.iter().any(|path| !path.starts_with(&to_prefix));
            (!still_listed && !ambiguous_member && !stray_create)
                .then(|| (from.to_owned(), to.to_owned()))
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
