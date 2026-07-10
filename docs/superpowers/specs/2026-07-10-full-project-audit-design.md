# Full-project audit and confirmed-fix design

## Goal

Audit every tracked subsystem in Naht and correct only defects that are demonstrated by code-path analysis plus a focused regression test, while reporting unconfirmed concerns without changing behavior.

## Scope

1. `naht-core`: VFS, mapping, frontmatter, reconciliation, binary/asset handling, protocol, state, build, limits.
2. `naht`: daemon HTTP boundary, session state, CLI/configuration, watcher/uploader.
3. `plugin`: MessagePack/protocol, connection, apply, terrain, Studio entrypoint and headless tests.
4. `site`: Svelte docs reader, content parity, tests/build/dependency checks.
5. CI, release workflows, repository docs, and documented commands.

## Evidence rule

A finding becomes a fix only when all three are present:

- a concrete data/control-flow path showing the defect;
- a deterministic reproduction or failing focused test;
- an unambiguous intended behavior from existing code, tests, or documentation.

Potential issues that fail any one condition are recorded as hypotheses with confidence and are not modified.

## Correction strategy

- Add or strengthen the smallest regression test first.
- Implement one root-cause fix at a time, keeping existing public behavior unless the documented contract is clearly violated.
- For security boundaries, reject invalid input explicitly and preserve valid-path behavior.
- Keep unrelated refactors out of the change set.
- Run the relevant subsystem gate after each batch, then the complete repository gate before completion.

## Expected high-risk checks

- Project-relative path validation for text and blob inputs, including absolute paths, `..`, symlinks, deletes, and writes.
- Actual enforcement of the `terrain_sync` opt-in.
- Protocol-version mismatch behavior at the handshake.
- HTTP body, batch, path, text, and blob size limits.
- Ack/cursor semantics, reconnect behavior, conflict freezing, and deletion safety.
- Plugin wire compatibility and Luau headless coverage.
- Site content routes, English/French parity where promised, dependency/audit status, and workflow drift.

## Non-goals

- No speculative redesigns or feature additions without a demonstrated defect.
- No automatic dependency upgrades solely because newer versions exist.
- No formatting-only churn outside files touched by confirmed fixes.
- No push, PR, or release publication from this pass.

