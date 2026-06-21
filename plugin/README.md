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
| `src/Client.luau` | HTTP client (`/info`, `/patches` long-poll, `/changes`, `/heartbeat`) |
| `src/Apply.luau` | Apply a patch to the DataModel through an injectable tree interface |
| `src/Connection.luau` | Handshake, long-poll + apply, edit push, reconnect with backoff |
| `src/Plugin.server.luau` | Entry: toolbar toggle and a status widget |

## Install

1. Build the plugin to a model file with Naht itself:
   ```sh
   naht build plugin/src -o naht-plugin.rbxm
   ```
2. In Studio, install the model as a local plugin (right-click in the Explorer, or use the Plugins
   folder). Enable **Game Settings → Security → Allow HTTP Requests**.

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

The live loop (HttpService, DataModel change signals, the toolbar UI) is validated manually in
Studio per the Stage 6 acceptance: editing a file updates Studio live and vice-versa, a conflict
surfaces in the widget and freezes, and killing/restarting the daemon reconnects without data loss.
