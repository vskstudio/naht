//! The wire protocol exchanged with the Studio plugin (architecture §7).
//!
//! Bodies are encoded as `MessagePack`: binary, compact, and decodable by a hand-written Luau
//! library inside Studio. The types are plain `serde` structs so the encoding stays a swappable
//! layer: nothing above this module names `MessagePack`. Field-named encoding (`to_vec_named`) keeps
//! the wire self-describing, so the Luau side decodes by key rather than by positional offset.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::binary::{BlobInstance, BlobPatch};
use crate::reconciler::{Direction, Patch};
use rmp_serde::decode;
use rmp_serde::encode;

/// The protocol version. Bumped on any breaking change to the message shapes; the handshake carries
/// it so an incompatible plugin/daemon pair fails loudly instead of misreading each other.
pub const PROTOCOL_VERSION: u32 = 5;

/// Request header every client sends to identify itself as the Studio plugin or the local CLI.
pub const CLIENT_HEADER: &str = "x-naht-client";

/// Request header carrying the Roblox place id the client is connected to.
pub const PLACE_ID_HEADER: &str = "x-naht-place-id";

/// Request header carrying the project's token, required on every route except `GET /info`.
pub const TOKEN_HEADER: &str = "x-naht-token";

/// Query parameter of `GET /info` carrying the plugin's nonce, which the daemon answers with a proof.
pub const NONCE_PARAM: &str = "nonce";

/// The longest nonce the daemon proves; a longer or non-alphanumeric one is refused with `400`.
pub const NONCE_MAX_LEN: usize = 128;

/// The message the daemon proof authenticates: the port the daemon serves and the plugin's nonce.
///
/// The proof is the lowercase hex HMAC-SHA256 of this message keyed by the token text. Binding the
/// port stops a process on the plugin's port from relaying the nonce to a daemon on another port.
#[must_use]
pub fn proof_message(port: u16, nonce: &str) -> String {
    format!("naht-daemon-proof-v1\n{port}\n{nonce}")
}

/// Whether `nonce` is a nonce the daemon proves: 1 to [`NONCE_MAX_LEN`] ASCII letters and digits.
#[must_use]
pub fn is_valid_nonce(nonce: &str) -> bool {
    (1..=NONCE_MAX_LEN).contains(&nonce.len())
        && nonce.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// The place id Studio reports for a place that has never been published, which no project binds to.
pub const UNPUBLISHED_PLACE_ID: u64 = 0;

/// Token opening the 403 body for a request without the client header, sent by daemons that know it.
pub const OUTDATED_PLUGIN_TOKEN: &str = "outdated-plugin";

/// The one project file the blob channel syncs: the Studio terrain, at the project root.
pub const TERRAIN_PATH: &str = "Terrain.terrain";

/// The handshake payload returned by `GET /info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    /// The wire protocol version this daemon speaks.
    pub protocol_version: u32,
    /// The `naht-core` version, surfaced for diagnostics.
    pub server_version: String,
    /// The project's name, shown in the plugin's connection UI.
    pub project_name: String,
    /// A stable id for the project, kept in the state store across daemon runs. The plugin keys its
    /// saved token by it, so a token is found again after a restart and a moved project folder.
    #[serde(default)]
    pub project_id: String,
    /// Whether every route except `GET /info` needs the `x-naht-token` header. Always `true` from
    /// protocol version 4 on.
    #[serde(default)]
    pub token_required: bool,
    /// A per-run session id; a change in it tells the plugin the daemon restarted.
    pub session_id: String,
    /// A random id per daemon run, which `.naht/daemon.port` records to identify this daemon.
    pub daemon_id: String,
    /// The place id this project is bound to, or `None` if unguarded. The plugin reports its own
    /// place id at handshake; a mismatch is rejected by the daemon before any sync.
    pub serve_place_id: Option<u64>,
    /// The paths frozen as conflicted, so a plugin that reconnects after a restart can show them.
    #[serde(default)]
    pub conflicted: Vec<String>,
    /// Whether the daemon synchronizes terrain blobs live, as `[serve] terrain_sync` says.
    #[serde(default)]
    pub terrain_sync: bool,
    /// The limits the daemon enforces that the plugin must respect; absent from daemons that predate
    /// the field, which the plugin reads as its built-in defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<ReportedLimits>,
    /// The daemon proof for the nonce of this request, present only when the request carried one.
    /// The plugin sends its token only after this matches the proof it computes with that token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<String>,
}

/// The limits a plugin must stay within, as `[limits]` configures them on the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedLimits {
    /// The most voxels one live terrain push may carry.
    pub terrain_voxels: usize,
}

