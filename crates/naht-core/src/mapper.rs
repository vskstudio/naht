//! The file ↔ instance mapping: the conventions from architecture §6.
//!
//! - `*.server.luau` → `Script`, `*.client.luau` → `LocalScript`, `*.luau` → `ModuleScript`
//! - `init.luau` / `init.server.luau` / `init.client.luau` → the containing directory *becomes*
//!   that script instance
//! - a plain directory → `Folder`
//!
//! Non-default properties travel inline as [`crate::frontmatter`]; non-source files (a README, say)
//! are not Roblox instances and are skipped, not lost.

use crate::vfs::{is_forbidden_name_character, is_windows_device_name};
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

use rbx_dom_weak::types::Variant;

use crate::frontmatter::{self, FrontmatterError};
use crate::snapshot::{SOURCE_PROPERTY, Snapshot};
use crate::vfs::{EntryKind, Vfs, VfsError};
use crate::walk::is_ignored_entry;

const SCRIPT: &str = "Script";
const LOCAL_SCRIPT: &str = "LocalScript";
const MODULE_SCRIPT: &str = "ModuleScript";
const FOLDER: &str = "Folder";

/// Errors from mapping between files and snapshots.
#[derive(Debug, thiserror::Error)]
pub enum MapError {
    /// A filesystem operation failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// A frontmatter directive could not be parsed.
    #[error(transparent)]
    Frontmatter(#[from] FrontmatterError),
    /// A script body was not valid UTF-8.
    #[error("non-UTF-8 script source: {}", .path.display())]
    NonUtf8 {
        /// The script whose body could not be decoded.
        path: PathBuf,
        /// The decoding failure.
        #[source]
        source: FromUtf8Error,
    },
    /// A path had no final component to name an instance after.
    #[error("path has no file name: {0}")]
    NoFileName(PathBuf),
    /// An instance name that cannot be a single, safe path component.
    #[error("unsafe instance name: {0:?}")]
    UnsafeName(String),
}

/// Map a directory (and everything under it) into a [`Snapshot`] tree.
///
/// # Errors
///
/// Returns an error when `dir` cannot be read or a file is not valid UTF-8.
pub fn snapshot_dir(vfs: &impl Vfs, dir: &Path) -> Result<Snapshot, MapError> {
    let name = file_name(dir)?;
    let entries: Vec<_> = vfs
        .list(dir)?
        .into_iter()
        .filter(|entry| !is_ignored_entry(&entry.path))
        .collect();

    let init = entries.iter().find_map(|entry| {
        if entry.kind != EntryKind::File {
            return None;
        }
        let class = init_class(entry.path.file_name()?.to_str()?)?;
        Some((entry.path.clone(), class))
    });

    let mut snapshot = match &init {
        Some((path, class)) => script_snapshot(vfs, path, class, &name)?,
        None => Snapshot::new(FOLDER, name),
    };

    let init_path = init.map(|(path, _)| path);
    for entry in &entries {
        if Some(&entry.path) == init_path.as_ref() {
            continue;
        }
        match entry.kind {
            EntryKind::Dir => snapshot.push_child(snapshot_dir(vfs, &entry.path)?),
            EntryKind::File => {
                let Some(file_name) = entry.path.file_name().and_then(|os_name| os_name.to_str())
                else {
                    continue;
                };
                if let Some((class, child_name)) = script_class(file_name) {
                    snapshot.push_child(script_snapshot(vfs, &entry.path, class, &child_name)?);
                }
            }
        }
    }
    Ok(snapshot)
}

/// Write a [`Snapshot`] tree into `parent`, the inverse of [`snapshot_dir`].
///
/// # Errors
///
/// Returns an error when a directory or file cannot be written.
pub fn write_tree(vfs: &mut impl Vfs, parent: &Path, snapshot: &Snapshot) -> Result<(), MapError> {
    let name = checked_name(&snapshot.name)?;
    if is_script(&snapshot.class) {
        let content = script_content(snapshot)?;
        if snapshot.children.is_empty() {
            let file = parent.join(script_file_name(&snapshot.class, name));
            vfs.write(&file, content.as_bytes())?;
        } else {
            let dir = parent.join(name);
            vfs.create_dir(&dir)?;
            vfs.write(
                &dir.join(init_file_name(&snapshot.class)),
                content.as_bytes(),
            )?;
            write_children(vfs, &dir, snapshot)?;
        }
    } else {
        let dir = parent.join(name);
        vfs.create_dir(&dir)?;
        write_children(vfs, &dir, snapshot)?;
    }
    Ok(())
}

fn write_children(vfs: &mut impl Vfs, dir: &Path, snapshot: &Snapshot) -> Result<(), MapError> {
    for child in &snapshot.children {
        write_tree(vfs, dir, child)?;
    }
    Ok(())
}

fn script_snapshot(
    vfs: &impl Vfs,
    path: &Path,
    class: &str,
    name: &str,
) -> Result<Snapshot, MapError> {
    let bytes = vfs.read(path)?;
    let body = String::from_utf8(bytes).map_err(|source| MapError::NonUtf8 {
        path: path.to_path_buf(),
        source,
    })?;
    let (mut properties, source) = frontmatter::split(&body)?;
    properties.insert(SOURCE_PROPERTY.to_owned(), Variant::String(source));
    Ok(Snapshot {
        name: name.to_owned(),
        class: class.to_owned(),
        properties,
        children: Vec::new(),
    })
}

fn script_content(snapshot: &Snapshot) -> Result<String, MapError> {
    let source = snapshot.source().unwrap_or_default();
    let mut properties = snapshot.properties.clone();
    properties.remove(SOURCE_PROPERTY);
    let mut content = frontmatter::render(&properties)?;
    content.push_str(source);
    Ok(content)
}

fn is_script(class: &str) -> bool {
    matches!(class, SCRIPT | LOCAL_SCRIPT | MODULE_SCRIPT)
}

pub(crate) fn script_class(file_name: &str) -> Option<(&'static str, String)> {
    for (suffix, class) in [
        (".server.luau", SCRIPT),
        (".client.luau", LOCAL_SCRIPT),
        (".luau", MODULE_SCRIPT),
    ] {
        if let Some(stem) = file_name.strip_suffix(suffix)
            && !stem.is_empty()
        {
            return Some((class, stem.to_owned()));
        }
    }
    None
}

pub(crate) fn init_class(file_name: &str) -> Option<&'static str> {
    match file_name {
        "init.server.luau" => Some(SCRIPT),
        "init.client.luau" => Some(LOCAL_SCRIPT),
        "init.luau" => Some(MODULE_SCRIPT),
        _ => None,
    }
}

