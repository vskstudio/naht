# Naht: Architecture

This document is the authoritative design. It is written so an implementer needs no outside context:
every component, boundary, and decision is stated here. The build order is in [`spec.md`](spec.md).

## 1. One sentence

Naht keeps a filesystem tree of Roblox source and a live Studio DataModel in **bidirectional sync**,
using a **persisted last-sync base** to do **real 3-way merges** and to **never silently overwrite**
either side.

## 2. Hard constraints (the ceiling no tool can beat)

These come from the Roblox Studio plugin API and shape every decision:

1. **Studio plugins run Luau only**: no Rust/WASM/native inside Studio.
2. **No filesystem access from inside Studio**: all disk I/O lives in the external daemon; the
   plugin only speaks HTTP to `localhost`.
3. **`HttpService` is request/response only**: no server push, no WebSocket client. The plugin must
   *initiate* every exchange (long-poll). The daemon can never spontaneously push to Studio.
4. **Property/type coverage is incomplete**: CSG geometry, `MeshPart.MeshId` binary data,
   and security-locked properties like `HttpService.HttpEnabled` cannot round-trip live. Terrain is the
   exception: it crosses as an opaque voxel blob on its own channel. See
   [§9 Limits & workarounds](#9-limits--workarounds).
5. **Assets (meshes, images, audio) are cloud uploads, not files**: a sync tool references an asset
   id; it does not sync pixels.
6. **Conflict is inevitable**: two live editors (FS + Studio) means a merge/conflict policy is
   mandatory, not optional.

## 3. Components

A Cargo workspace plus a hand-written Luau plugin.

### `naht-core` (library): the brain, zero network I/O
- **VFS**: a pluggable virtual filesystem trait (real disk or in-memory) so the reconciler is unit-
  testable without touching the disk.
- **PathMapper**: convention-based mapping between files and the Roblox instance tree, plus the
  optional config overlay (see §6).
- **Middleware**: converts a file to an instance `Snapshot` and back, using the
  [`rbx-dom`](https://github.com/rojo-rbx/rbx-dom) ecosystem (`rbx_dom_weak`, `rbx_reflection`,
  `rbx_binary`, `rbx_xml`). We depend on it; we do not reinvent Roblox type handling.
- **Reconciler**: diffs the current tree against the persisted last-sync base and produces a set of
  patches, in either direction.
- **Merge**: 3-way text merge for scripts (base = last-sync content); conflict detection for binary.
- **State store**: the SQLite-backed persisted base (see §5).
- **Protocol types**: the messages exchanged with the plugin, defined with `serde` so the wire
  encoding is a swappable layer.

All `naht-core` errors are typed with `thiserror`. No `unwrap()`/`expect()` on fallible I/O.

### `naht` (binary): CLI + daemon
- The localhost HTTP server (`axum`) that the plugin talks to.
- The file watcher (`notify-debouncer-full`).
- Session orchestration and ownership of the SQLite database.
- All CLI commands: `init`, `init --from-rojo`, `serve`, `pull`, `build`, `status`, `resolve`.
- `anyhow` at this boundary for ergonomic error context.

### `plugin/` (Luau): the hands, kept deliberately thin
- Long-polls the daemon, applies received patches to the DataModel, POSTs Studio-side edits back.
- Renders connection state (connected / reconnecting / conflict).
- Encodes/decodes MessagePack.
- **No sync logic** lives here. Mapping, reconciliation, and merge all live in `naht-core`. The
  thinner the plugin, the fewer the bugs: this is exactly where Rojo/Argon went wrong.

## 4. Data flow

**FS → Studio**
1. Watcher detects a file change → VFS.
2. Middleware turns the file into a `Snapshot`.
3. Reconciler diffs the snapshot against the persisted base → patch.
4. Patch is queued; the plugin's next long-poll picks it up.
5. Plugin applies it to the DataModel and acks; the daemon advances the base.

**Studio → FS**
1. Plugin detects a DataModel change → POSTs it to the daemon.
2. Middleware turns the instance change into file content.
3. Reconciler diffs against the persisted base.
4. Daemon writes the file (or raises a conflict); on success it advances the base.

**Conflict** (both sides changed since the base)
- Detected by comparing content hashes against the SQLite base.
- **Text:** 3-way merge with the last-sync content as the base. Clean merge → written, base advanced.
- **Unmergeable text:** git-style markers (`<<<<<<< FS` / `=======` / `>>>>>>> Studio`) written into
  the file, the path marked *conflicted* in SQLite, and sync for that path **frozen** until
  `naht resolve` confirms the markers are gone.
- **Binary:** file-level conflict, never auto-merged.

## 5. State store (SQLite)

The persisted last-sync base, the thing neither Rojo nor Argon keeps, lives in `.naht/state.db`.

Per instance, keyed by stable GUID:

| column | purpose |
|---|---|
| `guid` (PK) | stable instance identity across sessions |
| `path` | filesystem path it maps to |
| `class` | Roblox class name |
| `content_hash` | fast change detection |
| `base_content` (BLOB) | last-synced content: the **base** for 3-way text merge (text only) |
| `mtime` | filesystem modification time |
| `conflicted` | whether this path is frozen pending resolution |

A `meta` table holds the schema version and project identity. Writes are transactional. For binary
instances only the hash is stored (no merge base).

## 6. Configuration

- **Convention first.** Zero config for the common case:
  - `*.server.luau` → `Script`, `*.client.luau` → `LocalScript`, `*.luau` → `ModuleScript`
  - `init.luau` (and `init.server.luau` / `init.client.luau`) → the containing directory *becomes*
    that instance
  - a plain directory → `Folder`
- **Optional `naht.toml`** only for exceptions, applied as a **layered** config:
  defaults → `~/.naht/config.toml` (per-machine) → project `naht.toml`.
- **Limits** are the `[limits]` table: four caps the daemon enforces, for live terrain voxels, the `/inventory`
  body, the mass-delete warning and the connection count. Each has a default and a ceiling in
  `crates/naht/src/config/limits.rs`, and a zero or out-of-ceiling value stops `naht serve` before it takes the
  instance lock. The configuration page of the site lists the keys.
- **Inline property frontmatter**: non-default properties live with the source as a leading
  directive comment (`--!naht { Disabled = true }`), instead of Rojo's separate `.meta.json` files.
- **Build config is separate from sync mapping**: a sync-mapping change must never break a
  reproducible build.
- **`naht init --from-rojo`** reads an existing `default.project.json` and migrates it: the project
  name, place id, and the **instance tree**: each Rojo `$path`/`$className`/`$properties` is
  translated into Naht's convention layout, recording only the genuine exceptions as `[[tree]]`
  entries in `naht.toml` (and reporting anything Naht cannot represent). Source files are left in
  place; `naht build` assembles the instance tree from those mappings, so the build matches Rojo.

## 7. Transport & protocol

- **HTTP on `localhost`** (forced by constraint §2.3), bodies encoded as **MessagePack**: binary,
  handles complex Roblox types, with usable Luau libraries. JSON is avoided for payloads; the encoding
  is isolated behind the `serde` protocol types so it can be swapped (e.g. protobuf) later without a
  rewrite. We do not pursue gRPC (needs HTTP/2, unsupported by `HttpService`).
- **Endpoints:** `GET /info` (handshake), `GET /patches` (long-poll, held
  open until a change or timeout), `POST /changes` (Studio → FS), `POST /inventory` (the plugin's full script listing, once per session on attach), `GET /blobs` + `POST /blobs` (the
  binary terrain channel: a separate long-poll/push pair, kept apart from the text patch channel
  because blobs are synced opaquely with no diff/merge), `POST /ack` (the plugin confirms, by queue
  sequence, which patches it applied; an acked patch leaves the queue), `GET /heartbeat` (liveness ping), `GET /activity` (read-only live sync feed:
  a `pending` count plus a bounded ring buffer of recent completed sync events with a `since`
  cursor, for an external consumer; additive, no protocol-version bump).
- **Sessions (protocol 3):** `GET /patches`, `GET /blobs` and `POST /ack` take a `session` query parameter holding
  the session id from `GET /info`. Without it the daemon answers `400`. With another session's id (the daemon
  restarted since the plugin read it) it answers `409`, the plugin re-handshakes, and the request changes nothing.
  An ack from a previous session settles no patch.
