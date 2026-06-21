//! The filesystem watcher feeds the reconciler: a file appearing on disk becomes a queued patch
//! without any HTTP request. This test is timing-based (it waits for a debounced filesystem event),
//! so it polls with a generous timeout rather than asserting on a fixed delay.

use std::net::Ipv4Addr;
use std::time::Duration;

use naht::server::AppState;
use naht::session::Session;
use naht_core::reconciler::PatchKind;
use naht_core::state::StateStore;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_file_becomes_a_queued_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let store = StateStore::open(&root.join(".naht").join("state.db")).unwrap();
    let session = Session::new(root.clone(), store, "demo", None);
    let state = AppState::new(session, Duration::from_secs(1));

    let _watcher = naht::watcher::spawn(&root, state.clone()).unwrap();

    // Bind the listener so the server is live, mirroring real use; the watcher is the unit under test.
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    tokio::spawn({
        let state = state.clone();
        async move {
            let _ = naht::server::serve(listener, state).await;
        }
    });

    std::fs::write(root.join("Watched.luau"), "return 7").unwrap();

    // Wait up to ~5s for the debounced event to reconcile into a queued patch.
    let mut found = None;
    for _ in 0..100 {
        {
            let session = state.session_lock().await;
            let (_, patches) = session.take_patches(0);
            if let Some(patch) = patches.into_iter().next() {
                found = Some(patch);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let patch = found.expect("the watcher should have queued a patch for the new file");
    assert_eq!(patch.path, "Watched.luau");
    assert_eq!(patch.kind, PatchKind::Create);
    assert_eq!(patch.content.as_deref(), Some("return 7"));
}
