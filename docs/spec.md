# Naht — Implementation Spec (staged)

This spec is cut into **stages**. Each stage is **one PR**: self-contained, independently testable,
and leaves the project in a working state. The implementing agent follows the workflow in
[`../IMPLEMENTATION.md`](../IMPLEMENTATION.md): implement a stage, run the review loop, merge, move to
the next.

Design context — read before starting: [`architecture.md`](architecture.md). Decision rationale:
[`prior-art.md`](prior-art.md).

**Conventions for every stage**
- TDD: write the failing test first.
- No `unwrap()`/`expect()` on fallible I/O. `thiserror` in `naht-core`, `anyhow` in the `naht` binary.
- CI must be green: `cargo fmt --check`, `cargo clippy --all-targets` (0 warnings), `cargo test`.
- Depend on the `rbx-dom` ecosystem for Roblox types — do not reinvent them.

---

## Stage 0 — Scaffolding

**Goal:** a building, CI-green workspace skeleton.

**Scope**
- Cargo workspace with members `naht-core` (lib) and `naht` (bin). `plugin/` directory reserved.
- Add dependencies (not yet wired): `rbx_dom_weak`, `rbx_reflection`, `rbx_binary`, `rbx_xml`,
  `thiserror`, `anyhow`, `serde`, `serde_json`, `rmp-serde` (MessagePack), `axum`, `tokio`,
  `notify-debouncer-full`, `rusqlite`, `diffy`, `clap`.
- CI workflow: `fmt --check`, `clippy --all-targets -D warnings`, `test`.
- `rustfmt.toml`, `.gitignore`, crate-level docs.

**Acceptance**
- `cargo build`, `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` all pass on an empty
  workspace with a trivial smoke test.

---

## Stage 1 — VFS + PathMapper + Snapshot

**Goal:** turn a directory of files into a Roblox instance snapshot tree (and back), with no network
and no disk dependency in tests.

**Scope** (`naht-core`)
- `Vfs` trait (read/list/write/remove) with a real-disk impl and an in-memory impl for tests.
- `PathMapper`: the conventions from architecture §6 (`*.server.luau`→`Script`,
  `*.client.luau`→`LocalScript`, `*.luau`→`ModuleScript`, `init.luau`→folder-as-instance,
  directory→`Folder`).
- Middleware: file → instance `Snapshot` and `Snapshot` → file, via `rbx-dom`.
- Inline property frontmatter parsing (`--!naht { ... }`).

**Acceptance / tests** (in-memory VFS)
- A nested fixture tree maps to the expected instance tree (classes, names, hierarchy).
- `init.luau` collapses its directory into the instance.
- Frontmatter properties are parsed and attached.
- Round-trip: snapshot → files → snapshot is identity for a representative tree.

---

## Stage 2 — State store (SQLite)

**Goal:** persist and reload the last-sync base.

**Scope** (`naht-core`)
- SQLite schema from architecture §5 (`instances`, `meta`), transactional writes, schema versioning.
- API: open/create at `.naht/state.db`, upsert/get/remove per GUID, mark/clear `conflicted`, store
  `base_content` for text instances (hash only for binary).

**Acceptance / tests**
- Write a base, reopen the DB, read it back identically.
- Schema-version mismatch is detected and reported (no silent corruption).
- `conflicted` flag round-trips; binary instances store no `base_content`.

---

## Stage 3 — Reconciler + 3-way merge

**Goal:** compute directional patches and resolve conflicts safely. Still no network.

**Scope** (`naht-core`)
- Reconciler: diff a snapshot tree against the persisted base → ordered patch set, per direction.
- 3-way text merge via `diffy` (base = `base_content`). Clean merge advances the base.
- Conflict: write git-style markers, mark the path `conflicted`, freeze it.
- `status` (list conflicted/pending paths) and `resolve` (clear a path once markers are gone) logic.

**Acceptance / tests** (in-memory VFS + state store)
- One-sided change → patch only in that direction; base advances; no spurious patches.
- Both sides change non-overlapping regions → auto-merged cleanly.
- Both sides change the same region → markers written, path frozen, no data lost.
- `resolve` refuses while markers remain; succeeds once removed.

---

## Stage 4 — Daemon HTTP + protocol

**Goal:** the localhost server and wire protocol, driven by a fake Studio client.

**Scope** (`naht`)
- `axum` server on `localhost`; MessagePack bodies via the `naht-core` protocol types.
- Endpoints: `GET /info` (handshake + `servePlaceId` guard), `GET /patches` (long-poll),
  `POST /changes`, heartbeat.
- File watcher (`notify-debouncer-full`) feeding the reconciler; patch queue to the long-poll.
- Resilience: heartbeat, reconnect-safe re-diff against persisted state on a fresh `/info`.