- **Inventory (protocol 3, additive):** `POST /inventory?session=<id>` takes `{scripts: [{path, class, hash}]}`, where `path` is the path the plugin placed the script at (its reconciled path, not the Studio name when a rename was refused), `class` is the script class and `hash` is the source fingerprint (`naht_core::hash::source_fingerprint`, mirrored in `plugin/src/Fingerprint.luau`). The plugin posts it once per session, on attach. The daemon applies only rename pairs: a listed path the mirror lacks is paired with an unlisted mirror path of the same class and fingerprint, and the pair becomes a delete of the old path plus an upsert of the new one, so a rename made while the plugin was stopped leaves no old file. A listed path the mirror holds with another class or content is left alone, and creates and deletes without a partner are left alone and counted in one info line. A path with a queued or unacked patch is skipped, an empty listing changes nothing, and another session's id is answered `409`. A listed path the daemon refuses (`400`, naming it) fails the post, and the plugin reports it as a warning and carries on to `/patches`. A body over the 64 MiB limit is answered `413`, which the plugin also reports once per session without retrying. The route is additive: no protocol-version bump, and a daemon that predates it answers `404`, which the plugin reports as a warning.
- **Trust boundary:** one middleware gates every route before any handler runs, and answers `403`
  when the request carries an `Origin` header, when its `Host` is not exactly one of `127.0.0.1:<port>`,
  `localhost:<port>` or `[::1]:<port>` (this stops a web page or a DNS-rebinding host from reading or
  writing through the daemon), when `x-naht-client` is missing or empty (a custom header forces a browser
  preflight that the daemon never answers), or, when `servePlaceId` is set, when `x-naht-place-id` is
  missing, malformed or differs. Without `servePlaceId`, a request is also refused with `403` when its
  `x-naht-place-id` is malformed, when a `POST` omits the header once the project is bound to a place, or when
  the header names another place than the bound one (`bound to place N, not M`). A header naming place `0`, an
  unpublished place, is refused once the project is bound and accepted without binding while it is not (§11).
  The 403 body names the reason, and for a missing client header it starts with the token `outdated-plugin`. `POST /changes` accepts only visible paths the mapper turns into a Studio script: a batch
  with any refused change is refused whole, answered `400` with `{"refused_paths": [...]}` naming every refused
  path, and nothing in it is written; a batch with every change accepted is answered `200` with an empty body.
  `POST /blobs` accepts only `Terrain.terrain` at the project root; a batch naming any other path is `400`
  with a JSON body `{"refused_paths": [...]}` naming each one, and writes nothing. At most two POST requests
  read and handle a body at once: each holds a slot from the
  start of its body read until its handler returns, so a third POST waits for a slot, with no time limit of
  its own. Each body is capped at 64 MiB, or at `[limits] inventory_body_bytes` for `POST /inventory` (up to 128 MiB), and is answered `413` when it is larger. The 15-second timer covers only
  the read of the body: a body that stalls past it is answered `408`, and a body that ends early or is
  malformed is answered `400`. Time spent by a handler on the session is not covered by that timer. A request
  head that stalls for ten seconds is dropped. The raw body bytes held at once are therefore at most 128 MiB with the default limits, and at most 256 MiB when `inventory_body_bytes` is at its ceiling and two inventory bodies are read at once.
