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
| `src/Client.luau` | HTTP client (`/info`, `/patches` long-poll, `/changes`, `/inventory`, `/blobs`, `/ack`, `/heartbeat`), sends the `x-naht-client` and `x-naht-place-id` headers on every request, and `x-naht-token` once the token is known |
| `src/Apply.luau` | Apply a patch to the DataModel through an injectable tree interface |
| `src/Terrain.luau` | Read/write terrain voxels (`ReadVoxels`/`WriteVoxels`) as an opaque binary blob |
| `src/Connection.luau` | Handshake, long-poll + apply, reconnect with backoff, posts the inventory on attach |
| `src/TerrainSync.luau` | Terrain push and the `/blobs` channel, used only when `terrain_sync` is on |
| `src/Placement.luau` | Maps DataModel scripts to file paths, queues renames, moves and removals, builds the inventory |
| `src/PushQueue.luau` | Batches queued edits into `POST /changes`, retries after a failure and drops the paths the daemon refuses |
| `src/Plugin.server.luau` | Entry: toolbar toggle, a status widget, and the token box shown in `TokenRequired` |

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
- A rename of an instance, or a move of it to another parent inside the synced tree, is sent as one batch: a
  `Delete` of the old path and an `Upsert` of the new path. Renaming a folder sends that pair for every script
  under it, in the same batch. The plugin records the path the daemon holds for each instance, so a second
  rename deletes the file the daemon actually has. If the daemon refuses the new path, the old file is kept and
  the whole move is dropped, never half-applied.
- With `[serve] terrain_sync = true`, a sculpt in Studio is pushed when a `ChangeHistoryService` waypoint is
  added. The push waits one second after the last waypoint, so a burst of strokes sends one blob. A voxel blob
  equal to the last one pushed or read back is never sent, and a blob the plugin has just applied from the daemon
  is never pushed back. Failures, backoff and the 1,048,576 voxel cap are the same as for the initial snapshot.
- A terrain blob for any path other than `Terrain.terrain` is skipped with a `Warning`,
  `terrain blob for <path> skipped, only Terrain.terrain is synced`. Its content is never written over the
  terrain, and the blob cursor advances past it.
- An upsert matches an existing child by name and expected class. A same-named script of another class
  is replaced (its children move to the new script). A same-named non-script (a `Folder`, for example)
  is a sibling and never blocks the patch: `Foo/` and `Foo.luau` coexist, the way the daemon writes them.
  Only a Studio error while writing the source rejects a patch.
- Folders on the way to a file are matched only against non-script containers. A `Script` named `Foo`
  never hides a `Folder` named `Foo`: `Foo/Bar.luau` goes under the folder.
