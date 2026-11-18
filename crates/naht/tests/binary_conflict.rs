//! A frozen binary conflict cleared with `naht resolve --keep`, seen through a live session.

#![cfg(test)]

use std::fs;
use std::path::Path;

use naht::commands;
use naht::session::Session;
use naht_core::binary::{BlobInstance, Keep};
use naht_core::reconciler::Direction;
use naht_core::state::StateStore;

const BLOB: &str = "Terrain.terrain";
const DISK: [u8; 3] = [4, 5, 6];
const STUDIO: [u8; 3] = [7, 8, 9];

fn open(root: &Path) -> Session {
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None, true).unwrap()
}

fn terrain(content: &[u8]) -> Vec<BlobInstance> {
    vec![BlobInstance::new(BLOB, "Terrain", content.to_vec())]
}

fn frozen(root: &Path) -> Session {
    fs::write(root.join(BLOB), [1_u8, 2, 3]).unwrap();
    let mut session = open(root);
    session.apply_blob_changes(terrain(&[1, 2, 3])).unwrap();
    fs::write(root.join(BLOB), DISK).unwrap();
    session.apply_blob_changes(terrain(&STUDIO)).unwrap();
    session
}

fn to(direction: Direction, session: &mut Session) -> Vec<Vec<u8>> {
    let (_, patches) = session.take_blob_patches(0);
    patches
        .into_iter()
        .filter(|patch| patch.direction == direction)
        .filter_map(|patch| patch.content)
        .collect()
}

#[test]
fn keeping_the_disk_side_sends_the_disk_bytes_to_studio() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = frozen(root);
    assert_eq!(to(Direction::ToStudio, &mut session), Vec::<Vec<u8>>::new());

    commands::resolve(root, BLOB, Some(Keep::Disk)).unwrap();
    session.rescan().unwrap();

    assert_eq!(to(Direction::ToStudio, &mut session), vec![DISK.to_vec()]);
    assert_eq!(fs::read(root.join(BLOB)).unwrap(), DISK);
}

#[test]
fn keeping_the_studio_side_writes_the_studio_bytes_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut session = frozen(root);

    commands::resolve(root, BLOB, Some(Keep::Studio)).unwrap();
    session.rescan().unwrap();

    assert_eq!(to(Direction::ToStudio, &mut session), Vec::<Vec<u8>>::new());
    assert_eq!(fs::read(root.join(BLOB)).unwrap(), STUDIO);
}

#[test]
fn a_binary_conflict_needs_a_side_and_a_text_file_refuses_one() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _session = frozen(root);

    assert!(commands::resolve(root, BLOB, None).is_err());
    assert_eq!(fs::read(root.join(BLOB)).unwrap(), DISK);

    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("A.luau"), "local a = 1\n").unwrap();
    assert!(commands::resolve(root, "src/A.luau", Some(Keep::Disk)).is_err());
}