- **Token (protocol 4):** every route except `GET /info` requires the `x-naht-token` header. A request without it, or with another value, is answered `401` with `{"error": "token required"}` or `{"error": "token rejected"}`. The check runs after the client-header check and before the place binding, so a request with a wrong token binds no place. The token is 32 random bytes written as 64 lowercase hex digits in `.naht/token`, created by the first `naht serve` of a project and reused after. On Unix the file is created with mode 0600, so only the owning account can read it, and an existing file readable by its group or others is tightened to 0600 when the daemon loads it, with one warning that never includes the token. On Windows the file inherits the access rules of the `.naht` directory, and the daemon sets no further restriction there. `naht token` prints it, and the comparison is constant time. `GET /info` stays open so the plugin can learn the project id and whether a token is required; it returns no source.
- **What the gate is not:** it stops browsers and DNS rebinding, and on Unix the token keeps out every other local account, since that account cannot read `.naht/token`. On Windows the access to `.naht/token` is whatever the `.naht` directory grants, and no further restriction is claimed. It does not stop a process that runs as the same account: that process can read the token file, and it can read and write the project directly, so the operating system account is the boundary that protects the project. The place guard is a safety check against syncing into the wrong game, not authentication. The threat model is stated in [`SECURITY.md`](../SECURITY.md).
- **Header timeout and keep-alive:** the ten-second header timeout also closes an idle keep-alive
  connection after ten seconds. Clients must not depend on connection reuse: `Client.luau` makes no
  assumption about it, and a request that finds its connection closed is retried by the connection loop.
- **Connection cap:** the accept loop of `naht serve` holds at most `[limits] connections` connections at once (64 by default, the same number as the in-flight limit). A slot is taken before a connection is accepted and freed when the connection ends, so a connection past the cap is not accepted: it waits in the kernel's listen backlog, which holds no process memory, and is served once a slot frees (`a_connection_past_the_cap_waits_until_a_slot_frees_then_is_served`). The accept loop does not sniff the protocol version, so every connection is HTTP/1 from its first byte: a connection that sends nothing, or that opens with an HTTP/2 preface, is closed by the header timeout within ten seconds and frees its slot (`a_silent_connection_frees_its_slot_at_the_header_timeout`, `an_http2_preface_frees_its_slot_at_the_header_timeout`). An idle keep-alive connection is closed by the same timeout, so it frees its slot within ten seconds of going idle. The plugin needs a handful of connections for its long-polls and posts, far below 64. What remains open is in §11.
- **Protocol version 2:** the `x-naht-client` and `x-naht-place-id` headers are required on every request
  and came with version 2. A plugin built for version 1 sends no client header, so every route answers it
  `403`, and that plugin does not recognise the token: it keeps showing `Reconnecting` with the 403 reason
  and must be updated. The clean stop at `Disconnected` with the update message applies only to plugins
  built from version 2, which match the `outdated-plugin` token.
