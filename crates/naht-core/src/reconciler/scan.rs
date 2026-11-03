use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::mapper::{init_class, script_class};
use crate::vfs::{EntryKind, Vfs};

use super::model::{ReconcileError, TextInstance};

/// Walk `dir` in `vfs` and collect every source file into the flat, path-keyed text-instance map
/// the reconciler consumes.
///
/// Class assignment follows the same conventions as the mapper (`*.server.luau` → `Script`,
/// `*.client.luau` → `LocalScript`, `*.luau`/`init.luau` → `ModuleScript`); non-source files are
/// skipped, not lost. The daemon calls this each reconcile so the filesystem side is always read
/// fresh from disk rather than cached.
pub fn scan_text(
    vfs: &impl Vfs,
    dir: &Path,
) -> Result<BTreeMap<String, TextInstance>, ReconcileError> {
    let mut out = BTreeMap::new();
    scan_into(vfs, dir, &mut out)?;
    Ok(out)
}

fn scan_into(
    vfs: &impl Vfs,
    dir: &Path,
    out: &mut BTreeMap<String, TextInstance>,
) -> Result<(), ReconcileError> {
    for entry in vfs.list(dir)? {
        match entry.kind {
            EntryKind::Dir => scan_into(vfs, &entry.path, out)?,
            EntryKind::File => {
                let Some(name) = entry.path.file_name().and_then(|os_name| os_name.to_str()) else {
                    continue;
                };
                let Some(class) = init_class(name)
                    .or_else(|| script_class(name).map(|(found_class, _)| found_class))
                else {
                    continue;
                };
                let key = entry.path.to_string_lossy().into_owned();
                let content = decode(&key, &vfs.read(&entry.path)?)?;
                out.insert(key.clone(), TextInstance::new(key, class, content));
            }
        }
    }
    Ok(())
}

pub(super) fn decode(path: &str, bytes: &[u8]) -> Result<String, ReconcileError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| ReconcileError::NonUtf8(PathBuf::from(path)))
}
