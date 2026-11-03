use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use super::persist::advance_base;
use super::*;
use crate::merge;
use crate::state::StateStore;
use crate::vfs::{MemoryVfs, Vfs};

const CLASS: &str = "ModuleScript";

fn sides(items: &[(&str, &str)]) -> BTreeMap<String, TextInstance> {
    items
        .iter()
        .map(|(path, content)| (path.to_string(), TextInstance::new(*path, CLASS, *content)))
        .collect()
}

fn store_with_base(entries: &[(&str, &str)]) -> StateStore {
    let store = StateStore::open_in_memory().unwrap();
    for (path, content) in entries {
        advance_base(&store, path, CLASS, content).unwrap();
    }
    store
}

#[test]
fn one_sided_fs_change_pushes_only_to_studio_and_advances_base() {
    let mut vfs = MemoryVfs::new().with_file("m", "v1");
    let store = store_with_base(&[("m", "v1")]);
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "v2")]),
        &sides(&[("m", "v1")]),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].content.as_deref(), Some("v2"));
    assert_eq!(
        store.get("m").unwrap().unwrap().base_content,
        Some(b"v2".to_vec())
    );
}

#[test]
fn one_sided_studio_change_writes_the_file() {
    let mut vfs = MemoryVfs::new().with_file("m", "v1");
    let store = store_with_base(&[("m", "v1")]);
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "v1")]),
        &sides(&[("m", "v2")]),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToFs);
    assert_eq!(vfs.read(Path::new("m")).unwrap(), b"v2");
    assert_eq!(
        store.get("m").unwrap().unwrap().base_content,
        Some(b"v2".to_vec())
    );
}

#[test]
fn no_change_produces_no_patches() {
    let mut vfs = MemoryVfs::new().with_file("m", "v1");
    let store = store_with_base(&[("m", "v1")]);
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "v1")]),
        &sides(&[("m", "v1")]),
    )
    .unwrap();
    assert_eq!(patches.len(), 0);
}

#[test]
fn non_overlapping_both_changed_auto_merges() {
    let base = "local a = 1\nlocal b = 2\nlocal c = 3\n";
    let fs = "local a = 10\nlocal b = 2\nlocal c = 3\n";
    let studio = "local a = 1\nlocal b = 2\nlocal c = 30\n";
    let merged = "local a = 10\nlocal b = 2\nlocal c = 30\n";

    let mut vfs = MemoryVfs::new().with_file("m", base);
    let store = store_with_base(&[("m", base)]);
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", fs)]),
        &sides(&[("m", studio)]),
    )
    .unwrap();

    assert!(
        patches
            .iter()
            .any(|patch| patch.direction == Direction::ToFs)
    );
    assert!(
        patches
            .iter()
            .any(|patch| patch.direction == Direction::ToStudio)
    );
    assert!(
        patches
            .iter()
            .all(|patch| patch.content.as_deref() == Some(merged))
    );
    assert_eq!(vfs.read(Path::new("m")).unwrap(), merged.as_bytes());
    assert_eq!(
        store.get("m").unwrap().unwrap().base_content,
        Some(merged.as_bytes().to_vec())
    );
}

#[test]
fn overlapping_both_changed_freezes_with_markers_and_loses_nothing() {
    let base = "local a = 1\n";
    let mut vfs = MemoryVfs::new().with_file("m", base);
    let store = store_with_base(&[("m", base)]);
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "local a = 11\n")]),
        &sides(&[("m", "local a = 22\n")]),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].kind, PatchKind::Conflict);

    let on_disk = String::from_utf8(vfs.read(Path::new("m")).unwrap()).unwrap();
    assert!(merge::has_conflict_markers(&on_disk));
    assert!(on_disk.contains("local a = 11"));
    assert!(on_disk.contains("local a = 22"));

    let record = store.get("m").unwrap().unwrap();
    assert!(record.conflicted);
    assert_eq!(record.base_content, Some(base.as_bytes().to_vec()));
}

#[test]
fn a_frozen_path_is_skipped_on_the_next_reconcile() {
    let base = "local a = 1\n";
    let mut vfs = MemoryVfs::new().with_file("m", base);
    let store = store_with_base(&[("m", base)]);
    reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "local a = 11\n")]),
        &sides(&[("m", "local a = 22\n")]),
    )
    .unwrap();

    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "local a = 33\n")]),
        &sides(&[("m", "local a = 44\n")]),
    )
    .unwrap();
    assert_eq!(patches.len(), 0);
    assert_eq!(status(&store).unwrap().conflicted, vec!["m".to_owned()]);
}