- **Protocol version 3:** `PatchBatch` entries are `SequencedPatch {seq, patch}` and `POST /ack` takes `seqs`
  in place of the `paths` version 2 sent, so an acked patch is matched by its queue sequence. The handshake
  refuses any other version, so a version 2 plugin or daemon stops at the update message instead of acking
  paths the other side no longer reads. `GET /patches`, `GET /blobs` and `POST /ack` also require the `session` query parameter. Upgrade the CLI, the daemon and the plugin together.
- **Protocol version 4:** every route except `GET /info` requires the `x-naht-token` header, and `ServerInfo` reports `project_id` and `token_required`. The handshake refuses any other version, so a version 3 plugin stops at its update message before it sends a request that needs the token. Upgrade the CLI, the daemon and the plugin together.
- **Resilience** is the headline feature here, because it is exactly what Rojo and Argon get wrong:
  heartbeat, automatic reconnect, an explicit connection-state indicator, and: on reconnect: a
  **re-diff against the persisted state** rather than a blind re-push. Studio-bound patches are
  **ack-gated**: a patch's base advances only once the plugin acks its queue sequence, so a half-applied
  batch re-diffs the rest instead of treating them as synced. An acked patch leaves the queue, and an ack
  for a sequence the daemon has since superseded confirms nothing.
- **Retry backoff** is per channel. The text connection loop waits 1 s after a failed handshake or
  poll, doubles the wait on each further failure, and caps it at 15 s. A batch the plugin rejects (a patch that does not apply) is re-polled from the rejected patch after a wait that starts at 1 s, doubles on each further rejected batch, and caps at 60 s, the same cap as the blob channel; a batch with no rejection resets it to 1 s. The terrain blob loop
  (`_serveBlobs` in `plugin/src/Connection.luau`) runs apart from it: a failed `GET /blobs` poll, or a
  batch that does not apply, is retried at the same cursor after a wait that starts at 2 s, doubles, and
  caps at 60 s. A cleanly applied batch resets that wait to 2 s. The `/blobs` cursor is the ack: the daemon drops each queued blob patch below the cursor a poll sends, and the plugin keeps its blob cursor across a reconnect to the same daemon session. After five consecutive undecodable
  responses the blob channel shows a `keeps failing to decode, still retrying` Warning, and it keeps
  retrying: it never gives up while the connection runs.
- **A frozen blob is cleared by `naht resolve --keep disk|studio`.** A freeze stores the Studio hash as the base, so the record names what Studio holds. `--keep disk` is refused when the disk file is missing; otherwise it clears the conflict and stores a keep-disk base. The next reconcile pushes the disk bytes to Studio when they differ from Studio's, then records the disk hash as the base. `--keep studio` records the disk hash as the base, so the next reconcile writes the Studio bytes over the file; when the file is missing, the record is dropped and the next reconcile recreates the file from Studio. A binary conflict needs `--keep`, and a text file refuses it (`a_binary_conflict_needs_a_side_and_a_text_file_refuses_one`).
- **Long-poll wake-ups never race the queue.** A parked `/patches` or `/blobs` request enrols its
  `Notified` future *before* it checks the queue, and `enable()` registers that waiter immediately. A
  change that enqueues and notifies between the check and the await is then captured, not lost.
  Without `enable()`, the future only registers on its first poll, so the wakeup can slip through and
  the request stays parked for the full timeout although a patch is ready. Both channels follow this
  same discipline.

## 8. Error handling

- **No `unwrap()`/`expect()` on any FS or network operation in the sync loop.** This is the literal
  root cause of Rojo's data-loss crashes (`.unwrap()` on `PermissionDenied`).
- A failed write **pauses the affected path** and surfaces the error; it never kills the session and
  never discards the pending change.
