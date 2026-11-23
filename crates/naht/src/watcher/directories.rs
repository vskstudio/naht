//! The directories the Linux watcher keeps a watch on: the project root and every directory the
//! walk keeps, so the ignored names are never registered with the kernel.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use naht_core::walk::is_ignored_entry;
use notify_debouncer_full::notify::{
    self, Config, ErrorKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _,
    WatcherKind,
};
use notify_debouncer_full::{
    DebounceEventHandler, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer_opt,
};

use super::{DEBOUNCE, WatchError};

pub(super) type Directories = BTreeSet<PathBuf>;

pub(super) trait DirectoryWatch {
    fn watch_directory(&mut self, path: &Path) -> notify::Result<()>;
    fn unwatch_directory(&mut self, path: &Path) -> notify::Result<()>;
}

thread_local! {
    static CONSTRUCTED: RefCell<Option<Inotify>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub(super) struct Inotify(Arc<Mutex<RecommendedWatcher>>);

impl Inotify {
    fn lock(&self) -> MutexGuard<'_, RecommendedWatcher> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl notify::Watcher for Inotify {
    fn new<F: notify::EventHandler>(event_handler: F, config: Config) -> notify::Result<Self> {
        let inotify = Self(Arc::new(Mutex::new(RecommendedWatcher::new(
            event_handler,
            config,
        )?)));
        CONSTRUCTED.with_borrow_mut(|slot| *slot = Some(inotify.clone()));
        Ok(inotify)
    }

    fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
        self.lock().watch(path, recursive_mode)
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        self.lock().unwatch(path)
    }

    fn kind() -> WatcherKind {
        RecommendedWatcher::kind()
    }
}

impl DirectoryWatch for Inotify {
    fn watch_directory(&mut self, path: &Path) -> notify::Result<()> {
        self.watch(path, RecursiveMode::NonRecursive)
    }

    fn unwatch_directory(&mut self, path: &Path) -> notify::Result<()> {
        self.unwatch(path)
    }
}

pub(super) fn debounce(
    handler: impl DebounceEventHandler,
) -> Result<(Debouncer<Inotify, RecommendedCache>, Inotify), WatchError> {
    let debouncer = new_debouncer_opt::<_, Inotify, RecommendedCache>(
        DEBOUNCE,
        None,
        handler,
        RecommendedCache::new(),
        Config::default(),
    )?;
    let watch = CONSTRUCTED
        .with_borrow_mut(Option::take)
        .ok_or_else(|| notify::Error::generic("the debouncer did not build its watcher"))?;
    Ok((debouncer, watch))
}

pub(super) fn start_with(
    watch: &mut impl DirectoryWatch,
    root: &Path,
) -> Result<Directories, WatchError> {
    watch.watch_directory(root)?;
    let mut directories = Directories::from([root.to_path_buf()]);
    let failures = sync(watch, &mut directories, root, &BTreeSet::new())?;
    match failures.into_iter().next() {
        Some((_, error)) => Err(error.into()),
        None => Ok(directories),
    }
}

pub(super) fn sync(
    watch: &mut impl DirectoryWatch,
    directories: &mut Directories,
    root: &Path,
    touched: &BTreeSet<PathBuf>,
) -> notify::Result<Vec<(PathBuf, notify::Error)>> {
    let wanted = directories_to_watch(root).map_err(notify::Error::io)?;
    let released = released_directories(directories, &wanted, touched);
    for directory in released {
        release(watch, &directory);
        directories.remove(&directory);
    }
    let mut failures = Vec::new();
    for directory in wanted {
        if directories.contains(&directory) {
            continue;
        }
        match watch.watch_directory(&directory) {
            Ok(()) => {
                directories.insert(directory);
            }
            Err(error) if is_vanished(&error) => {}
            Err(error) => failures.push((directory, error)),
        }
    }
    Ok(failures)
}

fn released_directories(
    directories: &Directories,
    wanted: &Directories,
    touched: &BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    let mut released: BTreeSet<PathBuf> = directories
        .iter()
        .filter(|directory| !wanted.contains(*directory))
        .cloned()
        .collect();
    for path in touched {
        released.extend(
            directories
                .range(path.clone()..)
                .take_while(|directory| directory.starts_with(path))
                .cloned(),
        );
    }
    released
}

pub(super) fn touched_paths(events: &[DebouncedEvent]) -> BTreeSet<PathBuf> {
    events
        .iter()
        .filter(|event| !matches!(event.kind, EventKind::Access(_)))
        .flat_map(|event| event.paths.iter().cloned())
        .collect()
}

fn release(watch: &mut impl DirectoryWatch, directory: &Path) {
    if let Err(error) = watch.unwatch_directory(directory) {
        tracing::debug!(target: "naht::watcher", ?error, path = %directory.display(), "watch already released");
    }
}

fn is_vanished(error: &notify::Error) -> bool {
    match &error.kind {
        ErrorKind::PathNotFound => true,
        ErrorKind::Io(io_error) => io_error.kind() == io::ErrorKind::NotFound,
        ErrorKind::Generic(_)
        | ErrorKind::WatchNotFound
        | ErrorKind::InvalidConfig(_)
        | ErrorKind::MaxFilesWatch => false,
    }
}

pub(super) fn directories_to_watch(root: &Path) -> io::Result<Directories> {
    let mut wanted = Directories::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory.as_path() == root => return Err(error),
            Err(error) => {
                tracing::warn!(target: "naht::watcher", ?error, path = %directory.display(), "cannot read directory, its subdirectories are not watched");
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(target: "naht::watcher", ?error, path = %directory.display(), "cannot read a directory entry");
                    continue;
                }
            };
            let path = entry.path();
            if is_ignored_entry(&path) {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(target: "naht::watcher", ?error, path = %path.display(), "cannot read the entry type");
                }
            }
        }
        wanted.insert(directory);
    }
    Ok(wanted)
}
