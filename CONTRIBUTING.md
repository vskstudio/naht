# Contributing to Naht

Issues and pull requests are welcome from anyone. Merging is reserved to the
[VSK Studio maintainers](https://github.com/orgs/vskstudio/teams/maintainers):
every pull request needs the approval of one of them and a green CI before it
lands on `main`.

## Issues

- Search the open issues before filing a new one.
- Use the bug or feature template. A bug report with the exact commands and the
  `naht serve -vv` output gets fixed far faster.
- Security problems go through [SECURITY.md](SECURITY.md), never a public issue.

## Pull requests

1. Fork the repository and branch from `main`.
2. For anything larger than a small fix, open an issue first so the design can
   be agreed before you write the code.
3. Keep one pull request to one change, and link the issue it closes.
4. Run the quality chain locally before you push:

   ```sh
   ./scripts/check.sh
   lune run plugin/tests/run.luau
   ```

5. Open the pull request against `main`. A maintainer reviews it, and CI runs
   once a maintainer has approved the workflow for your fork.

Every review comment has to be resolved before merge, and a new push after an
approval asks for a fresh one.

## Code rules

The house rules are in [docs/code.md](docs/code.md) and are enforced by
`scripts/check.sh`:

- `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `cargo doc`,
  `cargo deny` and `cargo machete` must all pass.
- No plain `//` comments in Rust. Name things so they need none; rustdoc
  (`///`) on public items only.
- No en dash or em dash anywhere in the repository.
- No `unwrap()` or panics on the sync path: a failure pauses one path, it never
  kills the session.

## License

By contributing you agree that your work is dual-licensed under
[MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE), like the rest of the
project.