**Acceptance / tests**
- A fake Studio HTTP client: FS change → appears via `/patches`; `POST /changes` → file written.
- Conflicting change over the wire → markers + frozen path (Stage 3 behavior end-to-end).
- Mismatched `servePlaceId` is rejected at handshake.
- A disconnect/reconnect re-diffs instead of blind re-push (no duplicate/clobber).

---

## Stage 5 — CLI

**Goal:** the user-facing commands.

**Scope** (`naht`)
- `naht init` (scaffold a project), `naht init --from-rojo` (convert `default.project.json`),
  `naht serve`, `naht pull` (Studio → FS on demand), `naht build` (emit a model/place file via
  `rbx-dom`: binary `.rbxm` by default, XML `.rbxmx`/`.rbxlx` when that extension is used),
  `naht status`, `naht resolve`.
- Layered config loading (defaults → `~/.naht/config.toml` → project `naht.toml`).

**Acceptance / tests**
- `init` produces a working minimal project; `serve` runs against it.
- `--from-rojo` converts a sample `default.project.json` to equivalent Naht config.
- `status`/`resolve` reflect the Stage 3/4 conflict state.
- `build` produces a loadable model/place file for a fixture.

---

## Stage 6 — Studio plugin (Luau) — enables live bidirectional sync

**Goal:** the thin Luau plugin that closes the live loop. This is the v1 milestone.

**Scope** (`plugin/`)
- Long-poll client against the daemon; apply received patches to the DataModel.
- Detect DataModel edits and POST them to `/changes`.
- MessagePack encode/decode (hand-written or a vendored Luau lib).
- Connection-state UI (connected / reconnecting / conflict); reconnect with backoff.
- **No sync logic** — strictly transport + apply + report.

**Acceptance / tests**
- Manual: editing a file updates Studio live; editing in Studio updates the file live.
- Conflict path surfaces in the plugin UI and freezes correctly.
- Killing and restarting the daemon reconnects without data loss.
- Minimal Luau harness for the encode/decode and patch-apply paths.

---

## Stage 7 — Resilience, limits handling & polish

**Goal:** harden, and handle the API ceiling explicitly (architecture §9).

**Scope**
- Audit: zero `unwrap()`/`expect()` on I/O in the sync loop; every transport failure is recoverable.
- Reconnect re-diff hardening under partial/failed application.
- **Unsyncable-property detection:** at session start, scan for properties that can't round-trip
  (`MeshId` binary, terrain, CSG, `HttpEnabled`) and **warn explicitly** with guidance, never drop
  silently. Offer the place-file fallback.
- Binary round-trips: `rbxm` for CSG/models; `MeshId` reference sync (asset upload out of scope for
  v1 but documented). Terrain blob sync remains deferred.
- Final docs pass: README, `docs/`, and `naht --help` reflect actual behavior.

**Acceptance / tests**
- A project containing a Union round-trips via `rbxm` without loss.
- A locked/unsyncable property produces a clear warning, not a silent drop or crash.
- Docs match the shipped commands and flags.

---

# Post-v1 stages

Stages 0–7 ship v1 (live bidirectional sync). The stages below extend it. They keep the same rules
(TDD, no `unwrap()`/`expect()` on fallible I/O, CI green) and the same one-stage-one-PR workflow.
Stages 8–10 are quality-of-life and hardening; Stages 11–12 lift items previously deferred from v1.

---

## Stage 8 — Daemon observability & CLI ergonomics

**Goal:** make a running daemon legible and the CLI configurable at the command line. No behavioral
change to the sync engine.

**Scope** (`naht`)
- `tracing` + `tracing-subscriber` wired at the binary boundary. Instrument the hot paths:
  watcher (which file changed, debounced batch size), session (patch emitted, base advanced,
  conflict frozen), server (handshake, place-id guard outcome, long-poll wake, `POST /changes`).
  `naht-core` stays free of logging deps — it returns data, the binary logs it.
- `naht serve --port <PORT>` overrides the configured/default port; `-v/--verbose` (repeatable, e.g.
  `-vv`) raises the log level (`warn` → `info` → `debug`/`trace`). Default level is `info` to stderr.
- Respect `RUST_LOG` / a `NAHT_LOG` env filter when present (overrides `-v`).

**Acceptance / tests**
- `serve --port` beats the `naht.toml` value, which beats the default (assert the resolved port).
- An invalid `--port 0` is rejected with a clear error, not a panic.
- A unit test over the log-filter resolution: `-v` count and env filter map to the expected level.
- Smoke: a file change emits exactly one structured "patch" event at `info` (capture the subscriber).

---

## Stage 9 — Place-file build & watch mode

**Goal:** `naht build` can emit a **place** (a `DataModel` with services), and can rebuild on change.

