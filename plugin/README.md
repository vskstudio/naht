# Naht Studio plugin

The thin Luau plugin that closes the live loop: it long-polls the daemon, applies patches to the
DataModel, pushes Studio-side edits back, and shows connection state. **No sync logic lives here**:
mapping, reconciliation, and merge are all in [`naht-core`](../crates/naht-core). This is transport + apply
+ report (architecture §3).

## Layout

| File | Role |
|---|---|
| `src/MessagePack.luau` | Hand-written MessagePack codec for the exact subset the wire uses |
| `src/Protocol.luau` | Build/parse wire messages on top of the codec |
| `src/Client.luau` | HTTP client (`/info`, `/patches` long-poll, `/changes`, `/blobs`, `/ack`, `/heartbeat`) |
| `src/Apply.luau` | Apply a patch to the DataModel through an injectable tree interface |
| `src/Terrain.luau` | Read/write terrain voxels (`ReadVoxels`/`WriteVoxels`) as an opaque binary blob |
| `src/Connection.luau` | Handshake, long-poll + apply, edit push, reconnect with backoff |
| `src/Plugin.server.luau` | Entry: toolbar toggle and a status widget |

## Install

Each Naht release ships `naht-plugin.rbxmx`. From a source checkout, package it yourself:

```sh
naht package-plugin --src plugin/src --output naht-plugin.rbxmx
```

Then in Studio install it as a local plugin (right-click in the Explorer → **Insert from File…**, or
drop it into the local Plugins folder) and enable **Game Settings → Security → Allow HTTP Requests**.

## Use

1. Run the daemon for your project: `naht serve`.
2. Click **Naht Sync** in the toolbar. The status widget shows `Connecting → Connected`, then
   `Reconnecting` if the daemon goes away, and `Conflict` with the path when a merge can't be
   resolved automatically. Synced source mirrors under `ServerStorage/Naht`.

Behaviour notes:

- Turning sync off detaches every Studio listener, drops any edit still waiting to be pushed, and turning
  it back on attaches them once. Nothing is sent after sync is turned off.
- A delete patch removes only the instance of the matching script class (`Script`, `LocalScript` or
  `ModuleScript`) and never creates folders to hold it. A delete for a missing parent does nothing.
- An applied patch is not echoed back. Only a Studio edit that changes a script's source is pushed, and
  reverting a source to the value the daemon last sent is pushed as a real edit.
- An upsert matches an existing child by name and expected class. A same-named script of another class
  is replaced (its children move to the new script). A same-named non-script (a `Folder`, for example)
  blocks the patch: nothing is written and the patch is not acknowledged. A folder and a script with the
  same name cannot both be synced.
- Folders on the way to a file are matched only against non-script containers. A `Script` named `Foo`
  never hides a `Folder` named `Foo`: `Foo/Bar.luau` goes under the folder.
- Instance names containing `/`, `\` or NUL are not synced, because they would split into extra
  folders on disk. A `ModuleScript` whose name ends in `.server`, `.client` or `.luau` is not synced
  either, because its file name would be ambiguous. The plugin warns once per affected path.
- A terrain blob with out-of-range dimensions, a non-positive resolution, or a payload length that
  does not match its dimensions is refused. The cursor is held, the widget shows
  `Warning` with `terrain blob rejected: <reason>`, and the blob is retried with an exponential backoff
  from 2 seconds up to a cap of 60 seconds; the cursor advances only once every blob in a batch has been
  applied.
- A text patch that cannot be applied sets the widget to `Warning` with `patch rejected: <path>: <reason>`.
  It is not acknowledged, so the daemon resends it only after a reconnect. A later patch to the same path that
  applies clears the Warning. When every Warning has cleared, the widget returns to `Connected`.
- Terrain larger than 1,048,576 voxels at 4-stud resolution is not pushed live. The widget shows
  `Warning` with `terrain too large for live push, use naht build`, and the widget returns to `Connected`
  once a push fits under the cap.
- The reconnect backoff restarts (from 1 second) only after a poll completes, so a daemon that
  answers `/info` and then fails on `/patches` keeps backing off.

## Tests

The encode/decode, patch-apply, terrain blob, and connection lifecycle paths run headless under
[lune](https://github.com/lune-org/lune)
(pinned in the repo `aftman.toml`):

```sh
lune run plugin/tests/run.luau
```

The MessagePack cases decode bytes captured from the **real daemon encoder**
(`cargo run -p naht-core --example wire_golden`), so the codec is verified against the actual wire
format rather than only against itself.

## Studio validation checklist

The live loop (HttpService, DataModel change signals, the toolbar UI) cannot run headless, so it is
validated manually. Run each scenario in order against a real Studio session; each lists its expected
observable result, so any tester (or a future automated harness) can follow it deterministically.

Setup: `naht init demo`, `naht serve` in the project, install the plugin, enable HTTP requests.

1. **Connect.** Click **Naht Sync**.
   - *Expected:* the widget shows `Connecting`, then `Connected` with the project name; the project's
     source appears under `ServerStorage/Naht`.
2. **Disk → Studio.** Edit and save `src/Hello.luau` on disk.
   - *Expected:* the matching `ModuleScript`'s source updates in Studio within a moment.
3. **Studio → disk.** Edit that `ModuleScript`'s source in Studio.
   - *Expected:* the file on disk updates to match.
4. **Force a conflict.** Change the same script on both sides before a sync settles (e.g. stop typing
   in neither; edit the file and the Studio instance to different content quickly).
   - *Expected:* the file gets git-style conflict markers, the widget shows `Conflict` with the path,
     `naht status` lists it, and neither side is overwritten. `naht resolve <path>` (after removing
     the markers) clears it and sync resumes.
5. **Terrain sync** (with `[serve] terrain_sync = true` in `naht.toml`). Sculpt terrain in Studio,
   then change the synced `.terrain` blob on disk.
   - *Expected:* terrain changes propagate each way as an opaque voxel blob; a change on both sides
     freezes the blob path like any binary conflict (no silent overwrite).
6. **Reconnect re-diff.** Kill `naht serve` (the widget shows `Reconnecting`), edit a file on disk
   while it is down, then restart `naht serve`.
   - *Expected:* the plugin reconnects with backoff and the daemon re-diffs against its persisted
     state: the offline edit syncs, and nothing already in sync is re-clobbered.
