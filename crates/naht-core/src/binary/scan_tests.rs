use std::path::Path;

use super::tests::blobs;
use super::*;
use crate::state::StateStore;
use crate::vfs::{MemoryVfs, Vfs};

#[test]
fn a_terrain_file_in_another_case_is_left_alone_when_studio_reports_terrain() {
    let mut vfs = MemoryVfs::new().with_file("terrain.terrain", vec![5]);
    let store = StateStore::open_in_memory().unwrap();
    let scan = scan_blobs(&vfs, Path::new("")).unwrap();
    let studio = blobs(&[("Terrain.terrain", &[7])]);

    let patches = reconcile_blobs(
        &mut vfs,
        &store,
        &scan.instances,
        Some(&studio),
        &scan.unseen(),
    )
    .unwrap();

    assert!(
        patches.is_empty(),
        "a case variant was overwritten: {patches:?}"
    );
    assert_eq!(vfs.read(Path::new("terrain.terrain")).unwrap(), vec![5]);
    vfs.read(Path::new("Terrain.terrain")).unwrap_err();
    assert!(scan.unseen().contains("Terrain.terrain"));
}
