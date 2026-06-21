# Implementation Workflow

You are the implementing agent for **Naht**. Everything you need is in this repo — do not ask
questions. The design is frozen.

- **Architecture (authoritative):** [`docs/architecture.md`](docs/architecture.md)
- **Staged spec (what to build, in order):** [`docs/spec.md`](docs/spec.md)
- **Why every decision was made:** [`docs/prior-art.md`](docs/prior-art.md)

## Status

**Stages 0–7 are merged — v1 (live bidirectional sync) is shipped and CI-green.** The remaining work
is the post-v1 extension in [`docs/spec.md`](docs/spec.md):

- **Stage 8** — daemon observability (`tracing`) + CLI ergonomics (`serve --port`, `-v`).
- **Stage 9** — place-file build (`.rbxl`/`.rbxlx`, convention-mapped services) + `build --watch`.
- **Stage 10** — resilience & edge-case hardening (partial apply, large trees, unusual properties).
- **Stage 11** — terrain voxel sync (opaque blob, post-v1).
- **Stage 12** — Open Cloud asset upload (`MeshId`/image, post-v1).

## The loop

Work **one stage at a time**, in order. v1 was Stage 0 → Stage 7; the post-v1 work continues
Stage 8 → Stage 12. For each stage:

### 1. Implement the stage
- Open a branch and a PR for that stage only.
- Follow TDD: write the failing test first, then the minimal code to pass.
- Honor the conventions in `docs/spec.md`: no `unwrap()`/`expect()` on fallible I/O; `thiserror` in
  `naht-core`, `anyhow` in the `naht` binary; depend on `rbx-dom`, don't reinvent Roblox types.
- The stage's **Acceptance / tests** section is the definition of done.

### 2. Review — run this prompt **3 times**
After the stage is implemented, run the following review **three times in a row**. Each run is a fresh,
skeptical pass; fix what it legitimately finds, then run it again.

```
Review

- Respect des CI
- Décision d'architecture
- Performance
- Code quality
- Sécurité
- Bug review
- Supprimer les commentaire inutile
- Mettre à jour les doc avec le content actuelle du projet
```

**Avoid false positives.** Be precise. Do not invent issues to look productive, and do not delete
useful comments (the ones that explain *why*). The default CI bar is `cargo fmt --check`,
`cargo clippy --all-targets` (0 warnings), `cargo test` — stricter pedantic/nursery lints are not the
bar unless the stage says so. Distinguish a real regression from a pre-existing condition and say which.

### 3. Merge and advance
When a review pass **finds nothing real left to fix**, merge the current stage's PR and move to the
next stage. Repeat until Stage 7 is done.

## Definition of done

**v1 (done):** Stages 0–7 merged, CI green, the milestone (Stage 6) reached — live bidirectional sync
with conflict-safe 3-way merge and persisted state.

**Post-v1 (in progress):** Stages 8–12 merged the same way — each a single PR, CI green, its
**Acceptance / tests** section satisfied — extending the shipped v1 without regressing it.
