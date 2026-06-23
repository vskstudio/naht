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

## Shipped — Stages 0–15 (merged)

These stages are implemented and merged; their detail lives in git history, not here. Summary:

- **0** scaffolding · **1** VFS + PathMapper + Snapshot · **2** SQLite state store · **3** reconciler
  + 3-way merge · **4** daemon HTTP + MessagePack protocol · **5** CLI + layered config + model build
  · **6** Studio plugin (Luau) — v1 live-sync milestone · **7** resilience, limits handling & polish.
- **8** daemon observability (`tracing`) + CLI ergonomics (`serve --port`, `-v`) · **9** place-file
  build (`.rbxl`/`.rbxlx`) + `build --watch` · **10** resilience & edge-case hardening (ack-gating,
  atomic-save coalescing, large-tree guard) · **11** terrain voxel sync (`naht-core` blob engine +
  Luau `Terrain`) · **12** Open Cloud asset upload (`[assets]`, uploader trait + cache).
- **13** terrain wired into the live daemon (`[serve] terrain_sync`, `/blobs` channel, `Session` blob
  reconcile, plugin wiring) · **14** asset-upload failures isolated to the failing path · **15**
  acceptance-coverage hardening (DataModel-root, typed-property round-trips, assets-off determinism).

**v1 is shipped** (live bidirectional sync, conflict-safe 3-way merge, persisted state), plus the
post-v1 extension and its audit follow-up.

---

# Remaining stages

Same rules (TDD, no `unwrap()`/`expect()` on fallible I/O, CI green) and the same one-stage-one-PR
workflow.

---

## Stage 16 — Reconcile blobs on Studio text edits

**Goal:** close the one asymmetry the Stage 13 audit found. `Session::reconcile_blobs()` runs from
`rescan()` (filesystem events) and `apply_blob_changes()` (the `/blobs` channel), but **not** from
`apply_changes()` — the path that handles incoming Studio **text** edits (`POST /changes`). The spec
for Stage 13 said the session reconciles blobs "on rescan / applied changes"; the text-apply path is
the gap. In practice text and terrain are independent channels so no data is lost today, but a
filesystem terrain change that lands between a watcher rescan and a Studio text batch is not
re-reconciled until the next rescan — a latency hole, and a literal-spec divergence.

**Scope** (`naht`)
- `Session::apply_changes()` calls `reconcile_blobs()` after its text reconcile, exactly as
  `rescan()` does, so a single incoming Studio batch settles both channels.
- Keep it idempotent: a `reconcile_blobs()` with no blob changes must enqueue nothing and freeze
  nothing (no spurious patches on every text edit).

**Acceptance / tests** (fake Studio HTTP client)
- A filesystem `.terrain` change followed by a Studio **text** `POST /changes` (with no intervening
  rescan) results in the blob patch being queued for the `/blobs` long-poll — proving `apply_changes`
  drove the blob reconcile.
- A Studio text edit with **no** blob change on either side queues **no** blob patch and freezes
  nothing (idempotence).

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