- A source file that cannot be read or decoded (non-UTF-8, or a symlink) is skipped and logged as a warning; the rest of the tree still syncs. A symlink, live or dangling, is recorded as skipped and is never read through, so its target never reaches Studio.
- A `POST /changes` batch whose valid changes fail to apply is answered `500`, and nothing it did survives. It runs inside a transaction that journals each disk write (the previous content, or its absence, and any parent directory it created), each base record it changes and each Studio or blob mirror entry it writes. On failure, or on a panic, those are restored in reverse order, so a later batch cannot replay it and a rescan finds nothing to do (`a_failed_batch_leaves_no_disk_write_behind`, `a_panic_inside_a_batch_restores_the_state_it_had_changed`). `POST /blobs` runs in the same transaction, so a failed blob batch leaves no blob on disk or in the mirror (`a_failed_blob_batch_leaves_no_blob_on_disk_or_in_the_mirror`). The reconciler removes files only. After a batch's deletes, and after the text deletes a rescan makes, each directory above a deleted path is removed only when it is left empty, deepest first (`remove_if_emptied`), and the removal is journaled so a rollback recreates it. The rescans are the one `naht pull` asks for (`a_pull_that_deletes_the_last_file_of_a_folder_removes_the_folder`) and the one that follows a `naht resolve` lifting a freeze (`a_resolve_that_lifts_a_freeze_on_a_file_studio_dropped_removes_the_folder_it_empties`). The walk does not follow terrain blobs: a `Terrain.terrain` file that Studio drops keeps its disk copy, which the reconcile treats as new and sends back to Studio (see the omitted-terrain rule below). The project root, each top-level directory and each configured `[[tree]]` path are never removed, except that the folder of an init script the batch deleted is removed once it is empty, unless it is a configured `[[tree]]` path (`deleting_a_childless_init_script_removes_its_empty_folder`, `renaming_a_childless_init_script_removes_its_empty_folder`, `the_last_file_deleted_under_a_top_level_source_root_keeps_the_root`, `a_configured_source_root_keeps_its_directory_when_its_last_file_is_deleted`, `a_configured_source_root_spelled_with_a_dot_keeps_its_folder_when_only_its_init_script_is_deleted`). The walk considers only directories at depth two or deeper, so a directory directly under the project root (such as `src` or `packages`) is never removed by it, configured or not, except as the folder of a deleted init script as described above. A configured `[[tree]]` path is matched case-insensitively and by its normal path parts: a leading `./` is ignored, so `./src` and `src` name the same folder, and a folder whose name differs only in case from a configured path is kept as well (`a_source_root_configured_in_one_case_protects_the_folder_in_another`, `a_configured_nested_source_root_spelled_with_a_dot_keeps_its_directory_when_its_last_file_is_deleted`). A configured path that has a `..`, a root or a drive prefix is refused rather than folded: `naht serve` refuses the whole config at startup, `naht build` and `init --from-rojo` refuse the path when they read it (`naht build` also refuses a symbolic link on the path that leads outside the project, in `tree_source_dir`), and `Session::protect_source_roots` returns `SourceRootOutsideRoot` and protects none of the roots in that call (`serve_refuses_a_tree_path_with_a_parent_component`, `serve_refuses_an_absolute_tree_path`, `a_refused_tree_path_is_refused_before_the_instance_lock_is_taken`, `a_source_root_that_leaves_the_project_is_refused_rather_than_folded`, `a_refused_source_root_protects_none_of_the_roots_in_its_call`). A path made only of `.` is the project root: it is accepted, its key is empty, so it protects no folder and the project root is never removed (`a_source_root_made_only_of_dots_is_the_project_root_and_protects_no_folder`, `a_dot_tree_path_is_accepted_and_serve_reaches_the_bind`). A directory that still holds any entry, including a non-synced file such as `notes.txt`, is kept and the batch answers `200` (`an_emptied_folder_that_still_holds_a_non_synced_file_is_kept_with_200`). A directory the disk refuses to remove is kept, and the batch still answers `200` (`a_folder_the_disk_refuses_to_remove_keeps_the_batch_and_answers_200`). `DiskVfs::remove` and `MemoryVfs::remove` delete a directory only when it is empty (`remove_dir`), so no path through reconcile or rollback can delete a tree: a directory that still holds files survives (`a_batch_cannot_remove_a_directory_so_its_files_survive_a_failed_batch`, `removing_a_directory_outside_a_batch_keeps_its_files`, `remove_refuses_a_directory_that_still_has_files`). The activity events, queued patches and unacked changes a batch makes are held until it commits and dropped on failure, so a failed batch leaves the activity log and the patch queue unchanged (`a_failed_batch_leaves_the_activity_log_and_patch_queue_unchanged`). A panic during `commit` would leave the disk committed and the outbox partly applied; this is not reachable without an out-of-memory abort.
- **A path that one batch both upserts and deletes keeps the upsert**, whatever order Studio sends the two changes in: upserts are staged first, and a delete whose target is an upserted path is skipped (`a_path_upserted_and_deleted_in_one_batch_keeps_the_upsert_in_either_order`). A delete resolves its target against the upserts too, so a flat script deleted while its init file is upserted in the same batch leaves the Studio mirror (`a_deleted_flat_path_leaves_the_studio_mirror_when_its_init_file_is_upserted_in_the_same_batch`).
- A `POST` answer always reflects what was applied. The 35-second request timeout (the long-poll window plus ten seconds) wraps only the `GET` routes, so no `POST` handler is ever cut off after it has started applying: a batch is answered with the outcome of the apply that actually ran, never `408` after it ran. A `POST` is bounded before it applies, never while it applies: it waits at most ten seconds (`POST_WAIT`) for one of two body slots and for the session lock, and past either bound it is answered `503` with nothing applied (`a_post_waiting_longer_than_the_bound_for_the_session_lock_gets_503_and_applies_nothing`). Past the per-route limit of 64 requests in flight, a `POST` is shed at once with `503` (`a_post_beyond_the_concurrency_limit_is_shed_with_503`). Once a `POST` holds the lock its apply is untimed, and it holds its body slot until its handler returns, even if the client has gone. Replaying a batch that was already applied, immediately after it, leaves the disk and the Studio mirror unchanged and emits no patch (`a_batch_replayed_after_its_answer_was_lost_changes_nothing`).
- A `POST /changes` batch with any refused change is refused before anything applies: `400` with a JSON body `{"refused_paths": [...]}` naming every refused path in batch order. A change is refused when its path is not a visible Studio script path, has a segment holding a character Windows forbids in file names (`< > " | ? *`, or a control byte (0x00 to 0x1F, NUL included), on every host), has a segment that is a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `CLOCK$`, `CONIN$`, `CONOUT$`, `COM0` to `COM9`, `LPT0` to `LPT9`, the superscript-digit forms `COM¹` to `COM³` and `LPT¹` to `LPT³`, in any case, with or without an extension; `write_tree_refuses_each_windows_device_name_the_project_path_rule_refuses`), escapes the project root or goes through a symlink, or is an `Upsert` naming a path now held by a directory (a file replaced by a directory on disk, then edited in Studio; `text_batches_name_every_refused_path_in_the_body`, `a_text_batch_naming_a_path_that_is_now_a_directory_is_refused_400_naming_it`). A `Delete` on a directory is accepted only when the same batch deletes every on-disk entry under it, recursively: the directory is then removed, deepest first (`a_batch_deleting_a_folder_and_all_its_children_removes_the_directory`). A `Delete` on a directory whose entries the batch leaves in place is refused and named (`a_batch_deleting_a_folder_while_its_children_remain_is_refused_naming_it`). The `POST /blobs` directory case uses the same JSON `400` with `{"refused_paths": [...]}` naming each path held by a directory, and nothing applies (`a_blob_batch_naming_a_path_that_is_now_a_directory_is_refused_400_naming_it`). Known limit: every later Studio edit to a directory path is refused the same way until the user removes or renames the directory on disk. The plugin drops the named paths with a Warning and posts the rest of the batch. A `POST` refused with a `VfsError` (a path that is not safely rooted, or that goes through a symlink), or with a malformed body, is answered `400` with a JSON body `{"error": "..."}`, not plain text: the message carries the client's path or bytes, and the JSON encoding escapes them, control bytes included (`an_invalid_path_is_answered_as_json_with_the_client_path_escaped`, `a_malformed_body_is_answered_as_json_with_the_client_bytes_escaped`).
- A path the scan could not see (an unreadable file, or one under an ignored directory) is left alone on both sides and in the base: it is neither deleted nor overwritten by a Studio edit.
- A terrain file whose name differs from `Terrain.terrain` only by letter case (`terrain.terrain`, say) is not synced. The scan records it as skipped under `Terrain.terrain`, so that path is left alone on both sides and in the base, and the daemon warns that the file must be renamed to `Terrain.terrain`, or removed when `Terrain.terrain` is also on disk, since a rename over it would replace the synced file. Without this rule, a case-insensitive disk would resolve Studio's `Terrain.terrain` onto the variant and overwrite the user's file with no freeze (`a_terrain_file_in_another_case_is_left_alone_when_studio_reports_terrain`, `a_stray_terrain_file_beside_the_synced_name_is_never_told_to_rename_over_it`).
- A report that omits `Terrain.terrain` never deletes the disk copy. The blob reconcile drops that file's base record and resends the disk copy to Studio as a Create, and the file stays where it is. This is the decision: the blob channel never deletes a file on the filesystem, because a report that omits the terrain could be empty, and deleting on it would destroy the only copy. The shipped plugin never sends such a report: it posts `Terrain.terrain` with every blob it sends, and a terrain too large to push posts nothing, so only another local client can reach this case.
- A `[[tree]]` path, or a migrated Rojo `$path`, with a segment that names a Windows device (`CON`, `NUL`, `LPT1` and the rest of the list the `/changes` rule uses, with or without an extension) or holds a character Windows forbids in file names (`< > " : | ? *`, or a control byte) is refused with `TreePathNotPortable` on every host: `naht serve` refuses the config at startup, and `naht build` and `init --from-rojo` refuse it when they read it. The check runs on the path as written, so a project that builds on Linux cannot hold a folder Windows cannot check out. A drive prefix such as `C:` or `C:/src` is refused the same way, as `TreePathNotPortable` and not as `TreePathOutsideRoot`, because the portability check runs before the outside-root check. The rule runs on the raw string: a backslash is refused outright, and the remaining segments are split on `/`, so `src\lib` is refused the same way on every host and the Linux test proves it (`serve_refuses_a_tree_path_naming_a_windows_device_or_a_forbidden_character`, `build_rejects_a_tree_path_naming_a_windows_device`, `from_rojo_rejects_a_path_naming_a_windows_device`).
- A write never follows a symlink. A live or dangling link at the target is refused with `VfsError::Symlink`, and nothing behind it is created or changed.
- Temp files (`.<name>.<pid>.<n>.naht-tmp`) untouched for ten minutes are removed when a session opens, outside the walk's ignored directories. Younger ones are left alone, so a write running in another process (a second daemon or a concurrent `naht` command) is not broken. A temp file whose modification time cannot be read is kept.
- `thiserror` in `naht-core`, `anyhow` at the binary boundary.
- **Safety guard:** `servePlaceId`: when the project sets it, every request must carry the matching
  `x-naht-place-id` header (`403` otherwise), so source does not land in a different game. It is a safety
  check, not authentication: the token is the credential (see the token and the trust boundary in §7).

