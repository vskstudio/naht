# Naht Studio plugin

The thin Luau plugin that closes the live loop: it long-polls the daemon, applies patches to the
DataModel, pushes Studio-side edits back, and shows connection state. **No sync logic lives here** —
mapping, reconciliation, and merge are all in [`naht-core`](../naht-core). This is transport + apply
+ report (architecture §3).

## Layout

| File | Role |
|---|---|
| `src/MessagePack.luau` | Hand-written MessagePack codec — the exact subset the wire uses |
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

## Tests

The encode/decode and patch-apply paths run headless under [lune](https://github.com/lune-org/lune)
(pinned in the repo `aftman.toml`):

```sh
lune run plugin/tests/run.luau
```

The MessagePack cases decode bytes captured from the **real daemon encoder**
(`cargo run -p naht-core --example wire_golden`), so the codec is verified against the actual wire
format rather than only against itself.

## Studio validation checklist

The live loop — HttpService, DataModel change signals, the toolbar UI — cannot run headless, so it is
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
     state — the offline edit syncs, and nothing already in sync is re-clobbered.
