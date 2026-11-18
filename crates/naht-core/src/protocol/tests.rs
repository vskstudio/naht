use super::*;
use crate::reconciler::PatchKind;
use serde::de::IgnoredAny;
use std::iter::repeat_n;

fn nested_map(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x81, 0xa1, b'x'];
    bytes.extend(repeat_n(0x91, depth));
    bytes.push(0x00);
    bytes
}

fn nested_arrays(containers: usize) -> Vec<u8> {
    let mut bytes = vec![0x91; containers];
    bytes.push(0x00);
    bytes
}

#[test]
fn a_message_nested_past_the_depth_limit_is_refused() {
    let error = from_msgpack::<ServerInfo>(&nested_map(1000)).unwrap_err();
    assert!(error.to_string().contains("depth limit"), "{error}");
}

#[test]
fn a_message_nested_to_the_limit_decodes_and_one_container_deeper_is_refused() {
    let limit = MAX_MSGPACK_NESTING;
    from_msgpack::<IgnoredAny>(&nested_arrays(limit)).unwrap();
    let error = from_msgpack::<IgnoredAny>(&nested_arrays(limit.saturating_add(1))).unwrap_err();
    assert!(error.to_string().contains("depth limit"), "{error}");
}

#[test]
fn server_info_round_trips_through_msgpack() {
    let info = ServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_owned(),
        project_name: "demo".to_owned(),
        project_id: "p-1".to_owned(),
        token_required: true,
        session_id: "s-1".to_owned(),
        daemon_id: "d-1".to_owned(),
        serve_place_id: Some(123),
        conflicted: vec!["Frozen.luau".to_owned()],
        terrain_sync: true,
        limits: None,
    };
    let bytes = to_msgpack(&info).unwrap();
    assert_eq!(from_msgpack::<ServerInfo>(&bytes).unwrap(), info);
}

#[derive(Serialize)]
struct PreConflictServerInfo {
    protocol_version: u32,
    server_version: String,
    project_name: String,
    session_id: String,
    daemon_id: String,
    serve_place_id: Option<u64>,
}

#[test]
fn server_info_without_conflicted_decodes_with_none_frozen() {
    let legacy = PreConflictServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_owned(),
        project_name: "demo".to_owned(),
        session_id: "s-1".to_owned(),
        daemon_id: "d-1".to_owned(),
        serve_place_id: None,
    };
    let bytes = to_msgpack(&legacy).unwrap();
    assert_eq!(
        from_msgpack::<ServerInfo>(&bytes).unwrap().conflicted,
        Vec::<String>::new()
    );
}

#[test]
fn patch_batch_round_trips_through_msgpack() {
    let batch = PatchBatch {
        cursor: 7,
        patches: vec![SequencedPatch {
            seq: 4,
            patch: Patch {
                path: "src/A.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                direction: Direction::ToStudio,
                kind: PatchKind::Update,
                content: Some("return 2".to_owned()),
            },
        }],
    };
    let bytes = to_msgpack(&batch).unwrap();
    assert_eq!(from_msgpack::<PatchBatch>(&bytes).unwrap(), batch);
}

#[test]
fn blob_patch_batch_round_trips_raw_bytes_through_msgpack() {
    use crate::binary::BlobPatch;
    use crate::reconciler::{Direction, PatchKind};

    let blob: Vec<u8> = (u8::MIN..=u8::MAX).cycle().take(600).collect();
    let batch = BlobPatchBatch {
        cursor: 3,
        patches: vec![BlobPatch {
            path: "Terrain.terrain".to_owned(),
            class: "Terrain".to_owned(),
            direction: Direction::ToStudio,
            kind: PatchKind::Update,
            content: Some(blob.clone()),
        }],
    };
    let bytes = to_msgpack(&batch).unwrap();
    let decoded: BlobPatchBatch = from_msgpack(&bytes).unwrap();
    assert_eq!(decoded, batch);
    assert_eq!(decoded.patches[0].content.as_deref(), Some(blob.as_slice()));
}