## 9. Limits & workarounds

We cannot beat the API ceiling (§2). What we *can* do, and Rojo/Argon don't, is **detect and report
explicitly** instead of dropping silently, and handle the binary cases as cleanly as possible. Detail
lives in the final stage of [`spec.md`](spec.md).

| Case | Status | Approach |
|---|---|---|
| **`MeshId` / images** | 🟢 upload (post-v1) | The asset-id string syncs like any property; with `[assets]` enabled, `naht build` alone uploads a *local* mesh/image file once via **Open Cloud**, caches it by the BLAKE3 digest of its bytes, and rewrites its property to `rbxassetid://…`; `sync` and `serve` never upload and leave references as written. Only properties that Roblox reflection types as Content are uploaded; a local file named by any other property is reported and left as written. Off by default. Stage 12. |
| **Terrain** | 🟢 syncable (post-v1) | Read/written via `ReadVoxels`/`WriteVoxels` and synced as an opaque binary voxel blob: hash-compared, last-writer-wins, a both-sides change **freezes** (no diff/merge). Driven end to end through the daemon's `/blobs` channel when `[serve] terrain_sync` is on, for `Terrain.terrain` at the project root only; `serve` ignores every other `.terrain` file. A sculpt is pushed live on the next `ChangeHistoryService` waypoint after a 1 s debounce; an unchanged blob is never pushed, and a blob just applied from the daemon is never pushed back. A push over `[limits] terrain_voxels` (1,048,576 voxels by default) is refused with `413`, and the plugin then asks for `naht build`. Stages 11, 13. |
| **CSG / Unions** | 🟡 binary round-trip | Engine-generated binary geometry can't be rebuilt from text, but round-trips inside `rbxm` model files (opaque, file-level). |
| **`HttpEnabled` & security-locked props** | 🔴 hard block | Not settable by scripts/plugins by design. Naht warns and points to Game Settings; offers a place-file fallback. |

