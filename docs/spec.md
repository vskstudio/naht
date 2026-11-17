# Naht: Implementation Spec (staged)

Naht was built in **stages**, each one a single self-contained PR that left the project working. This
file records what shipped; the per-stage detail lives in git history. Design context:
[`architecture.md`](architecture.md).

**Conventions that held for every stage**
- TDD: the failing test came first.
- No `unwrap()`/`expect()` on fallible I/O. `thiserror` in `naht-core`, `anyhow` in the `naht` binary.
- CI green: `cargo fmt --check`, `cargo clippy --all-targets` (0 warnings), `cargo test`.
- Depend on the `rbx-dom` ecosystem for Roblox types. Never reinvent them.

---

## Shipped: Stages 0 to 19 (merged, `v0.1.0`)

- **0** scaffolding · **1** VFS + PathMapper + Snapshot · **2** SQLite state store · **3** reconciler
  + 3-way merge · **4** daemon HTTP + MessagePack protocol · **5** CLI + layered config + model build
  · **6** Studio plugin (Luau), the v1 live-sync milestone · **7** resilience, limits handling & polish.
- **8** daemon observability (`tracing`) + CLI ergonomics (`serve --port`, `-v`) · **9** place-file
  build (`.rbxl`/`.rbxlx`) + `build --watch` · **10** resilience & edge-case hardening (ack-gating,
  atomic-save coalescing, large-tree guard) · **11** terrain voxel sync (`naht-core` blob engine +
  Luau `Terrain`) · **12** Open Cloud asset upload (`[assets]`, uploader trait + cache).
- **13** terrain wired into the live daemon (`[serve] terrain_sync`, `/blobs` channel, `Session` blob
  reconcile, plugin wiring) · **14** asset-upload failures isolated to the failing property (a build warning, and the rest of the build continues) · **15**
  acceptance-coverage hardening (DataModel-root, typed-property round-trips, assets-off determinism).
- **16** blobs reconciled on Studio text edits too (`apply_changes` → `reconcile_blobs`) · **17**
  `naht init --from-rojo` migrates the project **tree**, not just name/place-id · **18** release &
  packaging pipeline (per-target CLI binaries + `.rbxmx` plugin on a version tag) · **19** user
  [quickstart](quickstart.md) + a written Studio validation checklist.

**v1 is shipped and tagged `v0.1.0`**: live bidirectional sync, conflict-safe 3-way merge, persisted
state, terrain blob sync, Open Cloud asset upload, Rojo migration, and a release pipeline.

## Shipped: v0.2.0 (merged)

- **`GET /activity`** is a read-only endpoint exposing live sync activity to an external consumer: a
  `pending` count (unacked FS→Studio text patches) plus a bounded in-memory ring buffer (256) of
  recent *completed* sync events `{seq, unix_ms, path, direction, kind, outcome}`, with a `since`
  cursor (1-based seqs, strictly-greater filter: a consumer never re-sees its last event, and
  `buffer_start_seq` signals an eviction gap). Additive: no existing message shape changed. The
  protocol version was `1` when this shipped, `2` in v0.4.0 after the client headers became mandatory,
  and is `3` now (see the protocol page). The Neublox adapter that consumes it lives outside this repo.

## Shipped: v0.3.0 (merged)

- **Workspace on the VSK Rust book** (#39): `naht-core` split into modules under 500 lines, typed
  `thiserror` errors per layer that keep their source, no panic outside tests, rustdoc as the only
  comment, typed protocol payloads, the full exemplar lint table in `Cargo.toml` and `clippy.toml`.
- **Daemon hardening**: graceful shutdown on SIGINT and SIGTERM, request body limits, HTTP errors
  that no longer leak internals, a watcher that ignores its own writes and access events, exit code
  78 for a bad configuration.
- **Dependencies on latest** (#61): rbx 3, reqwest 0.13, toml 1, tower-http 0.7, rusqlite 0.41 and
  toolchain 1.99.1. Place and model output is byte-identical to v0.2.0, held by golden fixtures. A
  `naht.toml` with a duplicate key is now rejected (exit 78).
- **CI enforces everything** through `scripts/check.sh`: fmt, the 500-line limit, no plain comments,
  no em or en dash (#63), clippy with warnings denied, tests, rustdoc with warnings denied, full
  `cargo deny` and workspace `cargo machete`.

## Protocol 3 (unreleased)

- **Sequenced text patches**: each `GET /patches` patch carries its queue `seq` beside its fields
  (`SequencedPatch`, flattened on the wire). `POST /ack` takes `{seqs: [...]}` in place of `{paths: [...]}`:
  an acked patch leaves the queue and advances its base, and an ack for a superseded sequence is a no-op.
  The plugin holds its cursor at the first patch it rejected, so the next poll asks for it again.
- **Blob cursor as ack**: the `/blobs` cursor is the plugin's ack. The plugin advances it only past a batch it
  applied in full, and the daemon drops every queued blob patch below the cursor a poll sends.

- **`POST /changes` response**: a batch whose every change is accepted is answered `200` with an empty body.
  A batch with any refused change is refused whole: `400` with a JSON body `{"refused_paths": [...]}` naming
  each refused path, and nothing in the batch applies. The request is `ChangeBatch` `{changes: [...]}` where
  each change is externally tagged, `Upsert {path, class, content}` or `Delete {path}`. A malformed body is
  `400`, and a batch whose valid changes fail to apply is `500` with the Studio mirror restored to its state
  before the batch.
- **Body limits**: at most two POST bodies are read and handled at once, each capped at 64 MiB (`413`), with
  a 15-second read timer (`408`) and `400` for a truncated or malformed body. Version 2 shipped in v0.4.0
  with the old `Ack` and `PatchBatch` shapes, so the sequenced shapes above and these limits ship as version 3,
  and `PROTOCOL_VERSION` is `3`.

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration: the Neublox adapter lives outside this repo.
