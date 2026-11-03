//! Activity buffer + pending count, driven directly against `Session`.

use naht::session::Session;
use naht_core::protocol::{Change, EventKind, Outcome};
use naht_core::reconciler::Direction;
use naht_core::state::StateStore;

fn session_at(root: &std::path::Path) -> Session {
    std::fs::create_dir_all(root.join("src")).unwrap();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    Session::new(root, store, "demo", None, true)
}

#[test]
fn ack_records_a_push_event_and_clears_pending() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = session_at(dir.path());
    std::fs::write(dir.path().join("src").join("A.luau"), "return 1").unwrap();
    s.rescan().unwrap();

    // The new file is a queued, unacked FS→Studio patch: pending, no completed event yet.
    let before = s.activity(0);
    assert_eq!(before.pending, 1);
    assert!(before.events.is_empty());

    // Ack the exact path the reconcile queued (don't hardcode the mapper's key).
    let (_, patches) = s.take_patches(0);
    let path = patches[0].path.clone();
    s.ack(std::slice::from_ref(&path)).unwrap();

    let after = s.activity(0);
    assert_eq!(after.pending, 0);
    assert_eq!(after.events.len(), 1);
    let e = &after.events[0];
    assert_eq!(e.path, path);
    assert_eq!(e.direction, Direction::ToStudio);
    assert_eq!(e.kind, EventKind::Text);
    assert_eq!(e.outcome, Outcome::Ok);
    assert_eq!(e.seq, 1); // seqs are 1-based (0 is the "from start" cursor sentinel)
}

#[test]
fn applying_a_studio_change_records_a_pull_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = session_at(dir.path());
    s.apply_changes(vec![Change::Upsert {
        path: "src/New.luau".to_string(),
        class: "ModuleScript".to_string(),
        content: "return 5".to_string(),
    }])
    .unwrap();

    let act = s.activity(0);
    let pulls: Vec<_> = act
        .events
        .iter()
        .filter(|e| e.direction == Direction::ToFs)
        .collect();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].kind, EventKind::Text);
    assert_eq!(pulls[0].outcome, Outcome::Ok);
}

#[test]
fn since_returns_only_newer_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = session_at(dir.path());
    for i in 0..3 {
        s.apply_changes(vec![Change::Upsert {
            path: format!("src/F{i}.luau"),
            class: "ModuleScript".to_string(),
            content: format!("return {i}"),
        }])
        .unwrap();
    }
    let all = s.activity(0);
    assert!(all.events.len() >= 3);
    let mid = all.events[0].seq;
    let rest = s.activity(mid);
    assert!(rest.events.iter().all(|e| e.seq > mid));
}

#[test]
fn buffer_evicts_oldest_past_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = session_at(dir.path());
    for i in 0..260 {
        s.apply_changes(vec![Change::Upsert {
            path: format!("src/E{i}.luau"),
            class: "ModuleScript".to_string(),
            content: "x".to_string(),
        }])
        .unwrap();
    }
    let act = s.activity(0);
    assert_eq!(act.events.len(), 256);
    assert_eq!(act.buffer_start_seq, act.events[0].seq);
    assert_eq!(act.buffer_start_seq, 5); // 260 events, seqs 1..=260 → oldest 4 evicted, front = 5
}
