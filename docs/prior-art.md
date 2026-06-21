# Prior Art — Rojo & Argon teardown

Every Naht decision is grounded in a concrete failure of the two incumbent tools. This is that
analysis. Evidence as of June 2026; items that could not be confirmed from primary sources are marked
**(unverified)**.

## Rojo (the incumbent)

- **Architecture.** Rust CLI (`rojo serve`) is the server; a Luau plugin in Studio is the client. HTTP
  on default port 34872, originally long-polling. Built on the `rbx-dom` ecosystem (`rbx_dom_weak`,
  `rbx_reflection`, `rbx_binary`, `rbx_xml`) and `memofs` (pluggable VFS).
- **Config.** `default.project.json` with sigil keys (`$className`, `$path`, `$properties`). Every
  service is hand-declared; non-default properties on file-backed instances need a separate
  `.meta.json` sidecar — source and properties live apart. Verbose and boilerplate-heavy.
- **Sync.** Primary model is **unidirectional FS → Studio**. Two-way is an opt-in toggle the docs label
  "EXPERIMENTAL!" / "very broken". **No 3-way merge** — it overwrites and shows an accept/reject patch
  dialog.
- **State.** **In-memory only.** The tree lives in `rbx_dom_weak` inside `ServeSession`; there is no
  on-disk last-sync state. Restarting `serve` rebuilds from the FS and loses prior reconciliation
  state.
- **Pain points (cited).**
  - Two-way sync destroys work and crashes: [#933](https://github.com/rojo-rbx/rojo/issues/933) —
    `.unwrap()` on a `PermissionDenied` FS error at `change_processor.rs:229`. Closed **"not planned."**
    Related: [#930](https://github.com/rojo-rbx/rojo/issues/930), [#790](https://github.com/rojo-rbx/rojo/issues/790),
    [#595](https://github.com/rojo-rbx/rojo/issues/595), [#329](https://github.com/rojo-rbx/rojo/issues/329),
    [#978](https://github.com/rojo-rbx/rojo/issues/978).
  - No multi-dev story; syncs on save, not commit: [#394](https://github.com/rojo-rbx/rojo/issues/394).
  - Two-way sync lags Studio: [#344](https://github.com/rojo-rbx/rojo/issues/344).

## Argon (the newer competitor)

- **Architecture.** CLI (Rust) + Roblox plugin + VS Code extension. HTTP via Actix-Web, **MessagePack**
  payloads (chosen over JSON for complex types/perf). Built on `rbx-dom`, `notify-debouncer-full`,
  `crossbeam-channel`. Plugin transport detail (poll interval, port) **(unverified)**.
- **Config.** Rojo-compatible `*.project.json`, plus **layered config** (defaults → `~/.argon/config.toml`
  → `./argon.toml`) — a genuine improvement over Rojo's single file.
- **Sync.** Bidirectional is first-class, with configurable conflict policy (`rename_instances`,
  `keep_duplicates`, `move_to_bin`) — but still **instance-level reconciliation, not 3-way text merge**.
- **State.** No documented on-disk last-sync persistence; assumed in-memory like Rojo **(unverified)**.
- **Pain points (cited).** Property-overwrite confusion ([discussion #94](https://github.com/orgs/argon-rbx/discussions/94),
  maintainer called the UI "misleading"); sync stops when the plugin widget is closed
  ([argon-roblox #3](https://github.com/argon-rbx/argon-roblox/issues/3)); repeated network-reliability
  fixes across 2.0.18–2.0.20 (`NetFail` on Windows).

## Failure mode → Naht decision

| Failure in Rojo / Argon | Naht decision |
|---|---|
| Two-way deletes edits + crashes via `.unwrap()` on FS error | No `unwrap()`/`expect()` on FS/network in the sync loop; a failed write pauses one path, never kills the session |
| Overwrite-on-conflict, no merge | 3-way text merge against a persisted base; unmergeable → git markers + frozen path, never auto-pick |
| In-memory state lost on restart | Persist last-sync base to SQLite (`.naht/state.db`) keyed by GUID |
| Two-way bolted on / experimental | Bidirectional is the core engine, FS and Studio as symmetric peers |
| Connection death silent or fatal | Heartbeat, auto-reconnect, visible connection state, re-diff vs persisted state on reconnect |
| Config verbosity + scattered `.meta.json` | Convention-first, inline property frontmatter, sidecars only as escape hatch |
| No per-machine/per-project config split | Keep Argon's layered config |
| Project file entangles build + sync | Separate build config from sync mapping |
| Live-sync property gaps fail silently | Detect unsyncable properties and warn explicitly + place-file fallback |
| No multi-dev story | Persisted base + 3-way merge as the foundation for commit-oriented sync (post-v1) |

## What they got right — keep it

- The **`rbx-dom` ecosystem** — mature, correct Roblox type handling. Depend on it.
- **Pluggable VFS** (memofs / Argon trait VFS) — makes the reconciler unit-testable in memory.
- **Convention-based directory→instance mapping** — intuitive; keep and extend.
- **Rojo-compatible project import** — meet users where they are (`naht init --from-rojo`).
- **Argon's layered config** and **explicit conflict policy** — right instincts; Naht goes further with
  real 3-way merge + persisted base.
- **`servePlaceId` guard** — cheap, high-value safety against syncing into the wrong game.
- **MessagePack transport** — reasonable for complex types; invest harder in reconnect robustness.

## Sources
[rojo](https://github.com/rojo-rbx/rojo) ·
[DeepWiki rojo](https://deepwiki.com/rojo-rbx/rojo) ·
[Rojo sync details](https://rojo.space/docs/v7/sync-details/) ·
[Rojo project format](https://rojo.space/docs/v7/project-format/) ·
[argon](https://github.com/argon-rbx/argon) ·
[DeepWiki argon](https://deepwiki.com/argon-rbx/argon) ·
[argon.wiki](https://argon.wiki/docs/getting-started/common-usage) ·
[argon discussion #94](https://github.com/orgs/argon-rbx/discussions/94) ·
[argon-roblox](https://github.com/argon-rbx/argon-roblox)