#[test]
fn change_batch_round_trips_both_variants() {
    let batch = ChangeBatch {
        changes: vec![
            Change::Upsert {
                path: "src/A.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 1".to_owned(),
            },
            Change::Delete {
                path: "src/B.luau".to_owned(),
            },
        ],
    };
    let bytes = to_msgpack(&batch).unwrap();
    assert_eq!(from_msgpack::<ChangeBatch>(&bytes).unwrap(), batch);
}

const PLUGIN_SUITE: &str = include_str!("../../../../plugin/tests/run.luau");

fn pinned_hex(name: &str) -> String {
    let prefix = format!("{name} = \"");
    let pins: Vec<&str> = PLUGIN_SUITE
        .lines()
        .filter_map(|line| {
            line.trim_start()
                .trim_start_matches("--")
                .trim_start()
                .strip_prefix(prefix.as_str())
        })
        .collect();
    let [pin] = pins.as_slice() else {
        panic!(
            "run.luau must pin `{name}` exactly once, found {}",
            pins.len()
        );
    };
    pin.split('"')
        .next()
        .expect("a pinned hex string")
        .to_owned()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .concat()
}

#[test]
fn server_info_encodes_to_the_bytes_the_plugin_suite_pins() {
    let info = ServerInfo {
        protocol_version: PROTOCOL_VERSION,
        server_version: "0.1.0".to_owned(),
        project_name: "demo".to_owned(),
        project_id: "p-1".to_owned(),
        token_required: true,
        session_id: "abc".to_owned(),
        daemon_id: "d-1".to_owned(),
        serve_place_id: Some(123),
        conflicted: Vec::new(),
        terrain_sync: false,
        limits: None,
    };
    assert_eq!(hex(&to_msgpack(&info).unwrap()), pinned_hex("server_info"));
}

#[test]
fn patch_batch_encodes_to_the_bytes_the_plugin_suite_pins() {
    let batch = PatchBatch {
        cursor: 2,
        patches: vec![SequencedPatch {
            seq: 1,
            patch: Patch {
                path: "src/A.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                direction: Direction::ToStudio,
                kind: PatchKind::Update,
                content: Some("return 1".to_owned()),
            },
        }],
    };
    assert_eq!(hex(&to_msgpack(&batch).unwrap()), pinned_hex("patch_batch"));
}

#[test]
fn change_batch_encodes_to_the_bytes_the_plugin_suite_pins() {
    let batch = ChangeBatch {
        changes: vec![
            Change::Upsert {
                path: "src/A.luau".to_owned(),
                class: "ModuleScript".to_owned(),
                content: "return 2".to_owned(),
            },
            Change::Delete {
                path: "src/B.luau".to_owned(),
            },
        ],
    };
    assert_eq!(
        hex(&to_msgpack(&batch).unwrap()),
        pinned_hex("change_batch")
    );
}

#[test]
fn blob_patch_batch_encodes_to_the_bytes_the_plugin_suite_pins() {
    let batch = BlobPatchBatch {
        cursor: 1,
        patches: vec![BlobPatch {
            path: "Terrain.terrain".to_owned(),
            class: "Terrain".to_owned(),
            direction: Direction::ToStudio,
            kind: PatchKind::Update,
            content: Some(vec![0x00, 0x01, 0xff, 0x80, 0x7f]),
        }],
    };
    assert_eq!(
        hex(&to_msgpack(&batch).unwrap()),
        pinned_hex("blob_patch_batch")
    );
}

#[test]
fn sync_activity_round_trips_through_msgpack() {
    let activity = SyncActivity {
        pending: 2,
        buffer_start_seq: 5,
        events: vec![SyncEvent {
            seq: 5,
            unix_ms: 1_700_000_000_000,
            path: "src/Foo.luau".to_owned(),
            direction: Direction::ToStudio,
            kind: EventKind::Text,
            outcome: Outcome::Ok,
        }],
    };
    let bytes = to_msgpack(&activity).unwrap();
    let decoded: SyncActivity = from_msgpack(&bytes).unwrap();
    assert_eq!(activity, decoded);
}
