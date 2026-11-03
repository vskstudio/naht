use super::error::CommandError;
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};
use std::io;
use std::path::{Path, PathBuf};

/// Naht's internal directory, holding the state database; never part of a build.
pub(super) const INTERNAL_DIR: &str = ".naht";

pub(super) fn read_optional(path: &Path) -> Result<String, CommandError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(CommandError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn write_file(path: &Path, contents: &str) -> Result<(), CommandError> {
    std::fs::write(path, contents).map_err(|source| CommandError::Write {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn create_file(path: &Path) -> Result<std::fs::File, CommandError> {
    std::fs::File::create(path).map_err(|source| CommandError::Create {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn create_parent_dirs(output: &Path) -> Result<(), CommandError> {
    match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            std::fs::create_dir_all(parent).map_err(|source| CommandError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })
        }
        _ => Ok(()),
    }
}

pub(super) fn open_store(root: &Path) -> Result<StateStore, CommandError> {
    let path = root.join(INTERNAL_DIR).join("state.db");
    StateStore::open(&path).map_err(|source| CommandError::StateOpen { path, source })
}

pub(super) fn rooted(root: &Path) -> Result<RootedVfs<DiskVfs>, CommandError> {
    Ok(RootedVfs::new(canonical(root)?, DiskVfs::new()))
}

pub(super) fn canonical(root: &Path) -> Result<PathBuf, CommandError> {
    root.canonicalize()
        .map_err(|source| CommandError::ProjectDir {
            path: root.to_path_buf(),
            source,
        })
}

pub(super) fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("naht")
        .to_string()
}
