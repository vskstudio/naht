#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

offenders=$(git ls-files -z -- '*.rs' | xargs -0 perl -ne '
    if (!m{^\s*//[/!]}) {
        my $code = $_;
        $code =~ s/"(?:[^"\\]|\\.)*"//g;
        $code =~ s/\x27(?:\\[^\x27]*|[^\x27\\])\x27//g;
        print "$ARGV:$.: $_" if $code =~ m{(?:^|[^/])//(?:[^/!]|$)|^////};
    }
    close ARGV if eof;
')

if [ -n "$offenders" ]; then
    printf '%s\n' "$offenders" >&2
    echo "plain // comments are forbidden in Rust code: use rustdoc on public items (rust-book docs.md)" >&2
    exit 1
fi
