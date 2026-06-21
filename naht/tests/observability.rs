//! Stage 8 smoke test: a sync that emits a patch logs exactly one structured `info` "patch" event.
//!
//! Captures events with a minimal layer (no env filter) so the assertion is deterministic and not
//! at the mercy of an ambient log level.

use std::sync::{Arc, Mutex};

use naht::session::Session;
use naht_core::state::StateStore;
use tracing::field::{Field, Visit};
use tracing::Level;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

/// A layer that records each event's message and level.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<(Level, String)>>>);

struct MessageVisitor(Option<String>);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor(None);
        event.record(&mut visitor);
        if let Some(message) = visitor.0 {
            self.0
                .lock()
                .expect("capture lock")
                .push((*event.metadata().level(), message));
        }
    }
}

#[test]
fn a_sync_emits_exactly_one_info_patch_event() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("A.luau"), "return 1").unwrap();

    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let mut session = Session::new(root, store, "demo", None);

    let capture = Capture::default();
    let events = capture.0.clone();
    let subscriber = tracing_subscriber::registry().with(capture);
    tracing::subscriber::with_default(subscriber, || {
        session.rescan().unwrap();
    });

    let patch_events: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, message)| message.contains("patch emitted"))
        .cloned()
        .collect();
    assert_eq!(patch_events.len(), 1, "expected exactly one patch event");
    assert_eq!(
        patch_events[0].0,
        Level::INFO,
        "patch event must be at info"
    );
}
