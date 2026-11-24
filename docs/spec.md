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
  and is `4` in v0.5.0 (see the protocol page). The Neublox adapter that consumes it lives outside this repo.

## Shipped: v0.5.0 (merged)

- **Per project token** (#77): `naht serve` creates `.naht/token` (owner only: mode `0600` on Unix, and a protected DACL for the current user alone on Windows, each tightened if it is looser; on Windows the `.naht` directory is owner-only too) and every route except the `GET /info` handshake requires it in
  `x-naht-token`, so another local process can no longer read patches or post changes. `naht token`
  prints it; the plugin asks for it once per project and keeps it in its settings. Protocol version 4:
  the plugin and the daemon must be updated together.
- **Configurable limits** (#77): a `[limits]` section in `naht.toml` sets the terrain size cap, the
  inventory body limit, the mass deletion warning threshold and the connection cap, with today's values
  as defaults; absurd values stop `naht serve` with a clear message. The plugin reads the limits from
  the handshake. `naht serve` re-diffs the disk once at startup, so an edit made while no daemon ran
  reaches Studio on the next attach.
- **Studio renames, moves and live terrain** (#76): renames and reparents in Studio are written to disk,
  and live terrain edits sync. On attach the plugin posts its placed script inventory, which applies detected
  renames. A folder is followed as renamed only when the evidence is unambiguous: each `(class, fingerprint)`
  key has exactly one listed create and one unlisted script, the folder's pairs all land in one new folder, no
  listed script is left under the old one, and every unpaired create lies under the new folder. Twins, a split
  folder or an unrelated unpaired create infer nothing. Under a folder that qualifies, a script Studio deleted
  moves with its disk bytes and stays as a script Studio deleted while stopped does in a folder that was not
  renamed, and a script whose Studio content changed moves only once that content arrives through `/changes`, so
  no side's content is lost. A folder whose scripts all changed is followed by names: it must hold the same
  relative script names and classes, at least two, as exactly one new folder, and no other folder may hold that set.
- **Place binding** (#76): a project binds to the first published place that attaches, and another place
  is refused with a 403. Unpublished places never bind. `[serve] place_id` pins the place, and
  `naht unbind` clears the binding without touching the rest of the state. A project shared between two
  places is now refused.
- **Daemon follow ups** (#75): a connection cap, emptied folders after resolve and pull, one portable
  name rule for `[[tree]]` paths, and JSON refusals.
- **Sync queue** (#74): acks by sequence, cursors kept across reconnects, and binary conflicts can be
  resolved.

## Shipped: v0.4.0 (merged)

- **Security fix GHSA-xhmf-2rv2-556x** (#72): one gate in front of every daemon route refuses a request
  with an `Origin` header, a non-loopback `Host` or no `x-naht-client` header, which stops web pages and
  DNS rebinding. Every client path is validated (no escape from the root, no symlink, no character
  Windows forbids), a batch with a refused path is answered `400 {"refused_paths": [...]}` and writes
  nothing, text batches roll back on failure, bodies are bounded in size, time, nesting and concurrency,
  and client values are logged escaped. Protocol version 2: the plugin and the daemon must be updated
  together.
- **Restart safe sync** (#71): sync stays correct across a daemon restart, and the plugin and CLI
  check that the daemon they reach is the one they expect.
- **Plugin fixes** (#70): stop, apply, terrain and failure reporting.
- **Core hardening** (#69): paths, atomic writes, scans and asset uploads.

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

## Protocols 3 and 4 (shipped in v0.5.0)

- **Sequenced text patches**: each `GET /patches` patch carries its queue `seq` beside its fields
  (`SequencedPatch`, flattened on the wire). `POST /ack` takes `{seqs: [...]}` in place of `{paths: [...]}`:
  an acked patch leaves the queue and advances its base, and an ack for a superseded sequence is a no-op.
  The plugin holds its cursor at the first patch it rejected, so the next poll asks for it again.
- **Blob cursor as ack**: the `/blobs` cursor is the plugin's ack. The plugin advances it only past a batch it
  applied in full, and the daemon drops every queued blob patch below the cursor a poll sends.
- **Session on every ack and poll**: `GET /patches`, `GET /blobs` and `POST /ack` require a `session` query
  parameter holding the session id from `GET /info`. A request without it is answered `400`. A request naming
  another session is answered `409`, the plugin re-handshakes, and the request changes nothing; an ack from a
  previous session settles no patch.

- **`POST /changes` response**: a batch whose every change is accepted is answered `200` with an empty body.
  A batch with any refused change is refused whole: `400` with a JSON body `{"refused_paths": [...]}` naming
  each refused path, and nothing in the batch applies. The request is `ChangeBatch` `{changes: [...]}` where
  each change is externally tagged, `Upsert {path, class, content}` or `Delete {path}`. A malformed body is
  `400`, and a batch whose valid changes fail to apply is `500` with the Studio mirror restored to its state
  before the batch.

- **`POST /inventory` response**: takes a MessagePack `{scripts: [{path, class, hash}]}` body and a `session` query parameter. It is answered `200` with an empty body when the listing applies, `400` with `{"refused_paths": [...]}` when it holds a `Delete` or a refused path, and `409` for another session's id.
- **Body limits**: at most two POST bodies are read and handled at once, each capped at 64 MiB (`413`), with
  a 15-second read timer (`408`) and `400` for a truncated or malformed body. Version 2 shipped in v0.4.0
  with the old `Ack` and `PatchBatch` shapes, so the sequenced shapes above and these limits shipped as version 3.
  v0.5.0 declares `PROTOCOL_VERSION` 4.

## Protocol 5 (unreleased)

Nothing in this section is in v0.5.0, whose `PROTOCOL_VERSION` is 4. The handshake compares versions for equality, so this travels with protocol 5 and is not released yet. The `[[tree]]` refusal (`TreePathHidden`) is new here; the project path rule already refused a dot segment in v0.5.0.

- **`POST /inventory` response**: takes a MessagePack `{scripts: [{path, class, hash}]}` body and a `session` query parameter. It is answered `200` with `{"wanted": [...]}` when the listing applies: `wanted` names the new paths of scripts under a renamed folder whose Studio source the daemon still needs, and is empty when there are none (an empty listing changes nothing), `400` with `{"refused_paths": [...]}` when it holds a `Delete` or a refused path, `409` for another session's id, and `404` from a daemon that predates the route, which the plugin reports as a warning rather than a failure. The handshake compares versions for equality, so the `wanted` reply is unreleased and travels with protocol 5. A plugin built before the `wanted` list discards the reply, and a new plugin against an older daemon reads the empty `200` as no wanted paths.
- **Dot names are refused** (#79): a path segment that starts with `.` fails the project path rule, so every
  `/changes` and `/inventory` path and every `[[tree]]` path in `naht.toml` refuses it, and the plugin does not
  sync a script whose name starts with `.` and warns once. The scanner and the watcher ignore dot entries inside a
  tree, so a dot-named file or folder under a tree was never synced and loses nothing. The tree folder itself is
  different: `naht build` and `init --from-rojo` read a `[[tree]]` folder whose name starts with `.` and migrate
  it, so this is a breaking change. `naht serve`, `naht build` and `init --from-rojo` now all refuse such a
  `[[tree]]` path with `TreePathHidden`, which names the path. The fix is to rename the folder or change the path.

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration: the Neublox adapter lives outside this repo.
