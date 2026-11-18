use std::path::Path;

use crate::protocol::TERRAIN_PATH;
use crate::reconciler::ReconcileError;
use crate::vfs::{EntryKind, Vfs};
use crate::walk::Scan;

use super::model::{BlobInstance, TERRAIN_CLASS};

/// Collect the synced terrain blob, `Terrain.terrain` directly under `dir`, into a path-keyed map.
///
/// Studio holds one terrain, so the blob channel syncs that single file and ignores every other
/// `*.terrain` file in the project. Non-blob files are left to the text scan. A blob that cannot be
/// read is recorded in [`Scan::skipped`].
///
/// # Errors
///
/// Returns an error only when `dir` itself cannot be listed.
pub fn scan_blobs(vfs: &impl Vfs, dir: &Path) -> Result<Scan<BlobInstance>, ReconcileError> {
    let terrain = dir.join(TERRAIN_PATH);
    let mut scan = Scan::default();
    for entry in vfs.list(dir)? {
        if entry.kind == EntryKind::File && entry.path == terrain {
            collect_blob(vfs, &entry.path, &mut scan);
        }
    }
    Ok(scan)
}

fn collect_blob(vfs: &impl Vfs, path: &Path, scan: &mut Scan<BlobInstance>) {
    let key = path.to_string_lossy().into_owned();
    match vfs.read(path) {
        Ok(content) => {
            scan.instances
                .insert(key.clone(), BlobInstance::new(key, TERRAIN_CLASS, content));
        }
        Err(error) => scan.skip(path.to_path_buf(), error.into()),
    }
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
    fn a_dangling_terrain_symlink_is_skipped_and_reported() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        symlink(
            dir.path().join("missing"),
            dir.path().join("Terrain.terrain"),
        )
        .unwrap();

        let scan = scan_blobs(&DiskVfs::new(), dir.path()).unwrap();

        assert!(scan.instances.is_empty());
        assert_eq!(scan.skipped.len(), 1);
        assert_eq!(scan.skipped[0].path, dir.path().join("Terrain.terrain"));
    }

    #[test]
    fn terrain_files_outside_the_root_are_not_collected() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Terrain.terrain", &[1]);
        write(dir.path(), "src/Terrain.terrain", &[2]);
        write(dir.path(), "src/Map.terrain", &[3]);
        write(dir.path(), ".git/Terrain.terrain", &[4]);
        write(dir.path(), "target/Terrain.terrain", &[5]);

        let scan = scan_blobs(&DiskVfs::new(), dir.path()).unwrap();

        let keys: Vec<&String> = scan.instances.keys().collect();
        assert_eq!(keys, vec![&key(dir.path(), "Terrain.terrain")]);
    }
}
