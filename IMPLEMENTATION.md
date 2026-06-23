# Implementation Workflow

You are the implementing agent for **Naht**. Everything you need is in this repo — do not ask
questions. The design is frozen.

- **Architecture (authoritative):** [`docs/architecture.md`](docs/architecture.md)
- **Staged spec (what to build, in order):** [`docs/spec.md`](docs/spec.md)
- **Why every decision was made:** [`docs/prior-art.md`](docs/prior-art.md)

## Status

**Stages 0–15 are merged and CI-green** — v1 (live bidirectional sync), the post-v1 extension
(observability, place-file build, hardening, terrain blob engine, Open Cloud asset upload), and the
audit follow-up (Stages 13–15) that made the post-v1 behavior actually exercised end to end.

The remaining work is the follow-up in [`docs/spec.md`](docs/spec.md):

- **Stage 16** — reconcile blobs on Studio text edits: `Session::apply_changes()` must call
  `reconcile_blobs()` like `rescan()` does, closing the one asymmetry the Stage 13 audit found.

## The loop

Work **one stage at a time**, in order. v1 was Stage 0 → Stage 7; the post-v1 work was Stage 8 →
Stage 12; the audit follow-up was Stage 13 → Stage 15; the next work is Stage 16. For each stage:

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
next stage. Repeat until the last stage is done.

## Definition of done

**v1 (done):** Stages 0–7 merged, CI green, the milestone (Stage 6) reached — live bidirectional sync
with conflict-safe 3-way merge and persisted state.

**Post-v1 (done):** Stages 8–12 merged — observability, place-file build, hardening, terrain blob
engine, and Open Cloud asset upload.

**Audit follow-up (done):** Stages 13–15 merged the same way — each a single PR, CI green, its
**Acceptance / tests** section satisfied — making the post-v1 behavior actually exercised end to end
without regressing what ships.

**Follow-up (in progress):** Stage 16 merged the same way — one PR, CI green, its **Acceptance /
tests** section satisfied.
