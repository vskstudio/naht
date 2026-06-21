# Naht — Architecture

This document is the authoritative design. It is written so an implementer needs no outside context:
every component, boundary, and decision is stated here. The reasoning behind each decision is in
[`prior-art.md`](prior-art.md); the build order is in [`spec.md`](spec.md).

## 1. One sentence

Naht keeps a filesystem tree of Roblox source and a live Studio DataModel in **bidirectional sync**,
using a **persisted last-sync base** to do **real 3-way merges** and to **never silently overwrite**
either side.

## 2. Hard constraints (the ceiling no tool can beat)

These come from the Roblox Studio plugin API and shape every decision:

1. **Studio plugins run Luau only** — no Rust/WASM/native inside Studio.
2. **No filesystem access from inside Studio** — all disk I/O lives in the external daemon; the
   plugin only speaks HTTP to `localhost`.
3. **`HttpService` is request/response only** — no server push, no WebSocket client. The plugin must
   *initiate* every exchange (long-poll). The daemon can never spontaneously push to Studio.
4. **Property/type coverage is incomplete** — terrain, CSG geometry, `MeshPart.MeshId` binary data,
   and security-locked properties like `HttpService.HttpEnabled` cannot round-trip live. See
   [§9 Limits & workarounds](#9-limits--workarounds).
5. **Assets (meshes, images, audio) are cloud uploads, not files** — a sync tool references an asset
   id; it does not sync pixels.
6. **Conflict is inevitable** — two live editors (FS + Studio) means a merge/conflict policy is
   mandatory, not optional.

## 3. Components

A Cargo workspace plus a hand-written Luau plugin.

### `naht-core` (library) — the brain, zero network I/O
- **VFS** — a pluggable virtual filesystem trait (real disk or in-memory) so the reconciler is unit-
  testable without touching the disk.
- **PathMapper** — convention-based mapping between files and the Roblox instance tree, plus the
  optional config overlay (see §6).
- **Middleware** — converts a file to an instance `Snapshot` and back, using the
  [`rbx-dom`](https://github.com/rojo-rbx/rbx-dom) ecosystem (`rbx_dom_weak`, `rbx_reflection`,
  `rbx_binary`, `rbx_xml`). We depend on it; we do not reinvent Roblox type handling.
- **Reconciler** — diffs the current tree against the persisted last-sync base and produces a set of
  patches, in either direction.
- **Merge** — 3-way text merge for scripts (base = last-sync content); conflict detection for binary.
- **State store** — the SQLite-backed persisted base (see §5).
- **Protocol types** — the messages exchanged with the plugin, defined with `serde` so the wire
  encoding is a swappable layer.

All `naht-core` errors are typed with `thiserror`. No `unwrap()`/`expect()` on fallible I/O.

### `naht` (binary) — CLI + daemon
- The localhost HTTP server (`axum`) that the plugin talks to.
- The file watcher (`notify-debouncer-full`).
- Session orchestration and ownership of the SQLite database.
- All CLI commands: `init`, `init --from-rojo`, `serve`, `pull`, `build`, `status`, `resolve`.
- `anyhow` at this boundary for ergonomic error context.

### `plugin/` (Luau) — the hands, kept deliberately thin
- Long-polls the daemon, applies received patches to the DataModel, POSTs Studio-side edits back.
- Renders connection state (connected / reconnecting / conflict).
- Encodes/decodes MessagePack.
- **No sync logic** lives here. Mapping, reconciliation, and merge all live in `naht-core`. The
  thinner the plugin, the fewer the bugs — this is exactly where Rojo/Argon went wrong.

## 4. Data flow

**FS → Studio**
1. Watcher detects a file change → VFS.
2. Middleware turns the file into a `Snapshot`.
3. Reconciler diffs the snapshot against the persisted base → patch.
4. Patch is queued; the plugin's next long-poll picks it up.
5. Plugin applies it to the DataModel and acks; the daemon advances the base.

**Studio → FS**
1. Plugin detects a DataModel change → POSTs it to the daemon.
2. Middleware turns the instance change into file content.
3. Reconciler diffs against the persisted base.
4. Daemon writes the file (or raises a conflict); on success it advances the base.

**Conflict** (both sides changed since the base)
- Detected by comparing content hashes against the SQLite base.
- **Text:** 3-way merge with the last-sync content as the base. Clean merge → written, base advanced.
- **Unmergeable text:** git-style markers (`<<<<<<< FS` / `=======` / `>>>>>>> Studio`) written into
  the file, the path marked *conflicted* in SQLite, and sync for that path **frozen** until
  `naht resolve` confirms the markers are gone.
- **Binary:** file-level conflict, never auto-merged.

## 5. State store (SQLite)

The persisted last-sync base — the thing neither Rojo nor Argon keeps — lives in `.naht/state.db`.

Per instance, keyed by stable GUID:

| column | purpose |
|---|---|
| `guid` (PK) | stable instance identity across sessions |
| `path` | filesystem path it maps to |
| `class` | Roblox class name |
| `content_hash` | fast change detection |
| `base_content` (BLOB) | last-synced content — the **base** for 3-way text merge (text only) |
| `mtime` | filesystem modification time |
| `conflicted` | whether this path is frozen pending resolution |

A `meta` table holds the schema version and project identity. Writes are transactional. For binary
instances only the hash is stored (no merge base).

## 6. Configuration

- **Convention first.** Zero config for the common case:
  - `*.server.luau` → `Script`, `*.client.luau` → `LocalScript`, `*.luau` → `ModuleScript`
  - `init.luau` (and `init.server.luau` / `init.client.luau`) → the containing directory *becomes*
    that instance
  - a plain directory → `Folder`
- **Optional `naht.toml`** only for exceptions, applied as a **layered** config:
  defaults → `~/.naht/config.toml` (per-machine) → project `naht.toml`.
- **Inline property frontmatter** — non-default properties live with the source as a leading
  directive comment (`--!naht { Disabled = true }`), instead of Rojo's separate `.meta.json` files.
- **Build config is separate from sync mapping** — a sync-mapping change must never break a
  reproducible build.
- **`naht init --from-rojo`** reads an existing `default.project.json` and converts it, so Rojo users
  migrate without friction.

## 7. Transport & protocol

- **HTTP on `localhost`** (forced by constraint §2.3), bodies encoded as **MessagePack** — binary,
  handles complex Roblox types, with usable Luau libraries. JSON is avoided for payloads; the encoding
  is isolated behind the `serde` protocol types so it can be swapped (e.g. protobuf) later without a
  rewrite. We do not pursue gRPC (needs HTTP/2, unsupported by `HttpService`).
- **Endpoints:** `GET /info` (handshake + `servePlaceId` guard), `GET /patches` (long-poll, held open
  until a change or timeout), `POST /changes` (Studio → FS), `GET /heartbeat` (liveness ping).
- **Resilience** is the headline feature here, because it is exactly what Rojo and Argon get wrong:
  heartbeat, automatic reconnect with backoff, an explicit connection-state indicator, and — on
  reconnect — a **re-diff against the persisted state** rather than a blind re-push.

## 8. Error handling

- **No `unwrap()`/`expect()` on any FS or network operation in the sync loop.** This is the literal
  root cause of Rojo's data-loss crashes (`.unwrap()` on `PermissionDenied`).
- A failed write **pauses the affected path** and surfaces the error; it never kills the session and
  never discards the pending change.
- `thiserror` in `naht-core`, `anyhow` at the binary boundary.
- **Safety guard:** `servePlaceId` — the daemon refuses to sync into a Studio session whose place id
  doesn't match the project, preventing source from landing in the wrong game.

## 9. Limits & workarounds

We cannot beat the API ceiling (§2). What we *can* do — and Rojo/Argon don't — is **detect and report
explicitly** instead of dropping silently, and handle the binary cases as cleanly as possible. Detail
lives in the final stage of [`spec.md`](spec.md).

| Case | Status | Approach |
|---|---|---|
| **`MeshId`** | 🟡 partial | The asset-id string syncs like any property; the mesh binary is a cloud asset uploaded via Open Cloud, not a synced file. |
| **Terrain** | 🟡 possible, deferred | Readable/writable via `ReadVoxels`/`WriteVoxels`; can be synced as an opaque binary voxel blob (no diff/merge). Low priority / YAGNI for v1. |
| **CSG / Unions** | 🟡 binary round-trip | Engine-generated binary geometry can't be rebuilt from text, but round-trips inside `rbxm` model files (opaque, file-level). |
| **`HttpEnabled` & security-locked props** | 🔴 hard block | Not settable by scripts/plugins by design. Naht warns and points to Game Settings; offers a place-file fallback. |

Where Naht can do **better than Rojo** on binary/asset handling (clear detection, explicit warnings,
clean opaque round-trips instead of silent drops or overwrites), it should — treated as a bonus goal,
not a v1 blocker.

## 10. Testing strategy

- **`naht-core`:** unit tests over an in-memory VFS — mapping, reconcile, merge, and state store in
  isolation.
- **Integration:** a fake Studio HTTP client (the `fake-studio` pattern) drives end-to-end FS ↔
  simulated-Studio scenarios, including conflict and reconnect.
- **Plugin:** a minimal Luau harness, added once the protocol stabilizes.
