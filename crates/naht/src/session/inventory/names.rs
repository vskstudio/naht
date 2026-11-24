use std::collections::{BTreeMap, BTreeSet};

use naht_core::inventory::InventoryEntry;
use naht_core::reconciler::TextInstance;

type Members = BTreeSet<(String, String)>;

const MIN_SHARED_SCRIPTS: usize = 2;

pub(super) struct NameEvidence<'a> {
    pub(super) unlisted: &'a [&'a TextInstance],
    pub(super) paired_mirror: &'a BTreeSet<&'a str>,
    pub(super) unmatched: &'a [&'a InventoryEntry],
    pub(super) listed: &'a [InventoryEntry],
    pub(super) mirror_paths: &'a [&'a str],
}

impl NameEvidence<'_> {
    /// Old folders paired with a new folder holding exactly the same script names and classes,
    /// at least two of them, when no other folder on either side holds that same set.
    pub(super) fn folder_renames(&self) -> BTreeMap<String, String> {
        let old = self.old_folders();
        let new = self.new_folders();
        let olds_by_members = by_members(&old);
        let news_by_members = by_members(&new);
        olds_by_members
            .into_iter()
            .filter(|(members, _)| members.len() >= MIN_SHARED_SCRIPTS)
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
        let blocked: BTreeSet<&str> = self
            .listed
            .iter()
            .map(|entry| entry.path.as_str())
            .chain(self.paired_mirror.iter().copied())
            .flat_map(ancestors)
            .collect();
        members_by_folder(
            self.unlisted
                .iter()
                .map(|known| (known.path.as_str(), known.class.as_str())),
            &blocked,
        )
    }

    fn new_folders(&self) -> BTreeMap<&str, Members> {
        let unmatched: BTreeSet<&str> = self
            .unmatched
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        let blocked: BTreeSet<&str> = self
            .listed
            .iter()
            .map(|entry| entry.path.as_str())
            .filter(|path| !unmatched.contains(path))
            .chain(self.mirror_paths.iter().copied())
            .flat_map(ancestors)
            .collect();
        members_by_folder(
            self.unmatched
                .iter()
                .map(|entry| (entry.path.as_str(), entry.class.as_str())),
            &blocked,
        )
    }
}

pub(super) fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/')
        .filter_map(move |(at, _)| path.get(..at))
        .filter(|folder| !folder.is_empty())
}

fn members_by_folder<'a>(
    scripts: impl Iterator<Item = (&'a str, &'a str)>,
    blocked: &BTreeSet<&str>,
) -> BTreeMap<&'a str, Members> {
    let mut found: BTreeMap<&str, Members> = BTreeMap::new();
    for (path, class) in scripts {
        for folder in ancestors(path).filter(|folder| !blocked.contains(folder)) {
            if let Some(rest) = path
                .strip_prefix(folder)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                found
                    .entry(folder)
                    .or_default()
                    .insert((rest.to_owned(), class.to_owned()));
            }
        }
    }
    found
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