/// Returns the folder that holds an init script path, or `None` when the path is not an init file.
pub fn init_folder(path: &str) -> Option<&str> {
    let (parent, file_name) = split_parent(path);
    init_class(file_name)?;
    parent
}

/// Returns the flat script path that an init script's folder would be written as.
pub fn init_flat_path(path: &str) -> Option<String> {
    let (parent, file_name) = split_parent(path);
    let class = init_class(file_name)?;
    let (grandparent, folder_name) = split_parent(parent?);
    Some(join_path(
        grandparent,
        &script_file_name(class, folder_name),
    ))
}

/// Returns the init script path a flat script would be written as inside its folder.
pub fn flat_init_path(path: &str) -> Option<String> {
    let (parent, file_name) = split_parent(path);
    let (class, stem) = script_class(file_name)?;
    let folder = join_path(parent, &stem);
    Some(join_path(Some(&folder), init_file_name(class)))
}

fn split_parent(path: &str) -> (Option<&str>, &str) {
    match path.rsplit_once('/') {
        Some((parent, file_name)) => (Some(parent), file_name),
        None => (None, path),
    }
}

fn join_path(parent: Option<&str>, name: &str) -> String {
    match parent {
        Some(parent) => format!("{parent}/{name}"),
        None => name.to_owned(),
    }
}

fn script_file_name(class: &str, name: &str) -> String {
    match class {
        SCRIPT => format!("{name}.server.luau"),
        LOCAL_SCRIPT => format!("{name}.client.luau"),
        _ => format!("{name}.luau"),
    }
}

fn init_file_name(class: &str) -> &'static str {
    match class {
        SCRIPT => "init.server.luau",
        LOCAL_SCRIPT => "init.client.luau",
        _ => "init.luau",
    }
}

fn checked_name(name: &str) -> Result<&str, MapError> {
    if name.is_empty()
        || name.starts_with('.')
        || name.chars().any(is_forbidden_name_character)
        || is_windows_device_name(name)
    {
        return Err(MapError::UnsafeName(name.to_owned()));
    }
    Ok(name)
}

fn file_name(path: &Path) -> Result<String, MapError> {
    path.file_name()
        .and_then(|os_name| os_name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| MapError::NoFileName(path.to_path_buf()))
}

#[cfg(test)]
mod tests;
