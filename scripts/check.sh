#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

run() {
    if [ -n "${CI:-}" ]; then
        "$@"
    else
        systemd-run --user --scope -p MemoryMax=6G -q "$@"
    fi
}

run cargo fmt --all --check
./scripts/check-file-size.sh
./scripts/check-no-comments.sh
run cargo clippy --locked --workspace --all-targets -- -D warnings
run cargo test --locked --workspace
RUSTDOCFLAGS="-D warnings" run cargo doc --locked --workspace --no-deps
run cargo deny check
run cargo machete
