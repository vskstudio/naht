#!/usr/bin/env bash
set -euo pipefail

limit=500
offenders=()

while IFS= read -r -d '' file; do
    lines=$(wc -l < "$file")
    if (( lines > limit )); then
        offenders+=("$file: $lines lines (limit $limit)")
    fi
done < <(git ls-files -z -- '*.rs')

if (( ${#offenders[@]} > 0 )); then
    printf '%s\n' "${offenders[@]}" >&2
    exit 1
fi
