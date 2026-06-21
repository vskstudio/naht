--!strict
-- Build and parse the wire messages, on top of the MessagePack codec.
--
-- Mirrors the daemon's `naht-core::protocol` types. A Studio-side change is expressed as an
-- `Upsert`/`Delete` and encoded into the externally-tagged shape serde expects:
-- `{ Upsert = { path, class, content } }` / `{ Delete = { path } }`.

local MessagePack = require(script.Parent.MessagePack)

local Protocol = {}

Protocol.PROTOCOL_VERSION = 1

export type ServerInfo = {
	protocol_version: number,
	server_version: string,
	project_name: string,
	session_id: string,
	serve_place_id: number?,
}

export type Patch = {
	path: string,
	class: string,
	direction: string,
	kind: string,
	content: string?,
}

export type PatchBatch = {
	cursor: number,
	patches: { Patch },
}

export type Change = {
	kind: "Upsert" | "Delete",
	path: string,
	class: string?,
	content: string?,
}

-- Decode a `GET /info` body.
function Protocol.decodeInfo(bytes: string): ServerInfo
	return MessagePack.decode(bytes)
end

-- Decode a `GET /patches` body.
function Protocol.decodePatches(bytes: string): PatchBatch
	return MessagePack.decode(bytes)
end

-- A Studio-side upsert (instance created or source edited).
function Protocol.upsert(path: string, class: string, content: string): Change
	return { kind = "Upsert", path = path, class = class, content = content }
end

-- A Studio-side deletion.
function Protocol.delete(path: string): Change
	return { kind = "Delete", path = path }
end

-- Encode the paths the plugin applied into a `POST /ack` body. Only acked paths advance the
-- daemon's base, so a patch that failed to apply must be left out.
function Protocol.encodeAck(paths: { string }): string
	return MessagePack.encode({ paths = MessagePack.array(paths) })
end

-- Encode a batch of Studio-side changes into a `POST /changes` body.
function Protocol.encodeChanges(changes: { Change }): string
	local wire = {}
	for _, change in changes do
		if change.kind == "Delete" then
			table.insert(wire, { Delete = { path = change.path } })
		else
			table.insert(wire, {
				Upsert = {
					path = change.path,
					class = change.class,
					content = change.content,
				},
			})
		end
	end
	return MessagePack.encode({ changes = MessagePack.array(wire) })
end

return Protocol
