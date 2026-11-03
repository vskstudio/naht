//! Print golden MessagePack encodings of representative protocol messages, as hex.
//!
//! These bytes are exactly what the daemon puts on the wire (`rmp_serde::to_vec_named`). The Luau
//! plugin's `MessagePack` harness embeds them to cross-check its codec against the real format. Run
//! with `cargo run -p naht-core --example wire_golden`.

use std::io::{self, Write};

use naht_core::binary::BlobPatch;
use naht_core::protocol::{
    self, BlobPatchBatch, Change, ChangeBatch, PROTOCOL_VERSION, PatchBatch, ProtocolError,
    ServerInfo,
};
use naht_core::reconciler::{Direction, Patch, PatchKind};

#[derive(Debug, thiserror::Error)]
enum GoldenError {
    #[error("writing golden output: {0}")]
    Io(#[from] io::Error),
    #[error("encoding a golden message: {0}")]
    Encode(#[from] ProtocolError),
}

fn hex(out: &mut impl Write, label: &str, bytes: &[u8]) -> io::Result<()> {
    let encoded: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    writeln!(out, "{label} {encoded}")
}

fn main() -> Result<(), GoldenError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let info = ServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_string(),
        project_name: "demo".to_string(),
        session_id: "abc".to_string(),
        serve_place_id: Some(123),
    };
    hex(&mut out, "server_info", &protocol::to_msgpack(&info)?)?;

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
    hex(&mut out, "patch_batch", &protocol::to_msgpack(&patches)?)?;

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
    hex(&mut out, "change_batch", &protocol::to_msgpack(&changes)?)?;

    let blob_batch = BlobPatchBatch {
        cursor: 1,
        patches: vec![BlobPatch {
            path: "Terrain.terrain".to_string(),
            class: "Terrain".to_string(),
            direction: Direction::ToStudio,
            kind: PatchKind::Update,
            content: Some(vec![0x00, 0x01, 0xff, 0x80, 0x7f]),
        }],
    };
    hex(
        &mut out,
        "blob_patch_batch",
        &protocol::to_msgpack(&blob_batch)?,
    )?;
    Ok(())
}
