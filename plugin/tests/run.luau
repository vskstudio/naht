--!strict
-- Minimal Luau harness for the encode/decode and patch-apply paths (Stage 6 acceptance). Run with a
-- Luau runtime, e.g. `lune run plugin/tests/run.luau` from the repo root. It exits non-zero on any
-- failure.
--
-- The MessagePack cases decode bytes captured from the real daemon encoder
-- (`cargo run -p naht-core --example wire_golden`), so the codec is checked against the actual wire
-- format, not just against itself.

local MessagePack = require("../src/MessagePack")
local Apply = require("../src/Apply")
local Terrain = require("../src/Terrain")
local Protocol = require("../src/Protocol")

-- Golden MessagePack from `naht-core::protocol` (rmp_serde::to_vec_named).
local GOLDEN = {
	server_info = "85b070726f746f636f6c5f76657273696f6e01ae7365727665725f76657273696f6ea5302e312e30ac70726f6a6563745f6e616d65a464656d6faa73657373696f6e5f6964a3616263ae73657276655f706c6163655f69647b",
	patch_batch = "82a6637572736f7202a7706174636865739185a470617468aa7372632f412e6c756175a5636c617373ac4d6f64756c65536372697074a9646972656374696f6ea8546f53747564696fa46b696e64a6557064617465a7636f6e74656e74a872657475726e2031",
	change_batch = "81a76368616e6765739281a655707365727483a470617468aa7372632f412e6c756175a5636c617373ac4d6f64756c65536372697074a7636f6e74656e74a872657475726e203281a644656c65746581a470617468aa7372632f422e6c756175",
	blob_patch_batch = "82a6637572736f7201a7706174636865739185a470617468af5465727261696e2e7465727261696ea5636c617373a75465727261696ea9646972656374696f6ea8546f53747564696fa46b696e64a6557064617465a7636f6e74656e74c4050001ff807f",
}

local failures = 0

local function check(name: string, ok: boolean, detail: string?)
	if ok then
		print("ok   - " .. name)
	else
		failures += 1
		print("FAIL - " .. name .. (if detail ~= nil then ": " .. detail else ""))
	end
end

local function fromHex(hex: string): string
	return (hex:gsub("..", function(pair)
		return string.char(tonumber(pair, 16) :: number)
	end))
end

-- === MessagePack decode against the real wire format =========================================

local info = MessagePack.decode(fromHex(GOLDEN.server_info))
check("server_info protocol_version", info.protocol_version == 1)
check("server_info project_name", info.project_name == "demo")
check("server_info serve_place_id", info.serve_place_id == 123)
check("matching protocol version is accepted", Protocol.assertCompatible(info).protocol_version == 1)
local mismatchOk = pcall(function()
	Protocol.assertCompatible({
		protocol_version = 999,
		server_version = "future",
		project_name = "demo",
		session_id = "future",
		serve_place_id = nil,
	})
end)
check("mismatched protocol version is rejected", not mismatchOk)

