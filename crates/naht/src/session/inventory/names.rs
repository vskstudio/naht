use std::collections::{BTreeMap, BTreeSet};

use naht_core::inventory::InventoryEntry;
use naht_core::reconciler::TextInstance;

use super::is_under;

type Members = BTreeSet<(String, String)>;

pub(super) struct NameEvidence<'a> {
    pub(super) unlisted: &'a [&'a TextInstance],
    pub(super) paired_mirror: &'a BTreeSet<&'a str>,
    pub(super) unmatched: &'a [&'a InventoryEntry],
    pub(super) listed: &'a [InventoryEntry],
}

impl NameEvidence<'_> {
    /// Old folders paired with a new folder holding exactly the same script names and classes,
    /// when no other folder on either side holds that same set.
    pub(super) fn folder_renames(
        &self,
        mirror_has_folder: impl Fn(&str) -> bool,
    ) -> BTreeMap<String, String> {
        let old = self.old_folders();
        let new = self.new_folders(mirror_has_folder);
        let olds_by_members = by_members(&old);
        let news_by_members = by_members(&new);
        olds_by_members
            .into_iter()
            .filter_map(|(members, olds)| {
                let news = news_by_members.get(members)?;
                match (olds.as_slice(), news.as_slice()) {
                    ([from], [to]) if from != to => Some(((*from).to_owned(), (*to).to_owned())),
                    _ => None,
                }
            })
            .collect()
    }

    fn old_folders(&self) -> BTreeMap<&str, Members> {
        let candidates: BTreeSet<&str> = self
            .unlisted
            .iter()
            .flat_map(|known| ancestors(&known.path))
            .filter(|folder| {
                !self
                    .listed
                    .iter()
                    .any(|entry| is_under(&entry.path, folder))
                    && !self.paired_mirror.iter().any(|path| is_under(path, folder))
            })
            .collect();
        candidates
            .into_iter()
            .map(|folder| {
                let members = members_under(
                    folder,
                    self.unlisted
                        .iter()
                        .map(|known| (known.path.as_str(), known.class.as_str())),
                );
                (folder, members)
            })
            .collect()
    }

    fn new_folders(&self, mirror_has_folder: impl Fn(&str) -> bool) -> BTreeMap<&str, Members> {
        let unmatched: BTreeSet<&str> = self
            .unmatched
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        let candidates: BTreeSet<&str> = self
            .unmatched
            .iter()
            .flat_map(|entry| ancestors(&entry.path))
            .filter(|folder| {
                !mirror_has_folder(folder)
                    && self
                        .listed
                        .iter()
                        .filter(|entry| is_under(&entry.path, folder))
                        .all(|entry| unmatched.contains(entry.path.as_str()))
            })
            .collect();
        candidates
            .into_iter()
            .map(|folder| {
                let members = members_under(
                    folder,
                    self.unmatched
                        .iter()
                        .map(|entry| (entry.path.as_str(), entry.class.as_str())),
                );
                (folder, members)
            })
            .collect()
    }
}

fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/')
        .filter_map(move |(at, _)| path.get(..at))
        .filter(|folder| !folder.is_empty())
}

fn members_under<'a>(folder: &str, scripts: impl Iterator<Item = (&'a str, &'a str)>) -> Members {
    scripts
        .filter_map(|(path, class)| {
            let rest = path.strip_prefix(folder)?.strip_prefix('/')?;
            Some((rest.to_owned(), class.to_owned()))
        })
        .collect()
}

fn by_members<'a, 'f>(
    folders: &'a BTreeMap<&'f str, Members>,
) -> BTreeMap<&'a Members, Vec<&'f str>> {
    let mut grouped: BTreeMap<&Members, Vec<&str>> = BTreeMap::new();
    for (folder, members) in folders {
        grouped.entry(members).or_default().push(folder);
    }
    grouped
}
