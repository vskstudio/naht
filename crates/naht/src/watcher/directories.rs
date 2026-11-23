//! The directories the Linux watcher keeps a watch on: the project root and every directory the
//! walk keeps, so the ignored names are never registered with the kernel.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use naht_core::walk::is_ignored_entry;

pub(super) type Directories = BTreeSet<PathBuf>;

pub(super) fn directories_to_watch(root: &Path) -> io::Result<Directories> {
    let mut wanted = Directories::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory.as_path() == root => return Err(error),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_ignored_entry(&path) && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            }
        }
        wanted.insert(directory);
    }
    Ok(wanted)
}