/// The long-poll response from `GET /patches`: every queued patch past the client's cursor, plus the
/// new cursor to send next time.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PatchBatch {
    /// The sequence number to pass as the next request's cursor.
    pub cursor: u64,
    /// Patches flowing filesystem → Studio, in order.
    pub patches: Vec<SequencedPatch>,
}

/// A filesystem → Studio patch stamped with its queue sequence, which the plugin returns in
/// [`Ack::seqs`] once it has applied the patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequencedPatch {
    /// The queue sequence to acknowledge once the patch is applied.
    pub seq: u64,
    /// The patch itself, flattened so its fields sit beside `seq` on the wire.
    #[serde(flatten)]
    pub patch: Patch,
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

/// The long-poll response carrying filesystem → Studio **binary** patches (e.g. terrain blobs).
/// Separate from [`PatchBatch`] because blobs are synced opaquely (no diff/merge), as bin payloads.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BlobPatchBatch {
    /// The sequence number to pass as the next request's cursor.
    pub cursor: u64,
    /// Binary patches flowing filesystem → Studio, in order.
    pub patches: Vec<BlobPatch>,
}

/// The body of a Studio → filesystem **binary** change push: the blobs Studio currently holds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BlobChangeBatch {
    /// The blob instances to apply, in order.
    pub changes: Vec<BlobInstance>,
}

/// The liveness reply from the heartbeat endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    /// The current session id, so a heartbeat also detects a restart.
    pub session_id: String,
}

/// Whether a synced instance was diffable text or an opaque binary blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A text patch (source that is diffed/merged).
    Text,
    /// A binary blob (synced opaquely, e.g. terrain).
    Blob,
}

/// The outcome of a completed sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Applied cleanly.
    Ok,
    /// A conflicting change was detected; resolved without overwrite (conflict markers on disk).
    Conflict,
}

/// One completed sync operation, retained in the daemon's bounded activity buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    /// Monotonic per-session sequence number; the consumer's cursor. Never reused within a session.
    pub seq: u64,
    /// Milliseconds since the Unix epoch when the operation completed.
    pub unix_ms: u64,
    /// The instance's project-relative path (its stable key).
    pub path: String,
    /// Which way the change flowed.
    pub direction: Direction,
    /// Whether it was a text patch or an opaque binary blob.
    pub kind: EventKind,
    /// The operation's outcome.
    pub outcome: Outcome,
}

/// The response from `GET /activity`: the current pending count plus every retained event past the
/// caller's cursor. Read-only; polled by an external consumer to render live sync activity.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SyncActivity {
    /// Text patches sent to Studio but not yet acked: the live in-flight FS→Studio set.
    pub pending: u32,
    /// The oldest `seq` still retained. The caller's next expected event is `since + 1`, so it has a
    /// gap (events evicted between polls) exactly when `since + 1 < buffer_start_seq`.
    pub buffer_start_seq: u64,
    /// Events with `seq > since`, oldest-first. Empty when nothing is new.
    pub events: Vec<SyncEvent>,
}

/// The plugin's confirmation of the patches it applied, by queue sequence.
///
/// Only acked patches advance the daemon's base. A sequence the daemon has since superseded is
/// ignored, so a late ack never confirms newer content than the plugin holds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Ack {
    /// The queue sequences of the patches the plugin successfully applied.
    pub seqs: Vec<u64>,
}

/// Errors from encoding or decoding a protocol message.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// The value could not be encoded to `MessagePack`.
    #[error("protocol encode failed: {0}")]
    Encode(#[from] encode::Error),
    /// The bytes could not be decoded from `MessagePack`.
    #[error("protocol decode failed: {0}")]
    Decode(#[from] decode::Error),
}

/// Encode a protocol message to `MessagePack`, with field names preserved.
///
/// # Errors
///
/// Returns [`ProtocolError::Encode`] when `value` cannot be encoded.
pub fn to_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    Ok(rmp_serde::to_vec_named(value)?)
}

/// Decode a protocol message from `MessagePack`.
///
/// # Errors
///
/// Returns [`ProtocolError::Decode`] when `bytes` is not a valid message of type `T`.
pub fn from_msgpack<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    let mut deserializer = rmp_serde::Deserializer::from_read_ref(bytes);
    deserializer.set_max_depth(MAX_MSGPACK_NESTING.saturating_add(1));
    Ok(T::deserialize(&mut deserializer)?)
}

const MAX_MSGPACK_NESTING: usize = 32;

#[cfg(test)]
mod tests;
