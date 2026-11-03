# Code layout

The repository is one Cargo workspace. Every crate lives under `crates/` and
is named by its kind in the [rust-book](https://github.com/vskstudio/skills)
sense: the rules of a kind apply on top of the common base.

| Crate | Kind | Notes |
|---|---|---|
| [`crates/naht-core`](../crates/naht-core) | shared library | The brain: VFS, path mapping, reconciliation, 3-way merge, state store and wire protocol. Zero network I/O, linked by the CLI and the daemon. |
| [`crates/naht`](../crates/naht) | command line tool, with a library half | The `naht` binary (CLI and `serve`). Its library half (`lib.rs`) holds the commands, the watcher and the HTTP server, so they can be tested without the binary. `serve` is a daemon: it follows the daemon rules (bounded inputs, graceful shutdown on `SIGINT` and `SIGTERM`, configuration validated once at start). |

The Luau plugin under [`plugin/`](../plugin) is not a Cargo crate. Its tests
run with `lune run plugin/tests/run.luau`.

## Quality chain

`scripts/check.sh` runs what CI runs, in the same order: `cargo fmt --all
--check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
`cargo test --locked --workspace`, and `RUSTDOCFLAGS="-D warnings" cargo doc
--locked --workspace --no-deps`. The toolchain is pinned in
`rust-toolchain.toml`.
