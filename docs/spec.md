# Naht — Implementation Spec (staged)

Naht was built in **stages**, each one a single self-contained PR that left the project working. This
file records what shipped; the per-stage detail lives in git history. Design context:
[`architecture.md`](architecture.md). Decision rationale: [`prior-art.md`](prior-art.md).

**Conventions that held for every stage**
- TDD: the failing test came first.
- No `unwrap()`/`expect()` on fallible I/O. `thiserror` in `naht-core`, `anyhow` in the `naht` binary.
- CI green: `cargo fmt --check`, `cargo clippy --all-targets` (0 warnings), `cargo test`.
- Depend on the `rbx-dom` ecosystem for Roblox types — never reinvent them.

---

## Shipped — Stages 0–19 (merged, `v0.1.0`)

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
- **16** blobs reconciled on Studio text edits too (`apply_changes` → `reconcile_blobs`) · **17**
  `naht init --from-rojo` migrates the project **tree**, not just name/place-id · **18** release &
  packaging pipeline (per-target CLI binaries + `.rbxmx` plugin on a version tag) · **19** user
  [quickstart](quickstart.md) + a written Studio validation checklist.

**v1 is shipped and tagged `v0.1.0`**: live bidirectional sync, conflict-safe 3-way merge, persisted
state, terrain blob sync, Open Cloud asset upload, Rojo migration, and a release pipeline.

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
