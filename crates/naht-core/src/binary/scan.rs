use std::path::Path;

use crate::reconciler::ReconcileError;
use crate::vfs::{EntryKind, Vfs};
use crate::walk::{self, Scan};

use super::model::{BLOB_EXTENSION, BlobInstance, TERRAIN_CLASS};

/// Collect every binary blob file (`*.terrain`) under `dir` into a flat, path-keyed map.
///
/// This is the binary counterpart of [`crate::reconciler::scan_text`], consumed by the blob
/// reconciler. Non-blob files are skipped, not lost; the text scan picks them up instead. Dot entries
/// and dependency directories are not walked, and a blob that cannot be read is recorded in
/// [`Scan::skipped`] while the rest still scans.
///
/// # Errors
///
/// Returns an error only when `dir` itself cannot be listed.
pub fn scan_blobs(vfs: &impl Vfs, dir: &Path) -> Result<Scan<BlobInstance>, ReconcileError> {
    let mut scan = Scan::default();
    scan_blobs_into(vfs, dir, &mut scan)?;
    Ok(scan)
}

fn scan_blobs_into(
    vfs: &impl Vfs,
    dir: &Path,
    scan: &mut Scan<BlobInstance>,
) -> Result<(), ReconcileError> {
    for entry in vfs.list(dir)? {
        if walk::is_ignored_entry(&entry.path) {
            continue;
        }
        let outcome = match entry.kind {
            EntryKind::Dir => scan_blobs_into(vfs, &entry.path, scan),
            EntryKind::File => collect_blob(vfs, &entry.path, scan),
        };
        if let Err(error) = outcome {
            scan.skip(entry.path, error);
        }
    }
    Ok(())
}

fn collect_blob(
    vfs: &impl Vfs,
    path: &Path,
    scan: &mut Scan<BlobInstance>,
) -> Result<(), ReconcileError> {
    let is_blob = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == BLOB_EXTENSION);
    if !is_blob {
        return Ok(());
    }
    let key = path.to_string_lossy().into_owned();
    let content = vfs.read(path)?;
    scan.instances
        .insert(key.clone(), BlobInstance::new(key, TERRAIN_CLASS, content));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::vfs::DiskVfs;

    use super::scan_blobs;

    fn key(dir: &Path, relative: &str) -> String {
        dir.join(relative).to_string_lossy().into_owned()
    }

    fn write(dir: &Path, relative: &str, contents: &[u8]) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_blob_symlink_is_skipped_and_reported_while_the_rest_scan() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Terrain.terrain", &[1, 2, 3]);
        symlink(dir.path().join("missing"), dir.path().join("Gone.terrain")).unwrap();

        let scan = scan_blobs(&DiskVfs::new(), dir.path()).unwrap();

        assert_eq!(scan.instances.len(), 1);
        assert!(
            scan.instances
                .contains_key(&key(dir.path(), "Terrain.terrain"))
        );
        assert_eq!(scan.skipped.len(), 1);
        assert_eq!(scan.skipped[0].path, dir.path().join("Gone.terrain"));
    }

    #[test]
    fn dot_entries_and_dependency_directories_are_not_walked() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Terrain.terrain", &[1]);
        write(dir.path(), ".git/objects/blob.terrain", &[2]);
        write(dir.path(), "node_modules/pkg/cache.terrain", &[3]);
        write(dir.path(), "target/debug/out.terrain", &[4]);

        let scan = scan_blobs(&DiskVfs::new(), dir.path()).unwrap();

        let keys: Vec<&String> = scan.instances.keys().collect();
        assert_eq!(keys, vec![&key(dir.path(), "Terrain.terrain")]);
    }
}
