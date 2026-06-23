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

## Stage 17 — `naht init --from-rojo` migrates the project tree, not just the name

**Goal:** make Rojo migration actually move a project over. Today `--from-rojo` reads only `name` and
`servePlaceIds` from `default.project.json` and writes a `naht.toml`; it **ignores the `tree`** — the
`$path` / `$className` / `$properties` mapping that says where a Rojo project's source actually lives.
A migrated project therefore syncs nothing until the user re-lays-out the tree by hand. The migration
must translate the Rojo tree into Naht's convention-first layout, recording only the genuine
exceptions in `naht.toml`.

**Scope** (`naht`, possibly `naht-core` for the mapping)
- Parse the Rojo `tree`: walk each node's `$className`, `$path`, `$properties`, and nested children.
- For each `$path` node, map the referenced files into Naht's convention layout where the convention
  already expresses it (`*.server.luau` → Script, `init.luau` → folder-as-instance, etc.) — do **not**
  emit redundant config for anything convention already covers.
- Where Rojo encodes something convention cannot infer (a non-default `$className` on a folder, a
  `$properties` block, a path alias), record that as the minimal Naht equivalent (frontmatter on the
  file, or a `naht.toml` entry) — surfacing, not silently dropping, anything Naht cannot represent.
- Leave the user's source files in place; migration rewrites configuration/layout metadata, never the
  scripts themselves.

**Acceptance / tests**
- A `default.project.json` whose `tree` maps `ReplicatedStorage.Common` → `src/common` (with
  `*.server.luau`/`*.client.luau`/module files) migrates so that `naht build` / `naht serve` produce
  the **same instance tree** Rojo would have — asserted on the built model, not just on `naht.toml`.
- A `$className` or `$properties` Rojo cannot express by convention is carried into the migrated
  project (frontmatter or config), with a round-trip assertion on that property/class.
- Anything Naht cannot represent is **reported** to the user (a warning line), never dropped silently.
- The existing name + place-id conversion stays green.

---

## Stage 18 — Release & packaging pipeline

**Goal:** make Naht installable without `cargo build` from a checkout. Today CI runs fmt/clippy/test
only; there is no artifact. A tagged release must produce the CLI binaries and the Studio plugin in
the forms a user actually installs.

**Scope** (`.github/workflows`, `plugin/`)
- A release workflow triggered on a version tag (`v*`) that builds the `naht` binary for the standard
  targets (at least Linux x86_64, macOS arm64+x86_64, Windows x86_64) and attaches them to a GitHub
  Release.
- Package the Studio plugin as an installable `.rbxmx` (the hand-written Luau under `plugin/src`,
  assembled into one model file) and attach it to the same release.
- The release is reproducible from the tag alone: no manual steps, version derived from the tag and
  checked against the workspace version so a mismatch fails the build.

**Acceptance / tests**
- The workflow is exercisable on a tag and yields: one binary artifact per target and one
  `naht-plugin.rbxmx`, all attached to the release.
- A tag whose version does not match the workspace `Cargo.toml` version fails the job (guard test or
  a script unit-tested in isolation).
- The plugin-packaging step is verifiable headless: the assembled `.rbxmx` loads (e.g. under
  `rbx_xml` / lune) and contains the expected top-level plugin script.

---

## Stage 19 — User quickstart & Studio validation checklist

**Goal:** close the documentation and live-validation gap. The docs today are implementer-facing
(architecture, spec, prior-art); there is no end-to-end user guide, and the live Studio loop is only
"validated manually" with no written procedure. This stage is **docs + a reproducible manual
checklist**, no production code.

**Scope** (`docs/`, `README.md`, `plugin/README.md`)
- A `docs/quickstart.md`: install the CLI, install the plugin in Studio, `naht init`, `naht serve`,
  first round-trip edit (disk → Studio and Studio → disk), and how to read/resolve a conflict.
- A written **Studio validation checklist** (in `plugin/README.md` or `docs/`): the exact manual
  steps that exercise the live loop the automated tests cannot — connect, edit both sides, force a
  conflict, terrain sync, reconnect re-diff — each with the expected observable result.
- Cross-link from `README.md` so the quickstart is the obvious entry point; keep claims in `README.md`
  in step with what actually ships (e.g. that `--from-rojo` now migrates the tree, per Stage 17).

**Acceptance / tests**
- `docs/quickstart.md` exists and walks a new user from zero to a confirmed bidirectional sync.
- The validation checklist enumerates each manual scenario with its expected result, so any tester
  (or a future automated harness) can follow it deterministically.
- No doc claims a capability the code does not have (verified by reading the commands against the CLI).

---

## Permanently out of scope
- roblox-ts authoring of the plugin (hand-written Luau is the decision).
- Any Neublox-specific integration — the Neublox adapter lives outside this repo.