Where Naht can do **better than Rojo** on binary/asset handling (clear detection, explicit warnings,
clean opaque round-trips instead of silent drops or overwrites), it should: treated as a bonus goal,
not a v1 blocker.

## 10. Testing strategy

- **`naht-core`:** unit tests over an in-memory VFS: mapping, reconcile, merge, and state store in
  isolation.
- **Integration:** a fake Studio HTTP client (the `fake-studio` pattern) drives end-to-end FS ↔
  simulated-Studio scenarios, including conflict and reconnect.
- **Plugin:** a minimal Luau harness, added once the protocol stabilizes.

## 11. Known limitations and follow-ups

- **A local process can hold the connection slots (open).** `naht serve` serves at most `[limits] connections` connections at once (64 by default). A connection that sends nothing, or that opens with an HTTP/2 preface, is closed within ten seconds (§8), but a local process that keeps reconnecting can still take the slots as they free, and the plugin then waits in the kernel's listen backlog until one frees. The token is checked only after a connection holds its slot, so a process without the token can do this too, under any account. The limit has not been measured under load.
- **Port squatting while the daemon is down (open).** While `naht serve` is not running, nothing holds the port the plugin dials (the default `34872`, or the one given with `--port`). A local process can bind that port, answer `GET /info` with the project's public id and name, and receive the token: the plugin looks up the token stored for the project id it is told, and sends it on every request once it has one. The token and the place guard do not stop this, because the plugin trusts whatever answers on the port. The follow-up is a daemon proof: an HMAC over a nonce the plugin sends, keyed by the token, so the plugin checks the answering daemon before it sends the token. Until then the risk stays open, and it is recorded in [`SECURITY.md`](../SECURITY.md).
- **The place guard is a safety check (open).** `[serve] place_id` stops a session bound to the wrong place
  from syncing, but the place id is a value the client sends, so it is not a credential. Without `[serve] place_id`
  the daemon binds the project to the first place that attaches (recorded in `.naht/state.db`) and refuses any other
  with a message naming both ids. The binding is not a credential either: a first attach to the wrong place binds
  the project to it, and the fix is `naht unbind` (or set `[serve] place_id`). `naht unbind` clears only the recorded
  place, under the daemon lock, because deleting `.naht/state.db` would also drop the bases, the recorded conflicts
  and the asset cache. Once a place is recorded,
  every `POST` (`/changes`, `/blobs`, `/inventory`, `/ack`) without an `x-naht-place-id` header is refused with `403`,
  so omitting the header cannot write into a bound project (`a_write_without_a_place_header_is_refused_once_a_place_is_bound`,
  `a_write_without_a_place_header_is_refused_after_a_restart`). `GET` routes without the header stay open, which covers
  `naht pull` (it reads `/info` without the header). Every route except `GET /info` now needs the token, so `GET /patches` no longer hands
  queued patches to a local client without it. The place guard does not restrict reads, and it is not a credential
  either: the token is. The shipped plugin always sends the header.
