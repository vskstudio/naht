# Full-project audit and confirmed-fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Audit all Naht subsystems and fix only defects proven by deterministic reproduction, focused regression coverage, and an explicit existing contract.

**Architecture:** Keep `naht-core` responsible for domain validation and reconciliation, keep the daemon responsible for HTTP policy/capabilities, keep the plugin responsible for protocol compatibility and Studio transport, and keep the site/CI changes limited to confirmed drift or missing gates. Each task is independently tested before the next task begins.

**Tech Stack:** Rust workspace (`cargo fmt`, `clippy`, `cargo test`), Axum/Tokio, Luau/Lune when available, Svelte/Vite/Vitest, GitHub Actions.

## Global Constraints

- No fix without a focused regression test or deterministic reproduction.
- Preserve valid existing sync behavior and wire shapes.
- Do not upgrade dependencies speculatively.
- Do not modify unrelated files or generated `site/dist` output unless the repository workflow requires it.
- `docs/improvement/` contains the audit evidence and confirmed-vs-hypothesis decisions.
- No push, PR, or release publication.

---

### Task 1: Re-audit repository boundaries and establish baseline

**Files:**
- Inspect: `Cargo.toml`, `naht-core/src/`, `naht/src/`, `plugin/src/`, `plugin/tests/`, `site/src/`, `.github/workflows/`
- Modify: `docs/improvement/2026-07-10-full-project-audit.md`

- [ ] Run `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`.
- [ ] Run `npm test`, `npm run build`, and `npm audit --omit=dev --audit-level=high` in `site/`.
- [ ] Check `lune run plugin/tests/run.luau`; record unavailable tooling separately from failures.
- [ ] Inspect every public boundary and record only findings with code-path evidence.
- [ ] Write a repository-wide report separating confirmed findings, rejected hypotheses, and unavailable checks.

### Task 2: Harden project-relative path handling

**Files:**
- Modify: `naht-core/src/vfs.rs`
- Modify: `naht-core/src/reconciler.rs`
- Modify: `naht-core/src/binary.rs`
- Modify: `naht/src/session.rs`, `naht/src/server.rs`
- Test: `naht-core/src/vfs.rs`, `naht/tests/daemon.rs`

- [ ] Add failing tests for absolute paths, `../` traversal, and valid nested paths on both text and blob inputs.
- [ ] Add failing tests proving invalid paths are rejected before writes/removes and valid paths remain unchanged.
- [ ] Implement one shared validation boundary for protocol-originated project-relative paths.
- [ ] Return a client error for invalid request paths without mutating the session or filesystem.
- [ ] Run focused Rust tests, then the full Rust gate.

### Task 3: Enforce the terrain capability flag

**Files:**
- Modify: `naht/src/session.rs`, `naht/src/commands.rs`, `naht/src/server.rs`
- Modify: `plugin/src/Connection.luau`, only if the existing contract requires client-side suppression
- Test: `naht/tests/daemon.rs`, `naht/src/config.rs` or existing config tests

- [ ] Add a failing integration test showing the default configuration does not scan, write, queue, or return terrain blobs.
- [ ] Thread the resolved capability into the session without changing default text sync.
- [ ] Guard blob reconciliation and blob endpoints while disabled.
- [ ] Add a positive test proving enabled terrain sync still round-trips bytes.
- [ ] Run daemon integration tests and the full Rust gate.

### Task 4: Enforce protocol compatibility at handshake

**Files:**
- Modify: `plugin/src/Client.luau`, `plugin/src/Connection.luau` if needed
- Test: `plugin/tests/run.luau`
- Modify: `site/src/content/en/protocol.svx`, `site/src/content/fr/protocol.svx` only if behavior/documentation must be aligned

- [ ] Add a failing compatibility assertion for a mismatched server protocol version.
- [ ] Reject mismatches before any sync request and expose a clear reconnect/error state.
- [ ] Preserve the current matching-version wire behavior.
- [ ] Run the Lune test if available; otherwise perform static Luau validation and record the limitation.

### Task 5: Bound HTTP resource consumption

**Files:**
- Modify: `naht/src/server.rs`
- Modify: `naht/src/config.rs` only if limits are configurable under an existing configuration contract
- Test: `naht/tests/daemon.rs`
- Docs: `site/src/content/en/configuration.svx`, `site/src/content/fr/configuration.svx` only for exposed settings

- [ ] Add failing tests for oversized request bodies and excessive batch entries.
- [ ] Apply explicit Axum body limits and deterministic `413`/`400` responses.
- [ ] Add per-entry limits only where the protocol can enforce them without breaking valid terrain payloads.
- [ ] Run focused HTTP tests and the full Rust gate.

### Task 6: Audit and correct plugin behavior

**Files:**
- Inspect/modify: `plugin/src/MessagePack.luau`, `Protocol.luau`, `Apply.luau`, `Connection.luau`, `Terrain.luau`, `Plugin.server.luau`
- Test: `plugin/tests/run.luau`

- [ ] Compare every encoder/decoder field with `naht-core/src/protocol.rs` and golden wire fixtures.
- [ ] Add regression cases only for mismatches demonstrated by the comparison or runtime harness.
- [ ] Run the complete Lune harness when available and do not alter Studio-only behavior without a reproducible test.

### Task 7: Audit site, docs, CI, release, and dependency drift

**Files:**
- Inspect/modify: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `docs/`, `site/src/content/`, `site/package.json`, `aftman.toml`
- Test: existing site tests/build and workflow syntax/static checks available locally

- [ ] Verify every documented command and endpoint against implementation.
- [ ] Verify English/French content routes and navigation targets.
- [ ] Add only confirmed missing CI coverage, especially the documented Lune gate if the workflow can install the pinned tool reliably.
- [ ] Run site tests/build/audit and document any unavailable remote-only checks.

### Task 8: Final verification and report

**Files:**
- Modify: `docs/improvement/2026-07-10-full-project-audit.md`

- [ ] Run all available gates from a clean working state without hiding unrelated user changes.
- [ ] Run `git diff --check` and inspect the complete diff for false-positive fixes.
- [ ] Mark each finding as fixed, rejected, unresolved, or tooling-blocked with evidence.
- [ ] Do not claim completion until all successful commands and remaining limitations are recorded.

