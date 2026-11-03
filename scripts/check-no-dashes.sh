#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

offenders=$(git ls-files -z -- ':(exclude)Cargo.lock' ':(exclude)**/package-lock.json' \
    | xargs -0 grep -n -I -P '[\x{2013}\x{2014}]' /dev/null || true)

if [ -n "$offenders" ]; then
    printf '%s\n' "$offenders" >&2
    echo "en dash (U+2013) and em dash (U+2014) are forbidden: use a colon, a comma, parentheses, a full stop or a hyphen" >&2
    exit 1
fi