#[test]
fn resolve_refuses_while_markers_remain_then_succeeds() {
    let base = "local a = 1\n";
    let mut vfs = MemoryVfs::new().with_file("m", base);
    let store = store_with_base(&[("m", base)]);
    reconcile(
        &mut vfs,
        &store,
        &sides(&[("m", "local a = 11\n")]),
        &sides(&[("m", "local a = 22\n")]),
    )
    .unwrap();

    assert!(matches!(
        resolve(&vfs, &store, "m"),
        Err(ReconcileError::MarkersRemain(_))
    ));

    vfs.write(Path::new("m"), b"local a = 99\n").unwrap();
    resolve(&vfs, &store, "m").unwrap();
    assert_eq!(status(&store).unwrap().conflicted.len(), 0);
    assert_eq!(
        store.get("m").unwrap().unwrap().base_content,
        Some(b"local a = 99\n".to_vec())
    );
}

#[test]
fn new_fs_file_creates_in_studio() {
    let mut vfs = MemoryVfs::new().with_file("n", "fresh");
    let store = StateStore::open_in_memory().unwrap();
    let patches = reconcile(
        &mut vfs,
        &store,
        &sides(&[("n", "fresh")]),
        &BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Create);
    assert_eq!(
        store.get("n").unwrap().unwrap().base_content,
        Some(b"fresh".to_vec())
    );
}

#[test]
fn reconcile_scales_to_a_large_tree_without_quadratic_blowup() {
    const ITERATIONS: usize = 5_000;
    let mut vfs = MemoryVfs::new();
    let mut fs = BTreeMap::new();
    for index in 0..ITERATIONS {
        let path = format!("m{index}.luau");
        let content = format!("return {index}");
        vfs = vfs.with_file(&path, content.clone());
        fs.insert(path.clone(), TextInstance::new(path, CLASS, content));
    }
    let store = StateStore::open_in_memory().unwrap();

    let start = Instant::now();
    let patches = reconcile(&mut vfs, &store, &fs, &BTreeMap::new()).unwrap();
    let create_elapsed = start.elapsed();
    assert_eq!(patches.len(), ITERATIONS);
    assert!(
        create_elapsed < Duration::from_secs(10),
        "initial reconcile too slow: {create_elapsed:?}"
    );

    let steady_start = Instant::now();
    let steady_patches = reconcile(&mut vfs, &store, &fs, &fs).unwrap();
    assert_eq!(steady_patches.len(), 0);
    assert!(
        steady_start.elapsed() < Duration::from_secs(10),
        "steady-state reconcile too slow: {:?}",
        steady_start.elapsed()
    );
}

#[test]
fn scan_collects_source_files_as_text_instances_and_skips_the_rest() {
    let vfs = MemoryVfs::new()
        .with_file("proj/Greeter.luau", "return 1")
        .with_file("proj/run/main.server.luau", "print('s')")
        .with_file("proj/run/ui.client.luau", "print('c')")
        .with_file("proj/folder/init.luau", "return {}")
        .with_file("proj/README.md", "not an instance");

    let scanned = scan_text(&vfs, Path::new("proj")).unwrap();

    let mut got: Vec<(&str, &str, &str)> = scanned
        .values()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.class.as_str(),
                entry.content.as_str(),
            )
        })
        .collect();
    got.sort_unstable();
    assert_eq!(
        got,
        vec![
            ("proj/Greeter.luau", "ModuleScript", "return 1"),
            ("proj/folder/init.luau", "ModuleScript", "return {}"),
            ("proj/run/main.server.luau", "Script", "print('s')"),
            ("proj/run/ui.client.luau", "LocalScript", "print('c')"),
        ]
    );
}

#[test]
fn deletion_on_one_side_propagates_when_the_other_is_unchanged() {
    let mut vfs = MemoryVfs::new().with_file("m", "v1");
    let store = store_with_base(&[("m", "v1")]);
    let patches = reconcile(&mut vfs, &store, &BTreeMap::new(), &sides(&[("m", "v1")])).unwrap();

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_eq!(patches[0].kind, PatchKind::Delete);
    assert_eq!(store.get("m").unwrap(), None);
}
