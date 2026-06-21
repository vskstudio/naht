//! Print golden MessagePack encodings of representative protocol messages, as hex.
//!
//! These bytes are exactly what the daemon puts on the wire (`rmp_serde::to_vec_named`). The Luau
//! plugin's `MessagePack` harness embeds them to cross-check its codec against the real format. Run
//! with `cargo run -p naht-core --example wire_golden`.

use naht_core::protocol::{self, Change, ChangeBatch, PatchBatch, ServerInfo, PROTOCOL_VERSION};
use naht_core::reconciler::{Direction, Patch, PatchKind};

fn hex(label: &str, bytes: &[u8]) {
    let encoded: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    println!("{label} {encoded}");
}

fn main() {
    let info = ServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_string(),
        project_name: "demo".to_string(),
        session_id: "abc".to_string(),
        serve_place_id: Some(123),
    };
    hex("server_info", &protocol::to_msgpack(&info).unwrap());

    let patches = PatchBatch {
        cursor: 2,
        patches: vec![Patch {
            path: "src/A.luau".to_string(),
            class: "ModuleScript".to_string(),
            direction: Direction::ToStudio,
            kind: PatchKind::Update,
            content: Some("return 1".to_string()),
        }],
    };
    hex("patch_batch", &protocol::to_msgpack(&patches).unwrap());

    let changes = ChangeBatch {
        changes: vec![
            Change::Upsert {
                path: "src/A.luau".to_string(),
                class: "ModuleScript".to_string(),
                content: "return 2".to_string(),
            },
            Change::Delete {
                path: "src/B.luau".to_string(),
            },
        ],
    };
    hex("change_batch", &protocol::to_msgpack(&changes).unwrap());
}
