//! The wire protocol exchanged with the Studio plugin (architecture §7).
//!
//! Bodies are encoded as **MessagePack** — binary, compact, and decodable by a hand-written Luau
//! library inside Studio. The types are plain `serde` structs so the encoding stays a swappable
//! layer: nothing above this module names MessagePack. Field-named encoding (`to_vec_named`) keeps
//! the wire self-describing, so the Luau side decodes by key rather than by positional offset.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::reconciler::Patch;

/// The protocol version. Bumped on any breaking change to the message shapes; the handshake carries
/// it so an incompatible plugin/daemon pair fails loudly instead of misreading each other.
pub const PROTOCOL_VERSION: u32 = 1;

/// The handshake payload returned by `GET /info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// The wire protocol version this daemon speaks.
    pub protocol_version: u32,
    /// The `naht-core` version, surfaced for diagnostics.
    pub server_version: String,
    /// The project's name, shown in the plugin's connection UI.
    pub project_name: String,
    /// A per-run session id; a change in it tells the plugin the daemon restarted.
    pub session_id: String,
    /// The place id this project is bound to, or `None` if unguarded. The plugin reports its own
    /// place id at handshake; a mismatch is rejected by the daemon before any sync.
    pub serve_place_id: Option<u64>,
}

/// The long-poll response from `GET /patches`: every queued patch past the client's cursor, plus the
/// new cursor to send next time.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PatchBatch {
    /// The sequence number to pass as the next request's cursor.
    pub cursor: u64,
    /// Patches flowing filesystem → Studio, in order.
    pub patches: Vec<Patch>,
}

/// One Studio-side change pushed to `POST /changes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    /// An instance was created or its source edited.
    Upsert {
        /// The instance's project-relative path (its stable key).
        path: String,
        /// The Roblox class name.
        class: String,
        /// The new text content.
        content: String,
    },
    /// An instance was removed in Studio.
    Delete {
        /// The instance's project-relative path.
        path: String,
    },
}

/// The body of `POST /changes`: a batch of Studio-side edits.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChangeBatch {
    /// The edits to apply, in order.
    pub changes: Vec<Change>,
}

/// The liveness reply from the heartbeat endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    /// The current session id, so a heartbeat also detects a restart.
    pub session_id: String,
}

/// Errors from encoding or decoding a protocol message.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// The value could not be encoded to MessagePack.
    #[error("protocol encode failed: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    /// The bytes could not be decoded from MessagePack.
    #[error("protocol decode failed: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

/// Encode a protocol message to MessagePack, with field names preserved.
pub fn to_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    Ok(rmp_serde::to_vec_named(value)?)
}

/// Decode a protocol message from MessagePack.
pub fn from_msgpack<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    Ok(rmp_serde::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconciler::{Direction, PatchKind};

    #[test]
    fn server_info_round_trips_through_msgpack() {
        let info = ServerInfo {
            protocol_version: PROTOCOL_VERSION,
            server_version: "0.1.0".to_string(),
            project_name: "demo".to_string(),
            session_id: "s-1".to_string(),
            serve_place_id: Some(123),
        };
        let bytes = to_msgpack(&info).unwrap();
        assert_eq!(from_msgpack::<ServerInfo>(&bytes).unwrap(), info);
    }

    #[test]
    fn patch_batch_round_trips_through_msgpack() {
        let batch = PatchBatch {
            cursor: 7,
            patches: vec![Patch {
                path: "src/A.luau".to_string(),
                class: "ModuleScript".to_string(),
                direction: Direction::ToStudio,
                kind: PatchKind::Update,
                content: Some("return 2".to_string()),
            }],
        };
        let bytes = to_msgpack(&batch).unwrap();
        assert_eq!(from_msgpack::<PatchBatch>(&bytes).unwrap(), batch);
    }

    #[test]
    fn change_batch_round_trips_both_variants() {
        let batch = ChangeBatch {
            changes: vec![
                Change::Upsert {
                    path: "src/A.luau".to_string(),
                    class: "ModuleScript".to_string(),
                    content: "return 1".to_string(),
                },
                Change::Delete {
                    path: "src/B.luau".to_string(),
                },
            ],
        };
        let bytes = to_msgpack(&batch).unwrap();
        assert_eq!(from_msgpack::<ChangeBatch>(&bytes).unwrap(), batch);
    }
}
