//! The file ↔ instance mapping: the conventions from architecture §6.
//!
//! - `*.server.luau` → `Script`, `*.client.luau` → `LocalScript`, `*.luau` → `ModuleScript`
//! - `init.luau` / `init.server.luau` / `init.client.luau` → the containing directory *becomes*
//!   that script instance
//! - a plain directory → `Folder`
//!
//! Non-default properties travel inline as [`crate::frontmatter`]; non-source files (a README, say)
//! are not Roblox instances and are skipped, not lost.

use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

use rbx_dom_weak::types::Variant;

use crate::frontmatter::{self, FrontmatterError};
use crate::snapshot::{SOURCE_PROPERTY, Snapshot};
use crate::vfs::{EntryKind, Vfs, VfsError};

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
}

/// Map a directory (and everything under it) into a [`Snapshot`] tree.
///
/// # Errors
///
/// Returns an error when `dir` cannot be read or a file is not valid UTF-8.
pub fn snapshot_dir(vfs: &impl Vfs, dir: &Path) -> Result<Snapshot, MapError> {
    let name = file_name(dir)?;
    let entries = vfs.list(dir)?;

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
    if is_script(&snapshot.class) {
        let content = script_content(snapshot);
        if snapshot.children.is_empty() {
            let file = parent.join(script_file_name(&snapshot.class, &snapshot.name));
            vfs.write(&file, content.as_bytes())?;
        } else {
            let dir = parent.join(&snapshot.name);
            vfs.create_dir(&dir)?;
            vfs.write(
                &dir.join(init_file_name(&snapshot.class)),
                content.as_bytes(),
            )?;
            write_children(vfs, &dir, snapshot)?;
        }
    } else {
        let dir = parent.join(&snapshot.name);
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

fn script_content(snapshot: &Snapshot) -> String {
    let source = snapshot.source().unwrap_or_default();
    let mut properties = snapshot.properties.clone();
    properties.remove(SOURCE_PROPERTY);
    let mut content = frontmatter::render(&properties).unwrap_or_default();
    content.push_str(source);
    content
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

fn file_name(path: &Path) -> Result<String, MapError> {
    path.file_name()
        .and_then(|os_name| os_name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| MapError::NoFileName(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::MemoryVfs;

    fn fixture() -> MemoryVfs {
        MemoryVfs::new()
            .with_file("proj/Greeter.luau", "return 1")
            .with_file("proj/runtime/main.server.luau", "print('server')")
            .with_file("proj/runtime/ui.client.luau", "print('client')")
            .with_file(
                "proj/disabled.server.luau",
                "--!naht { Disabled = true }\nprint('x')",
            )
            .with_file("proj/folder/init.luau", "return {}")
            .with_file("proj/folder/inner.luau", "return 2")
            .with_dir("proj/empty")
            .with_file("proj/README.md", "not an instance")
    }

    fn child<'a>(snapshot: &'a Snapshot, name: &str) -> &'a Snapshot {
        snapshot
            .children
            .iter()
            .find(|child| child.name == name)
            .unwrap_or_else(|| panic!("missing child {name}"))
    }

    #[test]
    fn maps_classes_and_hierarchy() {
        let vfs = fixture();
        let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
        assert_eq!(tree.class, "Folder");
        assert_eq!(tree.name, "proj");

        assert_eq!(child(&tree, "Greeter").class, "ModuleScript");

        let runtime = child(&tree, "runtime");
        assert_eq!(runtime.class, "Folder");
        assert_eq!(child(runtime, "main").class, "Script");
        assert_eq!(child(runtime, "ui").class, "LocalScript");
    }

    #[test]
    fn non_source_file_is_skipped() {
        let vfs = fixture();
        let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
        assert!(tree.children.iter().all(|child| child.name != "README.md"));
    }

    #[test]
    fn init_collapses_directory_into_the_instance() {
        let vfs = fixture();
        let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
        let folder = child(&tree, "folder");
        assert_eq!(folder.class, "ModuleScript");
        assert_eq!(folder.source(), Some("return {}"));
        assert_eq!(child(folder, "inner").class, "ModuleScript");
    }

    #[test]
    fn frontmatter_properties_are_attached() {
        let vfs = fixture();
        let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
        let disabled = child(&tree, "disabled");
        assert_eq!(disabled.class, "Script");
        assert_eq!(
            disabled.properties.get("Disabled"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(disabled.source(), Some("print('x')"));
    }

    #[test]
    fn empty_directory_maps_to_an_empty_folder() {
        let vfs = fixture();
        let tree = snapshot_dir(&vfs, Path::new("proj")).unwrap();
        let empty = child(&tree, "empty");
        assert_eq!(empty.class, "Folder");
        assert_eq!(empty.children.len(), 0);
    }

    #[test]
    fn round_trip_snapshot_to_files_to_snapshot_is_identity() {
        let source = fixture();
        let tree = snapshot_dir(&source, Path::new("proj")).unwrap();

        let mut dest = MemoryVfs::new();
        write_tree(&mut dest, Path::new(""), &tree).unwrap();
        let round_tripped = snapshot_dir(&dest, Path::new("proj")).unwrap();

        assert_eq!(round_tripped, tree);
    }

    #[test]
    fn structured_properties_survive_fs_snapshot_fs_round_trip() {
        use rbx_dom_weak::types::{Attributes, Color3, Enum, Tags, Vector3};

        let mut attributes = Attributes::new();
        attributes.insert("Health".to_owned(), Variant::Float64(100.0));
        attributes.insert("Title".to_owned(), Variant::String("hero".to_owned()));

        let project = Snapshot::new("Folder", "proj").with_child(
            Snapshot::new("ModuleScript", "Config")
                .with_property(SOURCE_PROPERTY, Variant::String("return {}".to_owned()))
                .with_property("Color", Variant::Color3(Color3::new(0.1, 0.5, 0.9)))
                .with_property("Position", Variant::Vector3(Vector3::new(1.0, 2.0, 3.0)))
                .with_property("Material", Variant::Enum(Enum::from_u32(256)))
                .with_property("Attributes", Variant::Attributes(attributes))
                .with_property(
                    "Tags",
                    Variant::Tags(Tags::from(vec!["combat".to_owned(), "npc".to_owned()])),
                ),
        );

        let mut dest = MemoryVfs::new();
        write_tree(&mut dest, Path::new(""), &project).unwrap();
        let round_tripped = snapshot_dir(&dest, Path::new("proj")).unwrap();

        assert_eq!(round_tripped, project);
    }
}
