# Naht

> The seam between your filesystem and Roblox Studio — bidirectional, conflict-safe, never destructive.

**Naht** (German: *seam / suture*) is a Rust filesystem-sync tool for Roblox Studio. It keeps your
code on disk and your Studio session in lockstep **in both directions at once**, and when both sides
change the same script it does a **real 3-way merge** instead of silently overwriting your work.

It is a from-scratch alternative to [Rojo](https://github.com/rojo-rbx/rojo) and
[Argon](https://github.com/argon-rbx/argon), built around the failure modes that make those tools
painful: experimental/destructive two-way sync, in-memory-only state lost on restart, verbose
configuration, and silent data loss on conflict.

## Why Naht

| Pain in Rojo / Argon | What Naht does |
|---|---|
| Two-way sync is experimental and can delete Studio edits / crash the server | Bidirectional is the core design; no `unwrap()` in the sync loop — a failed write pauses one path, never kills the session |
| Overwrite-on-conflict, no merge | Real **3-way text merge** with a persisted base; unmergeable conflicts get git-style markers and freeze that path until resolved |
| Reconciliation state is in memory and lost on restart | Last-sync state is **persisted to SQLite**, so restarts and reconnects re-diff safely instead of re-clobbering |
| Verbose `default.project.json` + scattered `.meta.json` | **Convention over configuration**, layered config, inline property frontmatter; `naht init --from-rojo` migrates an existing project — name, place id, and the instance tree |
| Live-sync gaps fail silently | Unsyncable properties (CSG, terrain, `MeshId`, locked props) are **detected and reported** with guidance, never dropped; place-file fallback via `naht build` |

## Status

Naht is feature-complete through its staged build plan: the sync engine (`naht-core`), the localhost
daemon and MessagePack protocol, the CLI, the Luau Studio plugin, the limits/hardening pass, and the
post-v1 work — live terrain blob sync, isolated asset-upload failures, Rojo **tree** migration, and a
tagged-release packaging pipeline. The Rust side is covered by tests and the plugin's codec/apply
paths run headless under [lune](https://github.com/lune-org/lune); the live Studio loop is validated
manually against the [Studio validation checklist](plugin/README.md#studio-validation-checklist).

**New here?** Start with the [quickstart](docs/quickstart.md) — zero to a confirmed bidirectional
sync. See [`docs/`](docs/) for the architecture and the staged build plan.

## Usage

```sh
naht init [path]            # scaffold a project (--from-rojo converts a default.project.json)
naht serve [path]           # run the localhost sync daemon (--port to override, -v/-vv for logs)
naht status [path]          # list paths frozen by a conflict
naht resolve <path>         # clear a conflict once its markers are gone (--project <dir> to scope it)
naht build [path] -o out.rbxm   # build a model (.rbxm/.rbxmx) or place (.rbxl/.rbxlx); --watch to rebuild on change
naht pull [path]            # ask a running daemon to re-sync now
naht package-plugin -o naht-plugin.rbxmx   # package the Studio plugin into an installable model
```

Configuration is convention-first; an optional `naht.toml` (layered over `~/.naht/config.toml`)
carries only the exceptions — the project name, the serve port, and the place-id guard. Live
bidirectional sync between the daemon and Studio requires the plugin (Stage 6).

## Documentation

- [`docs/quickstart.md`](docs/quickstart.md) — install, init, serve, first round-trip, conflicts
- [`docs/architecture.md`](docs/architecture.md) — the system design
- [`docs/spec.md`](docs/spec.md) — the staged implementation spec (one stage = one PR)
- [`site/`](site/) — an animated Svelte documentation site (`cd site && npm install && npm run dev`)

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