- **Watch cost (N5).** The watcher installs one recursive watch on the project root, debounced at 200 ms. Directories the walk ignores (`node_modules`, `target`, dot-directories) are still watched, so a busy `node_modules` produces events that `is_project_event` then discards. The cost is CPU and event volume; correctness is unaffected. Excluding them would need per-directory watches that the debouncer's callback thread cannot update, so it is not implemented.
- **Place 0 is never bound.** Studio reports place id `0` until a place is published, and every unpublished place
  reports it, so the daemon cannot tell them apart and never records it. An unbound project accepts a request for
  place `0` without binding, so an unpublished place can attach before it is published; a bound project refuses it
  with `403`. `[serve] place_id = 0` is a config error.
- **A live rename from an unsaved Studio session is reverted on disk (open).** A rename synced live from a session
  that is never saved looks, at the next attach, like a rename made while the plugin was stopped, so the inventory
  pairs it and moves the file back to its old name. The content is preserved. This follows from the pairing rule in
  §7 and is accepted.
- **File modes (N7c).** A write goes to a temp file that is renamed over the target, and only the permission bits of the old file are copied. Owner, group, extended attributes and hard links are not preserved.
- **Upgrading (N4).** Asset cache rows keyed by the pre-BLAKE3 64-bit hash are deleted when the state store opens. An asset whose row was dropped is uploaded once more on its next `naht build`.
- **The inventory applies renames only (open).** A script created in Studio while the plugin was stopped is not written to disk, and a script deleted in Studio while the plugin was stopped stays on disk. Both are left alone and counted in one info line on the `naht::sync` target. Applying them would make Studio authoritative over the disk, which discards unsaved editor work (§7 rules that out).
- **A folder renamed while the plugin was stopped moves only the scripts it can pair (open).** Each listed script is paired with an unlisted mirror script of the same class and fingerprint. A script whose content also changed, or that was deleted in Studio at the same time, is not paired: its old file stays under the old folder, the new Studio content is not written at attach, and the next edit in Studio writes it under the new path, so the old file has to be removed by hand.
- **A folder case-only rename onto a folder spelled differently is refused (open).** On a case-insensitive disk, renaming `Utils` to `utils` in Studio moves the folder through a scratch name. When the disk already holds another folder that folds to the same name, the batch fails with `FolderRenameBlocked` and rolls back. The case-only folder rule is tested with a case-folding fake Vfs, not on a real case-insensitive volume in CI. A check on a vfat image showed that `rename(2)` to a case variant is a no-op on Linux, so the behaviour on macOS and Windows is reasoned, not measured.
- **A body over the `[limits] inventory_body_bytes` cap is not chunked (open).** The cap is 64 MiB by default. `POST /inventory` answers `413` for such a body, and the plugin skips the inventory for that session with a warning, so the next attach retries. Chunking was rejected: a rename pair can span two chunks, and a later chunk would then be read as a claim against a mirror entry an earlier chunk had paired.
- **A script whose name starts with `.` is not synced (open).** The plugin refuses such names (`isSafeName`), and the daemon refuses hidden components on write (`validate_source_path`), although the project path rule accepts them. The script is not written to disk and the plugin warns once.
