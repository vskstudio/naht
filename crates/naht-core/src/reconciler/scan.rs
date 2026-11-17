use std::path::{Path, PathBuf};

use crate::mapper::{init_class, script_class};
use crate::vfs::{EntryKind, Vfs};
use crate::walk::{self, Scan};

use super::model::{ReconcileError, TextInstance};

/// Walk `dir` in `vfs` and collect every source file into the flat, path-keyed text-instance map
/// the reconciler consumes.
///
/// Class assignment follows the same conventions as the mapper (`*.server.luau` → `Script`,
/// `*.client.luau` → `LocalScript`, `*.luau`/`init.luau` → `ModuleScript`); non-source files are
/// skipped, not lost. Dot entries and dependency directories are not walked. A file that cannot be
/// read or decoded is recorded in [`Scan::skipped`] and the rest of the tree still scans. The daemon
/// calls this each reconcile so the filesystem side is always read fresh from disk rather than cached.
///
/// # Errors
///
/// Returns an error only when `dir` itself cannot be listed.
pub fn scan_text(vfs: &impl Vfs, dir: &Path) -> Result<Scan<TextInstance>, ReconcileError> {
    let mut scan = Scan::default();
    scan_into(vfs, dir, &mut scan)?;
    Ok(scan)
}

fn scan_into(
    vfs: &impl Vfs,
    dir: &Path,
    scan: &mut Scan<TextInstance>,
) -> Result<(), ReconcileError> {
    for entry in vfs.list(dir)? {
        if walk::is_ignored_entry(&entry.path) {
            continue;
        }
        let outcome = match entry.kind {
            EntryKind::Dir => scan_into(vfs, &entry.path, scan),
            EntryKind::File => collect_file(vfs, &entry.path, scan),
        };
        if let Err(error) = outcome {
            scan.skip(entry.path, error);
        }
    }
    Ok(())
}

fn collect_file(
    vfs: &impl Vfs,
    path: &Path,
    scan: &mut Scan<TextInstance>,
) -> Result<(), ReconcileError> {
    let Some(name) = path.file_name().and_then(|os_name| os_name.to_str()) else {
        return Ok(());
    };
    let Some(class) =
        init_class(name).or_else(|| script_class(name).map(|(found_class, _)| found_class))
    else {
        return Ok(());
    };
    let key = path.to_string_lossy().into_owned();
    let content = decode(&key, &vfs.read(path)?)?;
    scan.instances
        .insert(key.clone(), TextInstance::new(key, class, content));
    Ok(())
}

pub(super) fn decode(path: &str, bytes: &[u8]) -> Result<String, ReconcileError> {
    String::from_utf8(bytes.to_vec()).map_err(|source| ReconcileError::NonUtf8 {
        path: PathBuf::from(path),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::vfs::DiskVfs;

    use super::scan_text;

    fn key(dir: &Path, relative: &str) -> String {
        dir.join(relative).to_string_lossy().into_owned()
    }

    fn write(dir: &Path, relative: &str, contents: &[u8]) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn a_file_that_is_not_utf8_is_skipped_and_reported_while_the_rest_scan() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Main.luau", b"return 1");
        write(dir.path(), "Broken.luau", &[0xff, 0xfe, 0x00]);

        let scan = scan_text(&DiskVfs::new(), dir.path()).unwrap();

        assert!(scan.instances.contains_key(&key(dir.path(), "Main.luau")));
        assert!(!scan.instances.contains_key(&key(dir.path(), "Broken.luau")));
        assert_eq!(scan.skipped.len(), 1);
        assert_eq!(scan.skipped[0].path, dir.path().join("Broken.luau"));
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_skipped_and_reported_while_the_rest_scan() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Main.luau", b"return 1");
        symlink(dir.path().join("missing"), dir.path().join("Gone.luau")).unwrap();

        let scan = scan_text(&DiskVfs::new(), dir.path()).unwrap();

        assert_eq!(scan.instances.len(), 1);
        assert_eq!(scan.skipped.len(), 1);
        assert_eq!(scan.skipped[0].path, dir.path().join("Gone.luau"));
    }

    #[test]
    fn dot_entries_and_dependency_directories_are_not_walked() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Main.luau", b"return 1");
        write(dir.path(), ".hidden.luau", b"return 2");
        write(dir.path(), ".git/hooks/pre.luau", b"return 3");
        write(dir.path(), "node_modules/pkg/index.luau", b"return 4");
        write(dir.path(), "target/debug/build.luau", b"return 5");
        write(dir.path(), "Pkg/.cache/x.luau", b"return 6");

        let scan = scan_text(&DiskVfs::new(), dir.path()).unwrap();

        let keys: Vec<&String> = scan.instances.keys().collect();
        assert_eq!(keys, vec![&key(dir.path(), "Main.luau")]);
        assert!(scan.skipped.is_empty());
    }
}
