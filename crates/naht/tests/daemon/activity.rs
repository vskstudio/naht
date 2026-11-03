use naht_core::protocol::{Change, EventKind};
use naht_core::reconciler::Direction;

use super::support::Harness;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activity_reports_a_push_event_after_ack() {
    let harness = Harness::start(None).await;
    harness.write("Greeter.luau", "return 1");
    harness.info(None).await;
    let batch = harness.patches(0).await;
    harness.ack_all(&batch).await;

    let act = harness.activity(0).await;
    assert_eq!(act.events.len(), 1);
    assert_eq!(act.events[0].direction, Direction::ToStudio);
    assert_eq!(act.events[0].kind, EventKind::Text);
    assert_eq!(act.events[0].path, "Greeter.luau");
    assert_eq!(act.pending, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activity_reports_a_pull_event_after_a_studio_change() {
    let harness = Harness::start(None).await;
    harness
        .post_changes(vec![Change::Upsert {
            path: "New.luau".to_owned(),
            class: "ModuleScript".to_owned(),
            content: "return 5".to_owned(),
        }])
        .await;

    let act = harness.activity(0).await;
    let pulls: Vec<_> = act
        .events
        .iter()
        .filter(|event| event.direction == Direction::ToFs)
        .collect();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].path, "New.luau");
    assert_eq!(pulls[0].kind, EventKind::Text);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activity_since_filters_seen_events() {
    let harness = Harness::start(None).await;
    harness.write("A.luau", "return 1");
    harness.info(None).await;
    let batch = harness.patches(0).await;
    harness.ack_all(&batch).await;

    let first = harness.activity(0).await;
    assert_eq!(first.events.len(), 1);
    let seq = first.events[0].seq;

    let none = harness.activity(seq).await;
    assert!(none.events.is_empty());
}
