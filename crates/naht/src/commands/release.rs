use super::error::CommandError;
use super::files::{create_file, create_parent_dirs};
use naht_core::build::{self, ModelFormat};
use naht_core::mapper;
use naht_core::snapshot::Snapshot;
use naht_core::vfs::DiskVfs;
use std::io::{self};
use std::path::Path;

/// Package the Studio plugin under `src` into a single installable `.rbxmx` model at `output`.
///
/// The hand-written Luau modules are assembled under one `Naht` `Folder`, so the entry `Script` and
/// its sibling `ModuleScript`s keep the `script.Parent.X` require paths they use at runtime. The
/// directory convention maps the files: `Plugin.server.luau` → the `Script`, the rest → `ModuleScript`.
pub fn package_plugin(src: &Path, output: &Path) -> Result<(), CommandError> {
    let mut tree =
        mapper::snapshot_dir(&DiskVfs::new(), src).map_err(|source| CommandError::Tree {
            path: src.to_path_buf(),
            source,
        })?;
    tree.name = "Naht".to_string();
    let root = Snapshot::new("Folder", "plugin").with_child(tree);

    create_parent_dirs(output)?;
    let file = create_file(output)?;
    build::write_model(io::BufWriter::new(file), &root, ModelFormat::Xml)?;
    Ok(())
}

/// Verify a release tag (`vX.Y.Z` or `X.Y.Z`) matches the workspace crate version, so a tagged
/// release cannot ship a binary whose version disagrees with its tag.
pub fn check_release_version(tag: &str) -> Result<(), CommandError> {
    let want = naht_core::version();
    let got = tag.strip_prefix('v').unwrap_or(tag);
    if got != want {
        return Err(CommandError::ReleaseMismatch {
            tag: tag.to_string(),
            want: want.to_string(),
        });
    }
    Ok(())
}
