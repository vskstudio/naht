--!strict

export type ServerInfo = {
	protocol_version: number,
	server_version: string,
	project_name: string,
	session_id: string,
	serve_place_id: number?,
	conflicted: { string }?,
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

export type BlobInstance = {
	path: string,
	class: string,
	content: string,
}

export type BlobPatch = {
	path: string,
	class: string,
	direction: string,
	kind: string,
	content: string?,
}

export type BlobPatchBatch = {
	cursor: number,
	patches: { BlobPatch },
}

export type Codec = {
	encode: (value: any) -> string,
	decode: (bytes: string) -> any,
	array: (items: { any }) -> any,
	binary: (bytes: string) -> any,
}

local PROTOCOL_VERSION = 2

local function new(MessagePack: Codec)
	local Protocol = {}

	Protocol.PROTOCOL_VERSION = PROTOCOL_VERSION
	Protocol.OUTDATED_PLUGIN_TOKEN = "outdated-plugin"
	Protocol.INCOMPATIBLE_VERSION = "naht: incompatible protocol version"

	function Protocol.decodeInfo(bytes: string): ServerInfo
		return MessagePack.decode(bytes)
	end

	function Protocol.assertCompatible(info: ServerInfo): ServerInfo
		if info.protocol_version ~= Protocol.PROTOCOL_VERSION then
			local hint = if info.protocol_version > Protocol.PROTOCOL_VERSION
				then "update the naht plugin"
				else "update the naht daemon"
			error(
				string.format(
					"%s (daemon %s, plugin %s): %s",
					Protocol.INCOMPATIBLE_VERSION,
					tostring(info.protocol_version),
					tostring(Protocol.PROTOCOL_VERSION),
					hint
				)
			)
		end
		return info
	end

	function Protocol.decodePatches(bytes: string): PatchBatch
		return MessagePack.decode(bytes)
	end

	function Protocol.decodeBlobs(bytes: string): BlobPatchBatch
		return MessagePack.decode(bytes)
	end

	function Protocol.encodeBlobs(changes: { BlobInstance }): string
		local wire = {}
		for _, change in changes do
			table.insert(wire, {
				path = change.path,
				class = change.class,
				content = MessagePack.binary(change.content),
			})
		end
		return MessagePack.encode({ changes = MessagePack.array(wire) })
	end

	function Protocol.upsert(path: string, class: string, content: string): Change
		return { kind = "Upsert", path = path, class = class, content = content }
	end

	function Protocol.delete(path: string): Change
		return { kind = "Delete", path = path }
	end

	function Protocol.encodeAck(seqs: { number }): string
		return MessagePack.encode({ seqs = MessagePack.array(seqs) })
	end

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
end

return { new = new }