- Instance names containing `/`, `\`, `:`, `< > " | ? *` or a control byte (0x00 to 0x1F, NUL included), and folder names that are empty, `.` or `..`, are not synced,
  because the daemon refuses them as project paths. The daemon splits a path on both `/` and `\`, so an inner `\`
  would become nested folders on disk, and the plugin refuses every name that contains one. The daemon itself
  accepts an inner `\` as a separator and rejects a leading one or one that leaves an empty folder name, so this rule is stricter than the daemon on purpose. A `ModuleScript` whose name ends in `.server`, `.client` or `.luau` is not synced
  either, because its file name would be ambiguous. The plugin warns once per affected path.
- A terrain blob with out-of-range dimensions, a non-positive resolution, or a payload length that
  does not match its dimensions is refused. The cursor is held, the widget shows
  `Warning` with `terrain blob rejected: <reason>`, and the blob is retried with an exponential backoff
  from 2 seconds up to a cap of 60 seconds; the cursor advances only once every blob in a batch has been
  applied.
- A text patch that cannot be applied sets the widget to `Warning` with `patch rejected: <path>: <reason>`.
  It is not acknowledged, so the daemon resends it only after a reconnect. A later patch to the same path that
  applies clears the Warning. When every Warning has cleared, the widget returns to `Connected`, or to `Conflict` while a path is still in conflict. A Conflict takes precedence over a Warning, because it needs `naht resolve` and a Warning may never clear in the session.
- Terrain larger than 1,048,576 voxels at 4-stud resolution is not pushed live. The widget shows
  `Warning` with `terrain too large for live push, use naht build`, and the widget returns to `Connected`
  once a push fits under the cap.
- The initial terrain snapshot is retried with an exponential backoff from 1 second up to a cap of 60 seconds
  until the daemon accepts it. Each failed attempt sets the widget to `Warning` with `terrain push failed: <reason>`,
  and the next success clears it. Stop or a reconnect ends the retry.
- If polling the terrain blob stream fails while sync is running, the widget shows `Warning` with
  `terrain blob poll failed: <reason>`, and the poll is retried with an exponential backoff from 2 seconds up
  to a cap of 60 seconds. The next successful poll clears the Warning. When five batches in a row fail to decode at
  the same cursor, the Warning becomes `terrain blob batch at cursor <n> keeps failing to decode, still retrying`.
  Only a decode failure reported by the client counts toward that, never a plain error that happens to mention a
  batch. The cursor is never skipped, so the poll keeps backing off, and a later success clears it.
- The reconnect backoff restarts (from 1 second) only after a poll completes, so a daemon that
  answers `/info` and then fails on `/patches` keeps backing off.
- A studio edit whose push fails transiently (a 408, 502, 503 or 504, or a network failure) keeps its batch.
  It is retried with an exponential backoff from 2 seconds up to a cap of 60 seconds, and the widget shows
  `Warning` with `push failed: <reason>` until a push succeeds. These statuses are retried without a limit.
- A 500 is retried the same way, but only until it has come back three times with no accepted post in between.
  Only an accepted post resets that count: a 503 or any other transient failure between two 500s does not. On the
  third 500 the batch is dropped, not merged back, and the widget shows a `Warning` for each of its paths as
  `not synced, the daemon kept failing on <path>: <reason>`.
- Any other refusal of a push (a 400, for example) is not retried as a batch. The daemon names the paths it
  refused in a 400 body as `{"refused_paths": [...]}`. Those paths are dropped, each with a `Warning` as
  `not synced, the daemon refused <path>: <reason>`, and the rest of the batch is posted again straight away.
- A 400 that names no path is not dropped as a whole. The batch is posted again one edit at a time, and only an
  edit the daemon refuses on its own is dropped with the same `Warning`. A single unnamed refusal drops that edit
  the same way.
- An edit made while a post is in flight, including one made during a refused post, is not lost. It waits for
  the post to finish and is sent with the next batch, after the refusal has been handled.
- Dropped edits are never merged back. Each one's `Warning` clears when a later edit to the same path is accepted,
  and otherwise stays until Stop. Stop also discards any edit still waiting to be pushed.
- A push waits for any earlier push still in flight, including one from a connection that was stopped and started
  again. Each post starts a 30-second watchdog that does not depend on any later edit. A post that gets no reply
  before it fires is abandoned: the widget shows `Warning` with `push abandoned: no reply for 30 s, the batch is sent
  again`, the batch of that post is merged back into the pending edits, and a push is scheduled straight away. An
  edit made since then for the same path wins over the restored one. A post that keeps hanging is re-sent once per
  30 seconds, with the same Warning each time, until three abandonments in a row. The third one stops the automatic
  re-send: the batch stays in pending, and the widget shows `Warning` with `push held: the daemon is not answering,
  the batch is sent on the next edit or reconnect`. The next Studio edit or a reconnect sends it. Any accepted post
  resets the count. A late success from an abandoned post also resets it, since that proves the daemon answers, even
  while a newer post is still hung. Stop does not cancel a post in flight; only a reply or the watchdog
  does. A late reply from an abandoned post is ignored by the plugin's state, and the re-sent batch is an idempotent
  duplicate of it. A late reply can still land on the daemon after a newer post for the same path, so the daemon can
  end up holding the older content for that path until the next edit to it.
- An ack that fails transiently (the same statuses as a push, or a network failure) is retried the same way with
  `ack failed: <reason>`, and its paths stay unacked until the ack succeeds. An ack the daemon refuses with any
  other status is not retried: the widget shows `Warning` with `ack refused: <reason>` until Stop, and those
  paths are dropped from the unacked list.

Every request carries `x-naht-client: plugin` and `x-naht-place-id` (the id of the open place). The
daemon answers `403` to a request without the client header, with a body that starts with the token
`outdated-plugin`, and to a place id that differs from the project's `servePlaceId` when one is set. A plugin
built for protocol version 1 sends no client header and does not know that token: it keeps showing
`Reconnecting` with the 403 reason, so update the plugin to match the daemon (protocol version 2). Only a
plugin built for version 2 stops at `Disconnected` with the update message.

From protocol version 4 every request except `GET /info` also carries `x-naht-token`. The daemon answers `401`
to a request without that header or with another value, with the body `{"error": "token required"}` or
`{"error": "token rejected"}`. The plugin then stops at `TokenRequired`: the widget shows a text box, and the
user pastes the token that `naht token` prints and presses Enter. The plugin stores it in its plugin settings
under `naht.token.<projectId>`, so the user is asked once per project. A rejected token returns the widget to
the same state. A plugin built for version 3 stops at its update message, because the handshake refuses any
other version.

## Tests

The encode/decode, patch-apply, terrain blob, and connection lifecycle paths run headless under
[lune](https://github.com/lune-org/lune)
(pinned in the repo `aftman.toml`):

```sh
lune run plugin/tests/run.luau
```

The MessagePack cases decode bytes captured from the **real daemon encoder**, so the codec is
verified against the actual wire format rather than only against itself. The `GOLDEN` table in
`plugin/tests/run.luau` is the single source of truth for those bytes. The Lune harness reads
`GOLDEN` directly and is the authority on whether a pin is live: a pin that exists only inside a
`--[[ ]]` block leaves `GOLDEN.server_info` nil, and the harness fails on it. The Rust tests
`encodes_to_the_bytes` in `crates/naht-core/src/protocol.rs` are a cross-check, not a second source:
they read the pins from that file at compile time and compare the encoder output against them. After
a deliberate encoding change, run `cargo test -p naht-core encodes_to_the_bytes`, copy the `left` hex
from the failure, and paste it over the matching `GOLDEN` entry in `plugin/tests/run.luau`.

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
     the markers) clears the freeze and asks a running daemon to re-sync, so the resolved file reaches
     Studio at once. If the daemon does not answer within 2 seconds, `naht resolve` says the file was not
     re-synced.
5. **Terrain sync** (with `[serve] terrain_sync = true` in `naht.toml`). Sculpt terrain in Studio,
   then change the synced `.terrain` blob on disk.
   - *Expected:* terrain changes propagate each way as an opaque voxel blob; a change on both sides
     freezes the blob path like any binary conflict (no silent overwrite). A sculpt reaches the `.terrain`
     file on disk about a second after the last stroke, without a reconnect.
6. **Reconnect re-diff.** Kill `naht serve` (the widget shows `Reconnecting`), edit a file on disk
   while it is down, then restart `naht serve`.
   - *Expected:* the plugin reconnects with backoff and the daemon re-diffs against its persisted
     state: the offline edit syncs, and nothing already in sync is re-clobbered.
7. **Rename in Studio.** Rename a synced `ModuleScript`, then rename it again.
   - *Expected:* after each rename the old file is gone from disk and the new file holds the same source. Only
     one file exists for the script at any time, and a folder rename moves every script under it.
   - *Expected when refused:* a rename onto a path another script already holds, or onto a Windows device name such as `CON`, is refused with a warning that names the path. The file on disk is not overwritten, and the script keeps the file it had.
8. **Rename while the plugin is stopped.** Stop the plugin (turn Naht Sync off, or close Studio), rename a synced `ModuleScript` in the place, then turn the plugin on again.
   - *Expected:* on attach the plugin posts its listing, and the daemon removes the old file and writes the new one with the same source. Against a daemon that predates `/inventory`, the widget shows a warning to update naht, and the old file stays until naht is updated.
