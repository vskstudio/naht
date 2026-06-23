# Naht quickstart

From zero to a confirmed bidirectional sync between your filesystem and Roblox Studio. For the design
behind it, see [`architecture.md`](architecture.md).

## 1. Install the CLI

**From a release** (no toolchain needed): download the `naht` binary for your platform from the
[GitHub Releases](https://github.com/vskstudio/naht/releases) page and put it on your `PATH`.

- Linux: `naht-x86_64-unknown-linux-gnu`
- macOS: `naht-aarch64-apple-darwin` (Apple silicon) or `naht-x86_64-apple-darwin` (Intel)
- Windows: `naht-x86_64-pc-windows-msvc.exe`

**From source** (needs Rust): `cargo build --release -p naht` produces `target/release/naht`.

Verify it runs:

```sh
naht --version
```

## 2. Install the Studio plugin

Each release also ships `naht-plugin.rbxmx`. (From a source checkout, produce it yourself with
`naht package-plugin --src plugin/src --output naht-plugin.rbxmx`.)

1. In Studio, right-click in the Explorer and **Insert from File…**, or drop the `.rbxmx` into your
   local Plugins folder, to install it as a local plugin.
2. Enable **Game Settings → Security → Allow HTTP Requests** — the plugin talks to the daemon over
   `localhost` HTTP.

## 3. Create a project

```sh
naht init demo
cd demo
```

This scaffolds `src/Hello.luau`, a minimal `naht.toml`, and a `.gitignore` that excludes Naht's
internal `.naht/` state directory. Migrating from Rojo instead? `naht init --from-rojo` reads an
existing `default.project.json` and converts the name, place id, and the instance tree (see
[the configuration notes](architecture.md#6-configuration)).

## 4. Start the daemon

```sh
naht serve          # add -v for debug logs, --port to override the default 34872
```

It serves the project over `http://localhost:34872` and watches the filesystem for changes.

## 5. Connect Studio

Click **Naht Sync** in the toolbar. The status widget walks `Connecting → Connected`, and your
project's source appears under `ServerStorage/Naht`.

> If you set `[serve] place_id` in `naht.toml`, the daemon only connects to the Studio session whose
> place id matches — a guard against syncing source into the wrong game. A mismatch is rejected at
> the handshake.

## 6. Edit on both sides

**Disk → Studio.** Edit `src/Hello.luau` on disk and save. The change appears in the matching
`ModuleScript` in Studio within a moment.

**Studio → disk.** Edit that `ModuleScript`'s source in Studio. The file on disk updates to match.

That is the seam working in both directions at once — no manual push or pull.

## 7. Read and resolve a conflict

If the **same script changes on both sides** before a sync settles and the edits can't be merged
automatically, Naht does **not** overwrite either copy. Instead it:

- writes git-style conflict markers (`<<<<<<<` / `=======` / `>>>>>>>`) into the file on disk,
- **freezes** that path (no further sync until you resolve it),
- shows `Conflict` with the path in the plugin's status widget.

To resolve:

```sh
naht status                 # lists the frozen path(s)
# edit the file, remove the conflict markers, keep the content you want
naht resolve src/Hello.luau # clears the freeze; refuses while markers remain
```

Sync resumes for that path once it is resolved.

## Where to next

- The full command list is in the [README](../README.md#usage).
- The exact manual scenarios that exercise the live loop — including terrain sync and reconnect — are
  in the [Studio validation checklist](../plugin/README.md#studio-validation-checklist).