local batch = MessagePack.decode(fromHex(GOLDEN.patch_batch))
check("patch_batch cursor", batch.cursor == 2)
check("patch_batch length", #batch.patches == 1)
local patch = batch.patches[1]
check("patch path", patch.path == "src/A.luau")
check("patch class", patch.class == "ModuleScript")
check("patch direction (unit enum -> string)", patch.direction == "ToStudio")
check("patch kind", patch.kind == "Update")
check("patch content", patch.content == "return 1")

local changes = MessagePack.decode(fromHex(GOLDEN.change_batch))
check("change_batch length", #changes.changes == 2)
check("change upsert (externally tagged)", changes.changes[1].Upsert.path == "src/A.luau")
check("change upsert content", changes.changes[1].Upsert.content == "return 2")
check("change delete (externally tagged)", changes.changes[2].Delete.path == "src/B.luau")

-- === MessagePack binary (terrain blob) =======================================================

local blobBatch = MessagePack.decode(fromHex(GOLDEN.blob_patch_batch))
check("blob_batch cursor", blobBatch.cursor == 1)
local blobPatch = blobBatch.patches[1]
check("blob path", blobPatch.path == "Terrain.terrain")
local blob = blobPatch.content
check("blob length", #blob == 5)
check(
	"blob bytes preserved (bin format)",
	string.byte(blob, 1) == 0
		and string.byte(blob, 2) == 1
		and string.byte(blob, 3) == 255
		and string.byte(blob, 4) == 128
		and string.byte(blob, 5) == 127
)

local binRoundtrip = MessagePack.decode(MessagePack.encode({
	blob = MessagePack.binary("\0\255\10\127"),
}))
check("binary encode/decode round-trip", binRoundtrip.blob == "\0\255\10\127")

-- The `POST /blobs` wire shape (mirroring Protocol.encodeBlobs): raw voxel bytes carried as a
-- MessagePack bin survive a round-trip intact, including NUL and high bytes.
local blobPush = MessagePack.decode(MessagePack.encode({
	changes = MessagePack.array({
		{ path = "Terrain.terrain", class = "Terrain", content = MessagePack.binary("\0\1\255\128\127") },
	}),
}))
check("blob push preserves path", blobPush.changes[1].path == "Terrain.terrain")
check("blob push preserves class", blobPush.changes[1].class == "Terrain")
check("blob push preserves raw bytes", blobPush.changes[1].content == "\0\1\255\128\127")

-- === terrain blob (de)serialization round-trip ===============================================

local grid = {
	resolution = 4,
	min = { x = 0, y = 0, z = 0 },
	max = { x = 8, y = 8, z = 8 },
	-- nx=2, ny=1, nz=2
	materials = { { { 1, 256 } }, { { 2, 3 } } },
	occupancies = { { { 0.0, 1.0 } }, { { 0.5, 0.25 } } },
}
local terrainBack = Terrain.deserialize(Terrain.serialize(grid))
check(
	"terrain dims",
	#terrainBack.materials == 2 and #terrainBack.materials[1] == 1 and #terrainBack.materials[1][1] == 2
)
check(
	"terrain materials round-trip",
	terrainBack.materials[1][1][1] == 1
		and terrainBack.materials[1][1][2] == 256
		and terrainBack.materials[2][1][1] == 2
)
check(
	"terrain occupancies round-trip",
	terrainBack.occupancies[1][1][1] == 0.0
		and terrainBack.occupancies[1][1][2] == 1.0
		and terrainBack.occupancies[2][1][2] == 0.25
)
check("terrain resolution round-trip", terrainBack.resolution == 4)

-- === MessagePack encode round-trip ===========================================================

local roundtrip = MessagePack.decode(MessagePack.encode({
	changes = MessagePack.array({
		{ Upsert = { path = "p", class = "ModuleScript", content = "c" } },
		{ Delete = { path = "d" } },
	}),
}))
check("encode/decode upsert", roundtrip.changes[1].Upsert.path == "p")
check("encode/decode delete", roundtrip.changes[2].Delete.path == "d")
check("encode/decode preserves count", #roundtrip.changes == 2)

local primitives = MessagePack.decode(MessagePack.encode({
	flag = true,
	count = 4096,
	text = "hello",
}))
check("encode bool", primitives.flag == true)
check("encode number", primitives.count == 4096)
check("encode string", primitives.text == "hello")

local function rejects(name: string, hex: string)
	local ok = pcall(MessagePack.decode, fromHex(hex))
	check(name, not ok)
end

local function decodesTo(name: string, hex: string, expected: any)
	local ok, value = pcall(MessagePack.decode, fromHex(hex))
	check(name, ok and value == expected, if ok then "got " .. tostring(value) else tostring(value))
end

rejects("str8 shorter than its declared length is rejected", "d90a616263")
rejects("bin8 shorter than its declared length is rejected", "c40a010203")
rejects("array with a missing element is rejected", "9201")
rejects("map with a missing value is rejected", "81a161")
rejects("truncated uint16 is rejected", "cd01")
rejects("empty input is rejected", "")
rejects("trailing bytes after the top-level value are rejected", "0102")
rejects("unsupported byte 0xc1 is rejected", "c1")
decodesTo("float32 is decoded", "ca3fc00000", 1.5)
decodesTo("float64 is decoded", "cb3ff8000000000000", 1.5)
decodesTo("int64 is decoded", "d3fffffffffffffffe", -2)
decodesTo("negative fixint is decoded", "e0", -32)
decodesTo("int8 is decoded", "d0ff", -1)
decodesTo("int16 is decoded", "d1fffe", -2)
decodesTo("int32 is decoded", "d2fffffffe", -2)
local nestedOk = pcall(MessagePack.decode, fromHex(string.rep("91", 64) .. "01"))
check("64 nested arrays are accepted", nestedOk)
rejects("65 nested arrays exceed the depth cap", string.rep("91", 65) .. "01")

-- === patch-apply against a mock DataModel ====================================================

local function mockTree()
	local function newNode(className: string, name: string)
		return { ClassName = className, Name = name, Source = "", children = {}, parent = nil }
	end
	local root = newNode("Folder", "root")
	return {
		root = root,
		createInstance = function(className: string, name: string)
			return newNode(className, name)
		end,
		findChild = function(parent: any, name: string): any
			for _, child in parent.children do
				if child.Name == name then
					return child
				end
			end
			return nil
		end,
		children = function(parent: any): { any }
			local list = {}
			for index, child in parent.children do
				list[index] = child
			end
			return list
		end,
		className = function(node: any): string
			return node.ClassName
		end,
		getSource = function(node: any): string
			return node.Source
		end,
		setParent = function(node: any, parent: any)
			node.parent = parent
			table.insert(parent.children, node)
		end,
		setSource = function(node: any, content: string)
			node.Source = content
		end,
		destroy = function(node: any)
			local parent = node.parent
			if parent ~= nil then
				for index, child in parent.children do
					if child == node then
						table.remove(parent.children, index)
						break
					end
				end
			end
		end,
	}
end

local tree = mockTree()
local applier = Apply.create(tree)

applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
local src = tree.findChild(tree.root, "src")
check("apply creates intermediate folder", src ~= nil and src.ClassName == "Folder")
local a = if src ~= nil then tree.findChild(src, "A") else nil
check("apply creates leaf with class", a ~= nil and a.ClassName == "ModuleScript")
check("apply sets source", a ~= nil and a.Source == "return 1")

applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Update", content = "return 2" })
check("apply update reuses node and sets source", a ~= nil and a.Source == "return 2")
check("apply update does not duplicate", src ~= nil and #src.children == 1)

applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Delete" })
check("apply delete removes node", src ~= nil and #src.children == 0)

applier:apply({
	path = "ui/main.client.luau",
	class = "LocalScript",
	kind = "Create",
	content = "print('hi')",
})
local ui = tree.findChild(tree.root, "ui")
local main = if ui ~= nil then tree.findChild(ui, "main") else nil
check("apply strips .client.luau suffix", main ~= nil and main.Name == "main")
check("apply maps LocalScript class", main ~= nil and main.ClassName == "LocalScript")

local function freshApplier()
	local fresh = mockTree()
	return fresh, Apply.create(fresh)
end

local function countNamed(parent: any, name: string): number
	local count = 0
	for _, child in parent.children do
		if child.Name == name then
			count += 1
		end
	end
	return count
end

do
	local fresh, freshApply = freshApplier()
	freshApply:apply({ path = "Utils/Helper.luau", class = "ModuleScript", kind = "Create", content = "" })
	freshApply:apply({ path = "Utils.luau", class = "ModuleScript", kind = "Delete" })
	local utils = fresh.findChild(fresh.root, "Utils")
	check(
		"delete of a file keeps a same-named folder and its children",
		utils ~= nil and utils.ClassName == "Folder" and fresh.findChild(utils, "Helper") ~= nil
	)
end

do
	local fresh, freshApply = freshApplier()
	freshApply:apply({ path = "x.luau", class = "ModuleScript", kind = "Create", content = "a" })
	freshApply:apply({ path = "x.server.luau", class = "Script", kind = "Delete" })
	local survivor = fresh.findChild(fresh.root, "x")
	check(
		"delete of a script whose class changed keeps the new sibling",
		survivor ~= nil and survivor.ClassName == "ModuleScript" and survivor.Source == "a"
	)
end

do
	local fresh, freshApply = freshApplier()
	freshApply:apply({ path = "missing/A.luau", class = "ModuleScript", kind = "Delete" })
	check("delete never creates missing parent folders", fresh.findChild(fresh.root, "missing") == nil)
end

do
	local fresh, freshApply = freshApplier()
	freshApply:apply({ path = "x/Inner.luau", class = "ModuleScript", kind = "Create", content = "" })
	freshApply:apply({ path = "x.luau", class = "ModuleScript", kind = "Create", content = "" })
	freshApply:apply({ path = "x.luau", class = "ModuleScript", kind = "Delete" })
	local remaining = countNamed(fresh.root, "x")
	local folder = fresh.findChild(fresh.root, "x")
	check(
		"delete removes only the same-class sibling among same-named instances",
		remaining == 1 and folder ~= nil and folder.ClassName == "Folder"
	)
end

-- === transport modules compile ===============================================================
-- These talk to HttpService / the DataModel, so they can't run headless, but they must at least
-- compile. Catches syntax regressions in the modules the cases above don't load.

local luau = require("@lune/luau")
local fs = require("@lune/fs")
for _, path in
	{
		"plugin/src/Protocol.luau",
		"plugin/src/Client.luau",
		"plugin/src/Connection.luau",
		"plugin/src/Plugin.server.luau",
	}
do
	local ok = pcall(function()
		luau.load(fs.readFile(path))
	end)
	check("compiles " .. path, ok)
end

-- === result ==================================================================================

if failures > 0 then
	error(string.format("%d harness check(s) failed", failures))
end
print("all harness checks passed")
