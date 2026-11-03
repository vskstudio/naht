//! Activity buffer + pending count, driven directly against `Session`.

use naht::session::Session;
use naht_core::protocol::{Change, EventKind, Outcome};
use naht_core::reconciler::Direction;
use naht_core::state::StateStore;
use std::fs;
use std::path::Path;
use std::slice;

fn session_at(root: &Path) -> Session {
    fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None, true)
}

#[test]
fn ack_records_a_push_event_and_clears_pending() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = session_at(dir.path());
    fs::write(dir.path().join("src").join("A.luau"), "return 1").unwrap();
    session.rescan().unwrap();

    let before = session.activity(0);
    assert_eq!(before.pending, 1);
    assert!(before.events.is_empty());

    let (_, patches) = session.take_patches(0);
    let path = patches[0].path.clone();
    session.ack(slice::from_ref(&path)).unwrap();

    let after = session.activity(0);
    assert_eq!(after.pending, 0);
    assert_eq!(after.events.len(), 1);
    let event = &after.events[0];
    assert_eq!(event.path, path);
    assert_eq!(event.direction, Direction::ToStudio);
    assert_eq!(event.kind, EventKind::Text);
    assert_eq!(event.outcome, Outcome::Ok);
    assert_eq!(event.seq, 1);
}

#[test]
fn applying_a_studio_change_records_a_pull_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = session_at(dir.path());
    session
        .apply_changes(vec![Change::Upsert {
            path: "src/New.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .unwrap();

    let act = session.activity(0);
    let pulls: Vec<_> = act
        .events
        .iter()
        .filter(|event| event.direction == Direction::ToFs)
        .collect();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].kind, EventKind::Text);
    assert_eq!(pulls[0].outcome, Outcome::Ok);
}

#[test]
fn since_returns_only_newer_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = session_at(dir.path());
    for index in 0..3 {
        session
            .apply_changes(vec![Change::Upsert {
                path: format!("src/F{index}.luau"),
                class: "ModuleScript".to_owned(),
                content: format!("return {index}"),
            }])
            .unwrap();
    }
    let all = session.activity(0);
    assert!(all.events.len() >= 3);
    let mid = all.events[0].seq;
    let rest = session.activity(mid);
    assert!(rest.events.iter().all(|event| event.seq > mid));
}

#[test]
fn buffer_evicts_oldest_past_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = session_at(dir.path());
    for index in 0..260 {
        session
            .apply_changes(vec![Change::Upsert {
                path: format!("src/E{index}.luau"),
                class: "ModuleScript".to_owned(),
                content: "x".to_owned(),
            }])
            .unwrap();
    }
    let act = session.activity(0);
    assert_eq!(act.events.len(), 256);
    assert_eq!(act.buffer_start_seq, act.events[0].seq);
    assert_eq!(act.buffer_start_seq, 5);
}