**Scope** (`naht-core` + `naht`)
- **Place build.** When the output extension is `.rbxl`/`.rbxlx`, build a `DataModel`-rooted tree
  instead of a bare model. **Service mapping is convention-first (architecture §6 extension):** a
  top-level directory whose name matches a known Roblox service (`Workspace`,
  `ServerScriptService`, `ReplicatedStorage`, `StarterGui`, …, validated against `rbx_reflection`)
  becomes that service; any other top-level entry is placed under `Workspace`. A top-level directory
  that looks service-like but is unknown produces an **explicit warning**, never a silent reparent.
  Model output (`.rbxm`/`.rbxmx`) keeps Stage 5 behavior unchanged.
- **`naht build --watch`.** Reuse the Stage 4 watcher to re-emit the output file on each debounced
  change; log each rebuild (Stage 8). Independent of `serve` — no daemon, no Studio.

**Acceptance / tests**
- A fixture with `ServerScriptService/` and `ReplicatedStorage/` top-level dirs builds a `.rbxlx`
  whose root is a `DataModel` with those services populated; loose top-level files land in `Workspace`.
- An unknown service-shaped top-level dir triggers the warning and falls back to `Workspace`.
- `.rbxm`/`.rbxmx` output is byte-for-byte unchanged from Stage 5 for a model fixture (no regression).
- `--watch` rebuilds the output after a file write (drive via the in-memory/temp VFS + a change event).

---

## Stage 10 — Resilience & edge-case hardening

**Goal:** push the sync loop past the happy path — partial failures, large trees, unusual properties.

**Scope** (`naht-core` + `naht`)
- **Partial application.** A patch batch the plugin half-applies (some nodes fail) must leave the
  base advanced only for acked nodes; the rest re-diff on the next cycle. No clobber, no double-apply.
- **Large-tree behavior.** A reconcile over a deep/wide fixture (e.g. 5k instances) completes within a
  documented bound and does not load the whole tree into one allocation when avoidable; add a
  benchmark-style test guarding against accidental O(n²) reconcile.
- **Unusual properties.** Round-trip coverage for `Attributes`, tags (`CollectionService`),
  `Color3`/`Vector3`/enum-typed properties, and multi-line/odd-encoding source; anything not
  round-trippable is reported via the Stage 7 detector, never dropped.
- **Watcher edge cases.** Rapid create→rename→delete bursts, and atomic-save (write-temp-then-rename)
  editors, resolve to the correct final state.

**Acceptance / tests**
- Half-failed batch: base advances only for acked nodes; the unacked node reappears in the next diff.
- Reconcile of the large fixture stays under the asserted time/allocation guard.
- Each listed property type survives FS → snapshot → FS and snapshot → place-file round-trips.
- An atomic-save sequence yields one coalesced patch with the final content, not a delete+create flap.

---

## Stage 11 — Terrain voxel sync (post-v1)

**Goal:** lift terrain from "deferred" (architecture §9) to an opaque, conflict-safe blob sync.

**Scope** (`naht-core` + `plugin/`)
- Plugin reads terrain via `ReadVoxels` and writes via `WriteVoxels`, shipping the region as an
  opaque binary blob over the existing protocol (a new patch/change kind). **No diff/merge** of voxel
  data — it is file-level, hash-compared, last-writer-with-conflict-freeze like other binary cases.
- On disk the blob lives under a conventional path (e.g. `*.terrain` next to its place root); the
  state store records its hash only (no `base_content`, per architecture §5 binary rule).
- Detection from Stage 7 flips from "warn: terrain unsyncable" to "syncing terrain as opaque blob".

**Acceptance / tests**
- A terrain blob round-trips FS ↔ (fake-Studio) without byte loss.
- A both-sides terrain change freezes the path as a binary conflict (no silent overwrite).
- With terrain sync enabled, the Stage 7 warning for terrain no longer fires.

---

## Stage 12 — Open Cloud asset upload (post-v1)

**Goal:** lift `MeshId`/image binaries from "reference-only" (architecture §9) to an actual upload
path, so a local mesh/image file can become an asset id.

**Scope** (`naht-core` + `naht`)
- An **asset uploader** behind a trait (real Open Cloud impl + a fake for tests), driven by an API
  key from config/env (never committed). A local binary referenced by a property (e.g. a mesh file)
  is uploaded once, its returned asset id cached by content hash in the state store, and the property
  rewritten to `rbxassetid://…`.
- Re-upload is skipped when the content hash is unchanged (idempotent). Upload failure **pauses that
  asset's path** with a clear error; it never blocks the rest of the sync (architecture §8).
- `naht.toml` gains an optional `[assets]` section (enable flag, key source); disabled by default so
  v1 behavior (reference-only) is the unchanged default.

**Acceptance / tests** (fake uploader)
- A new local mesh is uploaded once; its asset id is cached and the property rewritten.
- An unchanged mesh on the next run is **not** re-uploaded (hash hit).
- An upload failure pauses only that path and surfaces the error; other paths keep syncing.
- With `[assets]` disabled, behavior is byte-identical to pre-Stage-12 (reference-only).

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
