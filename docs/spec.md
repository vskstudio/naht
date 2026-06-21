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
  `naht serve`, `naht pull` (Studio → FS on demand), `naht build` (emit `rbxl`/`rbxm` via `rbx-dom`),
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

## Out of scope for v1 (explicitly deferred)
- Live terrain voxel sync.
- Open Cloud asset *upload* pipeline (mesh/image binaries) — only the asset-id reference is synced.
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
