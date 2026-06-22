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

## Shipped — Stages 0–12 (merged)

These stages are implemented and merged; their detail lives in git history, not here. Summary:

- **0** scaffolding · **1** VFS + PathMapper + Snapshot · **2** SQLite state store · **3** reconciler
  + 3-way merge · **4** daemon HTTP + MessagePack protocol · **5** CLI + layered config + model build
  · **6** Studio plugin (Luau) — v1 live-sync milestone · **7** resilience, limits handling & polish.
- **8** daemon observability (`tracing`) + CLI ergonomics (`serve --port`, `-v`) · **9** place-file
  build (`.rbxl`/`.rbxlx`) + `build --watch` · **10** resilience & edge-case hardening (ack-gating,
  atomic-save coalescing, large-tree guard) · **11** terrain voxel sync (`naht-core` blob engine +
  Luau `Terrain`) · **12** Open Cloud asset upload (`[assets]`, uploader trait + cache).

**v1 is shipped** (live bidirectional sync, conflict-safe 3-way merge, persisted state).

---

# Remaining stages — audit follow-ups

A deep audit of the Stage 8–12 acceptance criteria (tests actually exercising the behavior, not just
compiling) found two correctness gaps and one coverage gap. The stages below close them. Same rules
(TDD, no `unwrap()`/`expect()` on fallible I/O, CI green) and the same one-stage-one-PR workflow.

---

## Stage 13 — Wire terrain sync into the live daemon

**Goal:** make Stage 11's terrain blob engine work **end to end** through the running daemon. Today
`naht-core`'s `binary`/blob engine and the Luau `Terrain` module are unit-tested in isolation, but the
daemon never drives them: `commands` calls `limits::scan` with no options, the `Session` never calls
the blob reconcile, and there is no blob transport on the wire. Terrain therefore does not actually
sync in a real session.

**Scope** (`naht`, `naht-core`, `plugin/`)
- Thread a `terrain_sync` flag from config (e.g. `[serve] terrain_sync = true`, default off) into
  `limits::scan` via its `Options`, so an enabled session suppresses the terrain warning at the
  **daemon level**, not just in the unit test.
- Carry `BlobPatchBatch` over the wire — either a dedicated `GET /blobs` (long-poll) + `POST /blobs`
  pair, or fold blobs into the existing `/patches` / `/changes` channel. State the choice in the PR.
- `Session` owns blob reconcile: on rescan / applied changes it calls the blob `reconcile_blobs()`,
  queues blob patches for the long-poll, persists the `.terrain` blob and its hash (hash-only base,
  architecture §5), and freezes a both-sides blob conflict like any binary path.
- Plugin `Connection` wires `Terrain.luau` read/write to the blob transport.

**Acceptance / tests** (fake Studio HTTP client, mirroring the Stage 4 pattern)
- End-to-end: a `.terrain` blob written on the filesystem appears to the fake client via the blob
  channel; a Studio-side blob POST writes the `.terrain` file byte-for-byte.
- **Integration**, not just the `limits` unit: with `terrain_sync` enabled in config, `naht serve`
  does **not** emit the Stage 7 terrain warning; with it disabled, the warning still fires.
- A both-sides blob change freezes the path as a binary conflict through the live daemon path.

---

## Stage 14 — Isolate asset-upload failures to the failing path

**Goal:** honor architecture §8 — a failed upload **pauses only that asset's path** and surfaces the
error; it never blocks the rest of the sync. Today `rewrite_snapshot_assets()` propagates the first
`AssetError` with `?`, aborting the **whole** snapshot rewrite on a single bad asset. The current unit
test only exercises `resolve_asset` in isolation, so it hides the wholesale-abort behavior.

**Scope** (`naht-core`)
- `rewrite_snapshot_assets()` must not short-circuit on the first failure. Walk every asset-bearing
  property/child, and for each: on success rewrite to `rbxassetid://…`; on failure leave the property
  at its original reference (paused / reference-only) and record the error. Return the set of
  per-path errors alongside the (partially) rewritten snapshot, rather than a single early `Err`.
- The daemon reports each failed path (and, in a live session, pauses it) without dropping siblings.

**Acceptance / tests** (fake uploader)
- A snapshot with assets A, B, C where the uploader fails on A: **B and C are rewritten** to their
  uploaded ids, **A keeps its original reference**, and A's error is surfaced — not a wholesale abort.
- A failure on one path does not change how many of the *other* paths upload (no skipped siblings).
- The existing single-asset isolation behavior stays green.

---

## Stage 15 — Close the acceptance-coverage gaps

**Goal:** assert behavior the Stage 9/10/12 audit found likely-correct but **untested**, so a future
regression is caught. No behavior change expected — these are tests (plus any fix a new test exposes).

**Scope / Acceptance** (tests)
- **Place root is a `DataModel`.** The place-build tests assert the built **root instance's class is
  `DataModel`** (`build` + CLI), not merely that services are populated (Stage 9 criterion 1).
- **Real property round-trips.** `Color3`, `Vector3`, an enum-typed property, `Attributes`, and a
  `CollectionService` tag each survive **FS → snapshot → FS** *and* **snapshot → place-file →
  snapshot** with value identity asserted — not only frontmatter string↔`Variant` parsing
  (Stage 10 criterion 3).
- **Assets-disabled determinism.** Building a fixture twice with `[assets]` disabled yields
  **byte-identical** output, and a snapshot-level check confirms **no property was rewritten** when
  assets are off (Stage 12 criterion 4).

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
