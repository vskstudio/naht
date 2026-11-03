use std::collections::BTreeMap;
use std::path::Path;

use crate::reconciler::ReconcileError;
use crate::vfs::{EntryKind, Vfs};

use super::model::{BLOB_EXTENSION, BlobInstance, TERRAIN_CLASS};

/// Walk `dir` in `vfs` and collect every binary blob file (`*.terrain`) into the flat, path-keyed
/// map the blob reconciler consumes: the binary counterpart of [`crate::reconciler::scan_text`].
/// Non-blob files are skipped, not lost; the text scan picks them up instead.
pub fn scan_blobs(
    vfs: &impl Vfs,
    dir: &Path,
) -> Result<BTreeMap<String, BlobInstance>, ReconcileError> {
    let mut out = BTreeMap::new();
    scan_blobs_into(vfs, dir, &mut out)?;
    Ok(out)
}

fn scan_blobs_into(
    vfs: &impl Vfs,
    dir: &Path,
    out: &mut BTreeMap<String, BlobInstance>,
) -> Result<(), ReconcileError> {
    for entry in vfs.list(dir)? {
        match entry.kind {
            EntryKind::Dir => scan_blobs_into(vfs, &entry.path, out)?,
            EntryKind::File => {
                let is_blob = entry
                    .path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == BLOB_EXTENSION);
                if !is_blob {
                    continue;
                }
                let key = entry.path.to_string_lossy().into_owned();
                let content = vfs.read(&entry.path)?;
                out.insert(key.clone(), BlobInstance::new(key, TERRAIN_CLASS, content));
            }
        }
    }
    Ok(())
}
