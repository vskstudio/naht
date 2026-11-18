use std::fs;

use naht_core::protocol::CLIENT_HEADER;
use naht_core::reconciler::{Direction, PatchKind};

use super::{Daemon, FILE, init};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_edit_made_while_no_daemon_ran_reaches_the_plugin_on_a_tokenless_first_attach() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("demo");
    init(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(FILE), "return 1").unwrap();

    let first = Daemon::start(&root).await;
    first.handshake().await;
    let synced = first.pending_patches().await.sequenced();
    let seqs: Vec<u64> = synced.iter().map(|patch| patch.seq).collect();
    first.ack(seqs).await;
    drop(first);

    fs::write(root.join(FILE), "return 2").unwrap();

    let second = Daemon::start(&root).await;
    let tokenless = second
        .client
        .get(format!("{}/info", second.base))
        .header(CLIENT_HEADER, "plugin")
        .send()
        .await
        .unwrap();
    assert!(tokenless.status().is_success(), "{}", tokenless.status());

    let patches = second.pending_patches().await.patches();
    assert_eq!(patches.len(), 1, "expected the offline edit: {patches:?}");
    assert_eq!(patches[0].path, FILE);
    assert_eq!(patches[0].direction, Direction::ToStudio);
    assert_ne!(patches[0].kind, PatchKind::Delete);
    assert_eq!(patches[0].content.as_deref(), Some("return 2"));
}
