--!strict

task = require("@lune/task") :: any
Enum = {
	Material = {
		FromValue = function(_, value: number)
			return value
		end,
	},
} :: any

local MessagePack = require("../src/MessagePack")
local Apply = require("../src/Apply")
local Terrain = require("../src/Terrain")
local Protocol = require("../src/Protocol").new(MessagePack)
local Connection = require("../src/Connection")

local GOLDEN = {
	server_info = "86b070726f746f636f6c5f76657273696f6e01ae7365727665725f76657273696f6ea5302e312e30ac70726f6a6563745f6e616d65a464656d6faa73657373696f6e5f6964a3616263ae73657276655f706c6163655f69647baa636f6e666c696374656490",
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

local function noop() end

local info = MessagePack.decode(fromHex(GOLDEN.server_info))
check("server_info protocol_version", info.protocol_version == 1)
check("server_info project_name", info.project_name == "demo")
check("server_info serve_place_id", info.serve_place_id == 123)
check("matching protocol version is accepted", Protocol.assertCompatible(info).protocol_version == 1)
check(
	"mismatched protocol version is rejected",
	not pcall(Protocol.assertCompatible, {
		protocol_version = 999,
		server_version = "future",
		project_name = "demo",
		session_id = "future",
		serve_place_id = nil,
	})
)

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

local blobPush = MessagePack.decode(MessagePack.encode({
	changes = MessagePack.array({
		{ path = "Terrain.terrain", class = "Terrain", content = MessagePack.binary("\0\1\255\128\127") },
	}),
}))
check("blob push preserves path", blobPush.changes[1].path == "Terrain.terrain")
check("blob push preserves class", blobPush.changes[1].class == "Terrain")
check("blob push preserves raw bytes", blobPush.changes[1].content == "\0\1\255\128\127")

local grid = {
	resolution = 4,
	min = { x = 0, y = 0, z = 0 },
	max = { x = 8, y = 8, z = 8 },
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

local function terrainHeader(nx: number, ny: number, nz: number, resolution: number): string
	return string.pack("<i4i4i4fffffff", nx, ny, nz, 0, 0, 0, 1, 1, 1, resolution)
end

local function rejectsTerrain(name: string, blobText: string)
	check("terrain blob rejects " .. name, not pcall(Terrain.deserialize, blobText))
end

rejectsTerrain("a truncated header", string.sub(terrainHeader(1, 1, 1, 4), 1, 20))
rejectsTerrain("a zero dimension", terrainHeader(0, 1, 1, 4))
rejectsTerrain("a negative dimension", terrainHeader(-1, 1, 1, 4))
rejectsTerrain("an oversized dimension", terrainHeader(4096, 1, 1, 4))
rejectsTerrain("a non-positive resolution", terrainHeader(1, 1, 1, 0) .. string.pack("<I2f", 1, 0.5))
rejectsTerrain("a missing voxel", terrainHeader(1, 1, 2, 4) .. string.pack("<I2f", 1, 0.5))
rejectsTerrain("trailing bytes", terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5) .. "\0")

local regionFromStudio = {
	CFrame = { Position = { X = 10, Y = 20, Z = 30 } },
	Size = { X = 8, Y = 4, Z = 2 },
}
local fakeVoxelTerrain = {
	ReadVoxels = function(_, _region, _resolution)
		return { { { { Value = 1 } } } }, { { { 0.5 } } }
	end,
}
local readBack = Terrain.deserialize(Terrain.read(fakeVoxelTerrain, regionFromStudio, 4))
check(
	"terrain read uses region min as center minus half size",
	readBack.min.x == 6 and readBack.min.y == 18 and readBack.min.z == 29
)
check(
	"terrain read uses region max as center plus half size",
	readBack.max.x == 14 and readBack.max.y == 22 and readBack.max.z == 31
)

local function voxelCountOf(sizeX: number, sizeY: number, sizeZ: number, resolution: number): number
	local ok, count = pcall(Terrain.voxelCount, sizeX, sizeY, sizeZ, resolution)
	return if ok then count else -1
end

check("voxel count of a 16 stud cube at 4 studs is 64", voxelCountOf(16, 16, 16, 4) == 64)
check("voxel count rounds a partial voxel up", voxelCountOf(5, 4, 4, 4) == 2)
check("a 1024x256x1024 region at 4 studs is 4194304 voxels", voxelCountOf(1024, 256, 1024, 4) == 4194304)

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
	check(name, not pcall(MessagePack.decode, fromHex(hex)))
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
check("64 nested arrays are accepted", pcall(MessagePack.decode, fromHex(string.rep("91", 64) .. "01")))
rejects("65 nested arrays exceed the depth cap", string.rep("91", 65) .. "01")

local function signal()
	local connections: { { handler: (...any) -> (), active: boolean } } = {}
	local object = {}
	function object:Connect(handler)
		local entry = { handler = handler, active = true }
		table.insert(connections, entry)
		return {
			Disconnect = function()
				entry.active = false
			end,
		}
	end
	function object:Fire(...)
		for _, entry in connections do
			if entry.active then
				entry.handler(...)
			end
		end
	end
	function object:count(): number
		local live = 0
		for _, entry in connections do
			if entry.active then
				live += 1
			end
		end
		return live
	end
	return object
end

local SCRIPT_CLASSES = { Script = true, LocalScript = true, ModuleScript = true }

local function newNode(className: string, name: string): any
	local node: any = {
		ClassName = className,
		Name = name,
		Source = "",
		Parent = nil,
		children = {},
		sourceChanged = signal(),
	}
	node.GetPropertyChangedSignal = function(_, _property)
		return node.sourceChanged
	end
	return node
end

local function studioWorld(syncEcho: boolean)
	local root: any = newNode("Folder", "root")
	root.DescendantAdded = signal()
	root.DescendantRemoving = signal()
	root.GetDescendants = function(self)
		local list = {}
		local function visit(node)
			for _, child in node.children do
				table.insert(list, child)
				visit(child)
			end
		end
		visit(self)
		return list
	end
	local queued: { any } = {}
	local function detach(node)
		local parent = node.Parent
		if parent ~= nil then
			for index, child in parent.children do
				if child == node then
					table.remove(parent.children, index)
					break
				end
			end
		end
		node.Parent = nil
	end
	local tree = {
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
			if not SCRIPT_CLASSES[node.ClassName] then
				error("Folder has no Source property")
			end
			return node.Source
		end,
		setParent = function(node: any, parent: any)
			detach(node)
			node.Parent = parent
			table.insert(parent.children, node)
			root.DescendantAdded:Fire(node)
		end,
		setSource = function(node: any, content: string)
			if not SCRIPT_CLASSES[node.ClassName] then
				error("Folder has no Source property")
			end
			node.Source = content
			if syncEcho then
				node.sourceChanged:Fire()
			else
				table.insert(queued, node)
			end
		end,
		destroy = function(node: any)
			detach(node)
			root.DescendantRemoving:Fire(node)
		end,
	}
	local function flush()
		local pending = queued
		queued = {}
		for _, node in pending do
			node.sourceChanged:Fire()
		end
	end
	return tree, root, flush
end

local function place(tree: any, className: string, name: string, parent: any): any
	local node = tree.createInstance(className, name)
	tree.setParent(node, parent)
	return node
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

local function findByClass(parent: any, name: string, className: string): any
	for _, child in parent.children do
		if child.Name == name and child.ClassName == className then
			return child
		end
	end
	return nil
end

local function lockedSource(tree: any, name: string, failures: number)
	local original = tree.setSource
	local remaining = failures
	tree.setSource = function(node: any, content: string)
		if node.Name == name and remaining > 0 then
			remaining -= 1
			error("Source is locked")
		end
		original(node, content)
	end
end

local function freshApplier()
	local tree, root = studioWorld(true)
	return tree, root, Apply.create(tree)
end

do
	local tree, root, applier = freshApplier()
	local shadow = place(tree, "Script", "Foo", root)
	local folder = place(tree, "Folder", "Foo", root)
	applier:apply({ path = "Foo/Bar.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
	check("a Script named Foo does not hide a Folder named Foo as parent", countNamed(folder, "Bar") == 1)
	check("the Script named Foo receives no child", #tree.children(shadow) == 0)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
	local src = tree.findChild(root, "src")
	check("apply creates intermediate folder", src ~= nil and src.ClassName == "Folder")
	local a = if src ~= nil then tree.findChild(src, "A") else nil
	check("apply creates leaf with class", a ~= nil and a.ClassName == "ModuleScript")
	check("apply sets source", a ~= nil and a.Source == "return 1")

	applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Update", content = "return 2" })
	check("apply update reuses node and sets source", a ~= nil and a.Source == "return 2")
	check("apply update does not duplicate", src ~= nil and #src.children == 1)

	applier:apply({ path = "src/A.luau", class = "ModuleScript", kind = "Delete" })
	check("apply delete removes node", src ~= nil and #src.children == 0)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "ui/main.client.luau", class = "LocalScript", kind = "Create", content = "print('hi')" })
	local ui = tree.findChild(root, "ui")
	local main = if ui ~= nil then tree.findChild(ui, "main") else nil
	check("apply strips .client.luau suffix", main ~= nil and main.Name == "main")
	check("apply maps LocalScript class", main ~= nil and main.ClassName == "LocalScript")
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "Utils/Helper.luau", class = "ModuleScript", kind = "Create", content = "" })
	applier:apply({ path = "Utils.luau", class = "ModuleScript", kind = "Delete" })
	local utils = tree.findChild(root, "Utils")
	check(
		"delete of a file keeps a same-named folder and its children",
		utils ~= nil and utils.ClassName == "Folder" and tree.findChild(utils, "Helper") ~= nil
	)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "missing/A.luau", class = "ModuleScript", kind = "Delete" })
	check("delete never creates missing parent folders", tree.findChild(root, "missing") == nil)
end

do
	local tree, root, applier = freshApplier()
	place(tree, "Folder", "x", root)
	place(tree, "ModuleScript", "x", root)
	applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Delete" })
	local remaining = tree.findChild(root, "x")
	check(
		"delete removes only the same-class sibling among same-named instances",
		countNamed(root, "x") == 1 and remaining ~= nil and remaining.ClassName == "Folder"
	)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "x.server.luau", class = "Script", kind = "Create", content = "s" })
	applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Update", content = "m" })
	applier:apply({ path = "x.server.luau", class = "Script", kind = "Delete" })
	local survivor = tree.findChild(root, "x")
	check(
		"Script x then batch [x.luau upsert, x.server.luau delete] leaves a ModuleScript x",
		countNamed(root, "x") == 1 and survivor ~= nil and survivor.ClassName == "ModuleScript"
	)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Create", content = "m" })
	applier:apply({ path = "x.server.luau", class = "Script", kind = "Update", content = "s" })
	local x = tree.findChild(root, "x")
	check(
		"Upsert x.server.luau over a ModuleScript x becomes a Script",
		countNamed(root, "x") == 1 and x ~= nil and x.ClassName == "Script" and x.Source == "s"
	)
end

do
	local tree, root, applier = freshApplier()
	local oldModule = place(tree, "ModuleScript", "x", root)
	place(tree, "ModuleScript", "Inner", oldModule)
	applier:apply({ path = "x.server.luau", class = "Script", kind = "Update", content = "s" })
	local x = tree.findChild(root, "x")
	check(
		"a class change moves the children of the replaced script",
		countNamed(root, "x") == 1 and x ~= nil and x.ClassName == "Script" and tree.findChild(x, "Inner") ~= nil
	)
end

do
	local tree, root, applier = freshApplier()
	applier:apply({ path = "Utils/Helper.luau", class = "ModuleScript", kind = "Create", content = "" })
	local folder = tree.findChild(root, "Utils")
	local ok, err = pcall(function()
		applier:apply({ path = "Utils.luau", class = "ModuleScript", kind = "Update", content = "print(1)" })
	end)
	local script = findByClass(root, "Utils", "ModuleScript")
	check("a same-named Folder does not block a ModuleScript", ok, tostring(err))
	check(
		"the ModuleScript Utils sits beside the Folder Utils with its source",
		countNamed(root, "Utils") == 2 and script ~= nil and script.Source == "print(1)"
	)
	check("the Folder Utils keeps its children", folder ~= nil and tree.findChild(folder, "Helper") ~= nil)
end

do
	local tree, root = studioWorld(true)
	local expected: { string } = {}
	local writeApplier = Apply.create(tree, {
		expect = function(path: string, content: string)
			table.insert(expected, path .. "=" .. content)
		end,
		forget = function(_path: string) end,
	})
	local expectedBeforeWrite = -1
	local originalSetSource = tree.setSource
	tree.setSource = function(node: any, content: string)
		expectedBeforeWrite = #expected
		originalSetSource(node, content)
	end
	writeApplier:apply({ kind = "Upsert", path = "Z.luau", class = "ModuleScript", content = "Q" })
	writeApplier:apply({ kind = "Upsert", path = "Z.luau", class = "ModuleScript", content = "Q" })
	check("apply records the expected echo before the write", expectedBeforeWrite == 1)
	check(
		"apply records the expected echo once and not for a no-op",
		#expected == 1 and expected[1] == "Z.luau=Q" and tree.findChild(root, "Z") ~= nil
	)
end

do
	local tree = studioWorld(true)
	local forgotten: { string } = {}
	tree.setSource = function(_node: any, _content: string)
		error("read only")
	end
	local failingApplier = Apply.create(tree, {
		expect = function(_path: string, _content: string) end,
		forget = function(path: string)
			table.insert(forgotten, path)
		end,
	})
	local ok = pcall(function()
		failingApplier:apply({ kind = "Upsert", path = "F.luau", class = "ModuleScript", content = "Q" })
	end)
	check("a failed source write is rolled back out of the echo record", not ok and forgotten[1] == "F.luau")
end

local extentsRegion = { CFrame = { Position = { X = 0, Y = 0, Z = 0 } }, Size = { X = 16, Y = 16, Z = 16 } }

local function extentsTerrainWith(region: any, onRead: ((any) -> ())?, onWrite: ((any) -> ())?)
	return {
		MaxExtents = {
			ExpandToGrid = function(_, _resolution)
				return region
			end,
		},
		ReadVoxels = function(_, readRegion, _resolution)
			if onRead ~= nil then
				onRead(readRegion)
			end
			return { { { { Value = 1 } } } }, { { { 0.5 } } }
		end,
		WriteVoxels = function(_, writeRegion, _resolution, _materials, _occupancies)
			if onWrite ~= nil then
				onWrite(writeRegion)
			end
		end,
	}
end

local function blobPatchOf(content: string)
	return { kind = "Upsert", direction = "ToStudio", path = "Terrain.terrain", class = "Terrain", content = content }
end

local function deps(terrain: any, tree: any?)
	return { terrain = terrain, tree = tree, apply = Apply, protocol = Protocol, terrainBlob = Terrain }
end

local function fakeConnection(client: any, terrain: any): any
	local tree, root = studioWorld(true)
	local connection = Connection.new(client, root, 1, noop, deps(terrain, tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	return connection
end

local function stateLog()
	local log: { { state: string, detail: string? } } = {}
	local function onState(state: string, detail: string?)
		table.insert(log, { state = state, detail = detail })
	end
	return log, onState
end

local function lastStateOf(log: { { state: string, detail: string? } }): string
	return if #log > 0 then log[#log].state else ""
end

do
	local readRegion: any = nil
	local expanded: number? = nil
	local posts: { any } = {}
	local terrain = extentsTerrainWith(extentsRegion, function(region)
		readRegion = region
	end)
	terrain.MaxExtents.ExpandToGrid = function(_, resolution)
		expanded = resolution
		return extentsRegion
	end
	local connection = fakeConnection({
		postBlobs = function(_, changes)
			table.insert(posts, changes)
		end,
	}, terrain)
	connection:_pushTerrain(1)
	check("terrain push expands MaxExtents to the 4-stud grid", expanded == 4)
	check("terrain push reads the MaxExtents region as-is", readRegion == extentsRegion)
	check("terrain push posts one blob", #posts == 1)
end

do
	local hugeRegion = { CFrame = { Position = { X = 0, Y = 0, Z = 0 } }, Size = { X = 1024, Y = 256, Z = 1024 } }
	local reads = 0
	local posts = 0
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new(
		{
			postBlobs = function(_, _changes)
				posts += 1
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(hugeRegion, function()
			reads += 1
		end), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_pushTerrain(1)
	local last = log[#log]
	check("terrain over the voxel cap is not read", reads == 0)
	check("terrain over the voxel cap posts nothing", posts == 0)
	check(
		"terrain over the voxel cap is reported through onState",
		last ~= nil
			and last.state == "Warning"
			and last.detail == "terrain too large for live push, use naht build",
		if last ~= nil then tostring(last.detail) else "no state reported"
	)
end

local function blobBackoffAfter(value: number): number?
	local ok, result = pcall(Connection.nextBlobBackoff, value)
	return if ok then result else nil
end

check("blob backoff doubles from 2 seconds", blobBackoffAfter(2) == 4)
check("blob backoff is capped at 60 seconds", blobBackoffAfter(32) == 60 and blobBackoffAfter(60) == 60)

do
	local calls: { number } = {}
	local connection: any = nil
	connection = fakeConnection({
		blobs = function(_, cursor)
			table.insert(calls, cursor)
			if #calls == 1 then
				return { cursor = 5, patches = { blobPatchOf(string.sub(terrainHeader(1, 1, 1, 4), 1, 20)) } }
			end
			connection.epoch += 1
			error("stop")
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	check("a failed blob keeps the blob cursor where it was", calls[2] == 0 and connection.blobCursor == 0)
end

do
	local log, onState = stateLog()
	local calls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			blobs = function(_, _cursor)
				calls += 1
				if calls == 1 then
					return { cursor = 9, patches = { blobPatchOf(string.sub(terrainHeader(1, 1, 1, 4), 1, 20)) } }
				end
				connection.epoch += 1
				error("stop")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	local reported = false
	for _, entry in log do
		if
			entry.state == "Warning"
			and entry.detail ~= nil
			and string.find(entry.detail, "terrain blob rejected: ", 1, true) == 1
		then
			reported = true
		end
	end
	check("a rejected blob is reported through onState", reported)
	check("a rejected blob keeps the blob cursor", connection.blobCursor == 0)
end

do
	local calls: { number } = {}
	local connection: any = nil
	connection = fakeConnection({
		blobs = function(_, cursor)
			table.insert(calls, cursor)
			if #calls == 1 then
				return { cursor = 7, patches = {} }
			end
			connection.epoch += 1
			error("stop")
		end,
	}, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1)
	check("an empty blob batch advances the blob cursor", calls[2] == 7)
end

do
	local writes = 0
	local connection: any = nil
	connection = fakeConnection(
		{
			blobs = function(_, _cursor)
				connection:stop()
				return { cursor = 4, patches = { blobPatchOf(terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)) } }
			end,
		},
		extentsTerrainWith(extentsRegion, nil, function()
			writes += 1
		end)
	)
	connection:_serveBlobs(1)
	check("stop during a blob poll writes no voxels", writes == 0 and connection.blobCursor == 0)
end

do
	local log, onState = stateLog()
	local infoConnection: any = nil
	local patchPolls = 0
	local blobPosts = 0
	local tree, root = studioWorld(true)
	infoConnection = Connection.new(
		{
			info = function(_, _placeId)
				infoConnection:stop()
				return { project_name = "demo" }
			end,
			patches = function(_, _cursor)
				patchPolls += 1
				return { cursor = 1, patches = {} }
			end,
			blobs = function(_, _cursor)
				return { cursor = 1, patches = {} }
			end,
			postBlobs = function(_, _changes)
				blobPosts += 1
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	infoConnection.running = true
	infoConnection.session = 1
	infoConnection:_run(1)
	check("stop during the handshake leaves the state Disconnected", lastStateOf(log) == "Disconnected", lastStateOf(log))
	check("stop during the handshake posts no terrain", blobPosts == 0)
	check("stop during the handshake polls no patches", patchPolls == 0)
end

do
	local tree, root = studioWorld(true)
	local acks = 0
	local connection: any = nil
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				connection:stop()
				return {
					cursor = 1,
					patches = { { kind = "Upsert", direction = "ToStudio", path = "Late.luau", class = "ModuleScript", content = "L" } },
				}
			end,
			postAck = function(_, _paths)
				acks += 1
			end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection:_serve(1)
	check("stop during a patch poll applies nothing", tree.findChild(root, "Late") == nil)
	check("stop during a patch poll acks nothing", acks == 0)
end

do
	local posts = 0
	local connection = fakeConnection({
		postChanges = function(_, _changes)
			posts += 1
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["x.luau"] = Protocol.delete("x.luau")
	connection:_schedulePush()
	connection:stop()
	task.wait(0.5)
	check("a pending push posts nothing after stop", posts == 0, "posts after stop: " .. tostring(posts))
end

do
	local attempts = 0
	local connection = fakeConnection({
		postChanges = function(_, _changes)
			attempts += 1
			error("daemon down")
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["x.luau"] = Protocol.delete("x.luau")
	connection:_schedulePush()
	task.wait(0.5)
	connection:stop()
	local attemptsAtStop = attempts
	task.wait(4.5)
	check(
		"a failing push is not retried after stop",
		attempts == attemptsAtStop and attemptsAtStop >= 1,
		string.format("attempts at stop %d, final %d", attemptsAtStop, attempts)
	)
end

do
	local pushed: { string } = {}
	local connection = fakeConnection({
		postChanges = function(_, changes)
			for _, change in changes do
				table.insert(pushed, change.path)
			end
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush()
	connection:stop()
	connection.running = true
	connection.session = 2
	connection.pendingPush["new.luau"] = Protocol.delete("new.luau")
	connection:_schedulePush()
	task.wait(0.5)
	check(
		"a batch queued before stop is dropped, not sent after a restart",
		table.find(pushed, "old.luau") == nil and table.find(pushed, "new.luau") ~= nil,
		table.concat(pushed, ",")
	)
end

do
	local posts = 0
	local connection = fakeConnection({
		postChanges = function(_, _changes)
			posts += 1
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush()
	connection:stop()
	connection.running = true
	connection.session = 2
	connection.pendingPush["new.luau"] = Protocol.delete("new.luau")
	connection:_schedulePush()
	task.wait(0.5)
	check("a push scheduled after a restart is sent", posts == 1, "posts: " .. tostring(posts))
end

do
	local posts = 0
	local connection = fakeConnection({
		postChanges = function(_, _changes)
			posts += 1
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush(0.1)
	connection:stop()
	connection.pendingPush["late.luau"] = Protocol.delete("late.luau")
	task.wait(0.4)
	check("a push timer that fires after stop sends nothing", posts == 0, "posts: " .. tostring(posts))
end

do
	local attempts = 0
	local posts = 0
	local connection: any = nil
	connection = fakeConnection({
		postChanges = function(_, _changes)
			attempts += 1
			if attempts == 1 then
				connection:stop()
				error("daemon down")
			end
			posts += 1
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush(0.1)
	task.wait(0.3)
	connection.running = true
	connection.session = 2
	connection.pendingPush["new.luau"] = Protocol.delete("new.luau")
	connection:_schedulePush()
	task.wait(0.5)
	check(
		"a push that fails during stop does not block the next push after restart",
		posts == 1,
		string.format("attempts %d, posts %d", attempts, posts)
	)
end

do
	local posts = 0
	local connection: any = nil
	connection = fakeConnection({
		postBlobs = function(_, _changes)
			posts += 1
		end,
	}, extentsTerrainWith(extentsRegion, function()
		connection:stop()
	end))
	connection:_pushTerrain(1)
	check("a terrain read that finishes after stop posts nothing", posts == 0, "posts: " .. tostring(posts))
end

do
	local tree, root = studioWorld(true)
	local connection: any = nil
	local fakeApply = {
		create = function()
			return {
				apply = function()
					connection:stop()
				end,
			}
		end,
	}
	local harnessDeps: any = deps(extentsTerrainWith(extentsRegion), tree)
	harnessDeps.apply = fakeApply
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				return {
					cursor = 5,
					patches = { { kind = "Upsert", direction = "ToStudio", path = "A.luau", class = "ModuleScript", content = "A" } },
				}
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		noop,
		harnessDeps
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 8
	connection:_serve(1)
	check("a stop during apply leaves the patch cursor where it was", connection.cursor == 0, "cursor " .. connection.cursor)
	check("a stop during apply leaves the reconnect backoff where it was", connection.backoff == 8, "backoff " .. connection.backoff)
end

do
	local connection: any = nil
	connection = fakeConnection({
		blobs = function(_, _cursor)
			connection:stop()
			return { cursor = 4, patches = {} }
		end,
	}, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1)
	check("a stop during a blob poll with no patches keeps the blob cursor", connection.blobCursor == 0, "cursor " .. connection.blobCursor)
end

do
	local writes = 0
	local connection: any = nil
	local blob = terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)
	connection = fakeConnection({
		blobs = function(_, _cursor)
			return { cursor = 4, patches = { blobPatchOf(blob), blobPatchOf(blob) } }
		end,
	}, extentsTerrainWith(extentsRegion, nil, function()
		writes += 1
		connection:stop()
	end))
	connection:_serveBlobs(1)
	check("a stop during a blob write applies no later blob", writes == 1, "writes " .. tostring(writes))
end

do
	local tree, root = studioWorld(true)
	local epochAtPoll = -1
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				return { project_name = "demo" }
			end,
			patches = function(_, _cursor)
				epochAtPoll = connection.epoch
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_run(1)
	check("a connect bumps the epoch before polling", epochAtPoll == 2, "epoch " .. tostring(epochAtPoll))
end

do
	local connection: any = nil
	connection = fakeConnection({
		info = function(_, _placeId)
			error("daemon down")
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.backoff = 0.05
	task.spawn(function()
		task.wait(0.02)
		connection:stop()
	end)
	connection:_run(1)
	check("a stop during the backoff wait leaves the backoff unchanged", connection.backoff == 0.05, "backoff " .. tostring(connection.backoff))
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	lockedSource(tree, "Bad", 1000)
	local acks: { { string } } = {}
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 1,
						patches = {
							{ kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
							{ kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
						},
					}
				end
				error("stop")
			end,
			postAck = function(_, paths)
				table.insert(acks, paths)
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	local last = log[#log]
	check(
		"a rejected text patch sets a Warning naming the path and reason",
		last ~= nil
			and last.state == "Warning"
			and last.detail ~= nil
			and string.find(last.detail, "patch rejected: Bad.luau: ", 1, true) == 1
			and string.find(last.detail, "Source is locked", 1, true) ~= nil,
		if last ~= nil then tostring(last.detail) else "no state reported"
	)
	check("a rejected text patch is left out of the ack", #acks == 1 and #acks[1] == 1 and acks[1][1] == "Good.luau")
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	lockedSource(tree, "Bad", 1)
	local bad = { kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" }
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return { cursor = 1, patches = { bad } }
				end
				if polls == 2 then
					return { cursor = 2, patches = { bad } }
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check("a rejected patch that later applies ends in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	lockedSource(tree, "Bad", 1)
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 1,
						patches = {
							{ kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
							{ kind = "Conflict", direction = "ToStudio", path = "C.luau", class = "ModuleScript" },
							{ kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
						},
					}
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check(
		"a conflict plus a success in one batch ends in Conflict",
		lastStateOf(log) == "Conflict" and log[#log].detail == "C.luau",
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 1,
						patches = { { kind = "Conflict", direction = "ToStudio", path = "C.luau", class = "ModuleScript" } },
					}
				end
				if polls == 2 then
					return {
						cursor = 2,
						patches = { { kind = "Upsert", direction = "ToStudio", path = "C.luau", class = "ModuleScript", content = "C" } },
					}
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check(
		"a later success on the conflicted path ends the Conflict",
		lastStateOf(log) == "Connected",
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new({}, root, 1, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection:_setWarning("terrain", "terrain too large for live push, use naht build")
	connection:_setConflict("C.luau")
	check(
		"a Conflict is reported while a permanent Warning is set",
		lastStateOf(log) == "Conflict" and log[#log].detail == "C.luau",
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local calls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			blobs = function(_, _cursor)
				calls += 1
				if calls == 1 then
					return { cursor = 1, patches = { blobPatchOf(string.sub(terrainHeader(1, 1, 1, 4), 1, 20)) } }
				end
				if calls == 2 then
					return { cursor = 2, patches = { blobPatchOf(terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)) } }
				end
				connection.epoch += 1
				error("superseded")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	check("a blob that later applies ends the Warning in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
	check("a blob that later applies resets the blob backoff", connection.blobBackoff == 2, "blob backoff " .. tostring(connection.blobBackoff))
end

do
	local tree, root = studioWorld(true)
	local arrivals = {}
	local calls = 0
	local client = {
		postBlobs = function(_, _blobs)
			calls += 1
			local call = calls
			if call == 1 then
				task.wait(0.05)
			end
			table.insert(arrivals, call)
		end,
		postAck = function() end,
	}
	local connection = Connection.new(client, root, 1, noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.epoch = 1
	task.spawn(function()
		connection:_pushTerrain(1)
	end)
	task.wait(0.01)
	connection.epoch = 2
	task.spawn(function()
		connection:_pushTerrain(2)
	end)
	task.wait(0.15)
	connection.running = false
	check(
		"the newest terrain snapshot arrives last across an epoch change",
		#arrivals == 2 and arrivals[2] == 2,
		table.concat(arrivals, ",")
	)
end

do
	local log, onState = stateLog()
	local calls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			blobs = function(_, _cursor)
				calls += 1
				if calls <= 5 then
					error({ undecodable = true, message = "naht: undecodable blob batch: bad header" })
				end
				connection.epoch += 1
				error("superseded")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	local last = log[#log]
	check(
		"a batch that keeps failing to decode is reported as still retrying",
		last ~= nil
			and last.state == "Warning"
			and string.find(last.detail or "", "terrain blob batch at cursor 0 keeps failing to decode, still retrying", 1, true) ~= nil,
		last and (last.state .. ": " .. (last.detail or "")) or "no state reported"
	)
	check(
		"the stalled wording no longer tells the user to restart the daemon",
		last ~= nil and string.find(last.detail or "", "restart", 1, true) == nil,
		last and (last.detail or "") or "no state reported"
	)
end

do
	local log, onState = stateLog()
	local calls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			blobs = function(_, _cursor)
				calls += 1
				if calls <= 5 then
					error("naht: undecodable blob batch: bad header")
				end
				connection.epoch += 1
				error("superseded")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	local last = log[#log]
	check(
		"a plain error that only mentions an undecodable batch is not counted as a decode failure",
		last ~= nil
			and last.state == "Warning"
			and string.find(last.detail or "", "terrain blob poll failed: ", 1, true) ~= nil
			and string.find(last.detail or "", "stalled", 1, true) == nil,
		last and (last.state .. ": " .. (last.detail or "")) or "no state reported"
	)
end

do
	local log, onState = stateLog()
	local calls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			blobs = function(_, _cursor)
				calls += 1
				if calls <= 5 then
					error({ undecodable = true, message = "naht: undecodable blob batch: bad header" })
				end
				if calls == 6 then
					return { cursor = 1, patches = {} }
				end
				connection.epoch += 1
				error("superseded")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	check("a later successful poll clears the stalled blob warning", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local attempts = 0
	local seenDelays: { number } = {}
	local keptEveryTime = true
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				attempts += 1
				table.insert(seenDelays, connection.pushBackoff)
				local sawOld = false
				for _, change in changes do
					if change.path == "old.luau" then
						sawOld = true
					end
				end
				if not sawOld then
					keptEveryTime = false
				end
				if attempts <= 3 then
					error("naht: /changes returned 503")
				end
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush()
	task.wait(0.6)
	check(
		"a 503 on changes keeps the batch and retries it",
		attempts == 4 and keptEveryTime,
		string.format("attempts %d, kept %s", attempts, tostring(keptEveryTime))
	)
	check(
		"each retry after a 503 on changes waits twice as long",
		seenDelays[1] == 0.01 and seenDelays[2] == 0.02 and seenDelays[3] == 0.04 and seenDelays[4] == 0.08,
		table.concat(seenDelays, ",")
	)
	check("a successful push resets the push backoff", connection.pushBackoff == 2, tostring(connection.pushBackoff))
	local warned: string? = nil
	for _, entry in log do
		if entry.state == "Warning" and warned == nil then
			warned = entry.detail
		end
	end
	check(
		"a 503 on changes sets a Warning naming the reason",
		warned ~= nil
			and string.find(warned, "push failed: ", 1, true) ~= nil
			and string.find(warned, "naht: /changes returned 503", 1, true) ~= nil,
		tostring(warned)
	)
	check("a successful push after a 503 ends in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local acks: { { string } } = {}
	local ackAttempts = 0
	local patchCalls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				patchCalls += 1
				if patchCalls == 1 then
					return {
						cursor = 1,
						patches = {
							{ kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
						},
					}
				end
				if patchCalls == 2 then
					return { cursor = 1, patches = {} }
				end
				error("stop")
			end,
			postAck = function(_, paths)
				ackAttempts += 1
				if ackAttempts == 1 then
					error("naht: /ack returned 503")
				end
				table.insert(acks, paths)
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.ackBackoff = 0.01
	pcall(connection._serve, connection, 1)
	check(
		"a 503 on ack keeps the applied paths and retries them",
		ackAttempts == 2 and #acks == 1 and #acks[1] == 1 and acks[1][1] == "Good.luau",
		string.format("attempts %d, acks %d", ackAttempts, #acks)
	)
	local warned = false
	for _, entry in log do
		local detail = entry.detail or ""
		if
			entry.state == "Warning"
			and string.find(detail, "ack failed: ", 1, true) ~= nil
			and string.find(detail, "naht: /ack returned 503", 1, true) ~= nil
		then
			warned = true
		end
	end
	check("a 503 on ack sets a Warning naming the reason", warned, lastStateOf(log))
	check("a successful ack retry ends in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				return { protocol_version = 1, project_name = "demo", conflicted = { "Frozen.luau" } }
			end,
			patches = function(_, _cursor)
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				task.wait()
				return { cursor = 0, patches = {} }
			end,
			postBlobs = function() end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_run(1)
	local conflictSeen = false
	local connectedSeen = false
	for _, entry in log do
		if entry.state == "Conflict" and entry.detail == "Frozen.luau" then
			conflictSeen = true
		end
		if entry.state == "Connected" then
			connectedSeen = true
		end
	end
	check(
		"a connect with a frozen path reports it as a Conflict and never Connected",
		conflictSeen and not connectedSeen,
		"states: " .. table.concat((function()
			local names = {}
			for _, entry in log do
				table.insert(names, entry.state .. "(" .. tostring(entry.detail) .. ")")
			end
			return names
		end)(), " ")
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new({}, root, 1, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.epoch = 1
	connection:_applyBlobBatch(1, { cursor = 1, patches = { { kind = "Conflict", direction = "ToStudio", path = "Terrain.terrain", class = "Terrain" } } })
	check("a blob Conflict is reported", lastStateOf(log) == "Conflict", lastStateOf(log))
	connection:_applyBlobBatch(1, { cursor = 2, patches = { blobPatchOf(terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)) } })
	check("a later blob success on the same path ends the blob Conflict", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local region = { CFrame = { Position = { X = 0, Y = 0, Z = 0 } }, Size = { X = 1024, Y = 256, Z = 1024 } }
	local posts = 0
	local tree, root = studioWorld(true)
	local connection = Connection.new(
		{
			postBlobs = function(_, _changes)
				posts += 1
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(region), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_pushTerrain(1)
	region.Size = { X = 8, Y = 8, Z = 8 }
	connection:_pushTerrain(1)
	check("a terrain push under the cap clears the too-large Warning", lastStateOf(log) == "Connected", lastStateOf(log))
	check("a terrain push under the cap posts its blob", posts == 1, "posts " .. tostring(posts))
end

do
	local tree, root = studioWorld(true)
	local acks = 0
	local connection: any = nil
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				connection:stop()
				return {
					cursor = 3,
					patches = { { kind = "Upsert", direction = "ToStudio", path = "A.luau", class = "ModuleScript", content = "A" } },
				}
			end,
			postAck = function(_, _paths)
				acks += 1
			end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection:_serve(1)
	check("a stop between patch poll and apply writes nothing", countNamed(root, "A") == 0, "A count " .. countNamed(root, "A"))
	check("a stop between patch poll and apply acks nothing", acks == 0, "acks " .. tostring(acks))
end

do
	local writes = 0
	local connection: any = nil
	connection = fakeConnection({
		blobs = function(_, _cursor)
			connection:stop()
			return { cursor = 4, patches = { blobPatchOf(terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)) } }
		end,
	}, extentsTerrainWith(extentsRegion, nil, function()
		writes += 1
	end))
	connection:_serveBlobs(1)
	check("a stop between blob poll and apply writes no blob", writes == 0, "writes " .. tostring(writes))
	check("a stop between blob poll and apply keeps the blob cursor", connection.blobCursor == 0, "cursor " .. tostring(connection.blobCursor))
end

local function warningsOf(log: { { state: string, detail: string? } }): { string }
	local found = {}
	for _, entry in log do
		if entry.state == "Warning" then
			table.insert(found, entry.detail or "")
		end
	end
	return found
end

local function terrainConnection(client: any, onState: (string, string?) -> (), terrain: any): any
	local tree, root = studioWorld(true)
	local connection = Connection.new(client, root, 1, onState, deps(terrain, tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	return connection
end

do
	local attempts = 0
	local accepted: { any } = {}
	local connection = fakeConnection({
		postBlobs = function(_, changes)
			attempts += 1
			if attempts == 1 then
				error("daemon down")
			end
			table.insert(accepted, changes)
		end,
	}, extentsTerrainWith(extentsRegion))
	connection.terrainBackoff = 0.01
	connection:_pushTerrain(1)
	check("a failed first terrain post is retried", attempts == 2, "attempts " .. tostring(attempts))
	check("the retried terrain snapshot is accepted once", #accepted == 1, "accepted " .. tostring(#accepted))
	check(
		"the accepted snapshot is the terrain read from the region",
		#accepted == 1 and accepted[1][1].content == Terrain.read(extentsTerrainWith(extentsRegion), extentsRegion, 4)
	)
	check("the terrain backoff returns to its start after acceptance", connection.terrainBackoff == 1, tostring(connection.terrainBackoff))
end

do
	local attempts = 0
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		postBlobs = function(_, _changes)
			attempts += 1
			error("daemon down")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection.terrainBackoff = 0.2
	task.spawn(function()
		task.wait(0.05)
		connection:stop()
	end)
	connection:_pushTerrain(1)
	check("a stop during the terrain retry wait posts no further attempt", attempts == 1, "attempts " .. tostring(attempts))
	check("a stop during the terrain retry leaves Disconnected as the last state", lastStateOf(log) == "Disconnected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		postBlobs = function(_, _changes)
			connection:stop()
			error("daemon down")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection:_pushTerrain(1)
	check(
		"a terrain post that fails during stop sets no Warning",
		#warningsOf(log) == 0 and lastStateOf(log) == "Disconnected",
		lastStateOf(log)
	)
end

do
	local attempts = 0
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		postBlobs = function(_, _changes)
			attempts += 1
			error("daemon down")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection.terrainBackoff = 0.01
	task.spawn(function()
		task.wait(0.05)
		connection:stop()
	end)
	connection:_pushTerrain(1)
	local found = warningsOf(log)
	check(
		"a terrain post that keeps failing sets a Warning naming the reason",
		#found >= 2
			and string.find(found[1], "terrain push failed: ", 1, true) ~= nil
			and string.find(found[1], "daemon down", 1, true) ~= nil,
		"warnings " .. tostring(#found)
	)
end

do
	local attempts = 0
	local log, onState = stateLog()
	local connection = terrainConnection({
		postBlobs = function(_, _changes)
			attempts += 1
			if attempts == 1 then
				error("daemon down")
			end
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection.terrainBackoff = 0.01
	connection:_pushTerrain(1)
	check("the failed terrain post raises one Warning", #warningsOf(log) == 1, "warnings " .. tostring(#warningsOf(log)))
	check("a terrain post that succeeds clears the Warning", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local polls = 0
	local connection: any = nil
	connection = terrainConnection({
		blobs = function(_, _cursor)
			polls += 1
			if polls > 1 then
				connection.epoch += 1
			end
			error("boom")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	local last = log[#log]
	check("a blob poll failure while running is retried until superseded", polls == 2, "polls " .. tostring(polls))
	check(
		"a blob poll failure while running sets a Warning naming the reason",
		last ~= nil
			and last.state == "Warning"
			and last.detail ~= nil
			and string.find(last.detail, "terrain blob poll failed: ", 1, true) ~= nil
			and string.find(last.detail, "boom", 1, true) ~= nil,
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		blobs = function(_, _cursor)
			connection:stop()
			error("boom")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1)
	check(
		"a blob poll failure after stop sets no Warning",
		#warningsOf(log) == 0 and lastStateOf(log) == "Disconnected",
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		blobs = function(_, _cursor)
			connection.epoch += 1
			error("boom")
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1)
	check("a blob poll failure after a reconnect sets no Warning", #warningsOf(log) == 0, "warnings " .. tostring(#warningsOf(log)))
end

do
	local log, onState = stateLog()
	local calls = 0
	local connection: any = nil
	connection = terrainConnection({
		blobs = function(_, _cursor)
			calls += 1
			if calls == 1 then
				error("timeout")
			end
			if calls > 2 then
				connection.epoch += 1
			end
			return { cursor = 6, patches = {} }
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection.blobBackoff = 0.01
	connection:_serveBlobs(1)
	local warned = false
	local clearedAfterWarning = false
	for _, entry in log do
		if entry.state == "Warning" and entry.detail ~= nil and string.find(entry.detail, "terrain blob poll failed: ", 1, true) == 1 then
			warned = true
		elseif warned and entry.state == "Connected" then
			clearedAfterWarning = true
		end
	end
	check("a transient blob poll failure is retried", calls >= 2, "calls " .. tostring(calls))
	check("the blob cursor advances once the retry succeeds", connection.blobCursor == 6, "cursor " .. tostring(connection.blobCursor))
	check("the blob poll Warning is set, then cleared by the retry", warned and clearedAfterWarning and lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local connection: any = nil
	connection = terrainConnection({
		blobs = function(_, _cursor)
			connection:stop()
			return { cursor = 1, patches = {} }
		end,
	}, onState, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1)
	check(
		"a stop during a blob poll sets no Warning",
		#warningsOf(log) == 0 and lastStateOf(log) == "Disconnected",
		lastStateOf(log)
	)
end

do
	local log, onState = stateLog()
	local connection: any = nil
	local blob = terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)
	connection = terrainConnection({
		blobs = function(_, _cursor)
			return { cursor = 1, patches = { blobPatchOf(blob) } }
		end,
	}, onState, extentsTerrainWith(extentsRegion, nil, function()
		connection:stop()
	end))
	connection.warnings["blob"] = "terrain blob rejected: earlier"
	connection:_serveBlobs(1)
	check(
		"a stop during a blob write leaves Disconnected as the last state, not a refreshed state",
		lastStateOf(log) == "Disconnected",
		lastStateOf(log)
	)
end

do
	local voxels = Terrain.voxelCount(1024, 1024, 64, 4)
	local blob = string.pack("<i4i4i4fffffff", 256, 256, 16, 0, 0, 0, 1, 1, 1, 4) .. string.rep("\0", 6 * voxels)
	local body = Protocol.encodeBlobs({ { path = "Terrain.terrain", class = "Terrain", content = blob } })
	local daemonBodyLimit = 64 * 1024 * 1024
	check("the largest region the cap admits is exactly MAX_VOXELS", voxels == Terrain.MAX_VOXELS, "voxels " .. tostring(voxels))
	check("the worst-case blob is a 40-byte header plus 6 bytes per voxel", #blob == 40 + 6 * Terrain.MAX_VOXELS, "bytes " .. tostring(#blob))
	check("the MessagePack envelope around the worst-case blob is under 256 bytes", #body - #blob < 256, "envelope " .. tostring(#body - #blob))
	check("the worst-case terrain body is under the 64 MiB daemon body limit", #body < daemonBodyLimit, "body " .. tostring(#body))
end

local function echoConnection(syncEcho: boolean)
	local tree, root, flush = studioWorld(syncEcho)
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 1,
						patches = { { kind = "Upsert", direction = "ToStudio", path = "Z.luau", class = "ModuleScript", content = "A" } },
					}
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection:_attachStudio()
	pcall(connection._serve, connection, 1)
	return connection, tree.findChild(root, "Z"), flush, tree, root
end

local function editStudio(node: any, content: string)
	node.Source = content
	node.sourceChanged:Fire()
end

do
	local connection, node = echoConnection(true)
	editStudio(node, "B")
	editStudio(node, "A")
	local pushed = connection.pendingPush["Z.luau"]
	check(
		"sync echo: edit to B then revert to A pushes A",
		pushed ~= nil and pushed.content == "A",
		if pushed ~= nil then "pushed " .. tostring(pushed.content) else "nothing pushed"
	)
	connection:stop()
end

do
	local connection, node, flush = echoConnection(false)
	flush()
	editStudio(node, "B")
	editStudio(node, "A")
	local pushed = connection.pendingPush["Z.luau"]
	check(
		"deferred echo: edit to B then revert to A pushes A",
		pushed ~= nil and pushed.content == "A",
		if pushed ~= nil then "pushed " .. tostring(pushed.content) else "nothing pushed"
	)
	connection:stop()
end

do
	local connection, _node, flush = echoConnection(false)
	flush()
	check("a deferred echo of the applied source is dropped", connection.pendingPush["Z.luau"] == nil)
	connection:stop()
end

do
	local connection, node, flush, tree, root = echoConnection(false)
	flush()
	tree.destroy(node)
	local recreated = tree.createInstance("ModuleScript", "Z")
	recreated.Source = "A"
	tree.setParent(recreated, root)
	local pushed = connection.pendingPush["Z.luau"]
	check(
		"a script deleted and recreated with its applied source is pushed",
		pushed ~= nil and pushed.kind == "Upsert" and pushed.content == "A",
		if pushed ~= nil then "pushed " .. tostring(pushed.kind) else "nothing pushed"
	)
	connection:stop()
end

do
	local connection = echoConnection(true)
	check("a synchronous echo of the applied source is dropped", connection.pendingPush["Z.luau"] == nil)
	connection:stop()
end

do
	local tree, root = studioWorld(true)
	local acked: { string } = {}
	local blockedPolls = 0
	place(tree, "Folder", "Utils", root)
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				blockedPolls += 1
				if blockedPolls == 1 then
					return {
						cursor = 1,
						patches = {
							{ kind = "Upsert", direction = "ToStudio", path = "Utils.luau", class = "ModuleScript", content = "x" },
							{ kind = "Upsert", direction = "ToStudio", path = "Other.luau", class = "ModuleScript", content = "y" },
						},
					}
				end
				error("stop")
			end,
			postAck = function(_, paths)
				acked = paths
			end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check(
		"a same-named folder does not block a patch, so it is acked",
		#acked == 2 and acked[1] == "Utils.luau" and acked[2] == "Other.luau",
		"acked " .. table.concat(acked, ",")
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new(
		{
			info = function(_, _placeId)
				error("offline")
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection:stop()
	check("stop reports Disconnected", lastStateOf(log) == "Disconnected")
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new({}, root, 1, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection:_setWarning("blob", "terrain blob rejected: stale")
	connection.conflicts["C.luau"] = true
	connection:stop()
	check("stop clears the Warnings left by the last run", next(connection.warnings) == nil)
	check("stop clears the Conflicts left by the last run", next(connection.conflicts) == nil)
	connection:_refreshState()
	check("the first state after a restart is not a stale Warning", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local tree, root = studioWorld(true)
	local lifecycle = Connection.new({}, root, 1, noop, deps(extentsTerrainWith(extentsRegion), tree))
	lifecycle:start()
	check("start attaches one set of Studio listeners", root.DescendantAdded:count() == 1)
	lifecycle:stop()
	check("stop detaches the Studio listeners", root.DescendantAdded:count() == 0)
	lifecycle:start()
	lifecycle:start()
	check("restart does not stack Studio listeners", root.DescendantAdded:count() == 1)
	lifecycle:stop()
	check("second stop detaches the Studio listeners", root.DescendantAdded:count() == 0)
end

do
	local tree, root = studioWorld(true)
	local infos = 0
	local client = {
		info = function(_, _placeId)
			infos += 1
			return { project_name = "Demo" }
		end,
		patches = function(_, _cursor)
			task.wait(0.01)
			return { cursor = 0, patches = {} }
		end,
		blobs = function(_, _cursor)
			task.wait(0.01)
			return { cursor = 0, patches = {} }
		end,
		postAck = function() end,
		postBlobs = function() end,
		postChanges = function() end,
	}
	local lifecycle = Connection.new(client, root, 1, noop, deps(extentsTerrainWith(extentsRegion), tree))
	lifecycle:start()
	task.wait(0.05)
	lifecycle:stop()
	lifecycle:start()
	task.wait(0.05)
	lifecycle:stop()
	check("start after a stop reconnects", infos == 2, infos)
end

do
	local tree, root = studioWorld(true)
	local connection = Connection.new({}, root, 1, noop, deps(extentsTerrainWith(extentsRegion), tree))
	local dir = place(tree, "Folder", "dir", root)
	check(
		"path of a plain script keeps its folders",
		connection:_pathOf(place(tree, "ModuleScript", "ok", dir)) == "dir/ok.luau"
	)
	check("path skips a name containing a slash", connection:_pathOf(place(tree, "ModuleScript", "a/b", root)) == nil)
	check("path skips a name containing a backslash", connection:_pathOf(place(tree, "ModuleScript", "a\\b", root)) == nil)
	check("path skips a name containing NUL", connection:_pathOf(place(tree, "ModuleScript", "a\0b", root)) == nil)
	check(
		"path skips a folder whose name contains a slash",
		connection:_pathOf(place(tree, "ModuleScript", "leaf", place(tree, "Folder", "x/y", root))) == nil
	)
	check("path skips a ModuleScript named x.server", connection:_pathOf(place(tree, "ModuleScript", "x.server", root)) == nil)
	check("path skips a ModuleScript named x.client", connection:_pathOf(place(tree, "ModuleScript", "x.client", root)) == nil)
	check("path skips a ModuleScript named x.luau", connection:_pathOf(place(tree, "ModuleScript", "x.luau", root)) == nil)
end

do
	local tree, root = studioWorld(true)
	local connection = Connection.new({}, root, 1, noop, deps(extentsTerrainWith(extentsRegion), tree))
	local refusedFolderNames = { "Config: v2", "a:b", ":", ".", "..", "", "a\0b" }
	for _, name in refusedFolderNames do
		local leaf = place(tree, "ModuleScript", "Leaf", place(tree, "Folder", name, root))
		check(string.format("path skips a folder named %q, as the daemon refuses it", name), connection:_pathOf(leaf) == nil)
	end
	local acceptedFolderNames = { "Config v2", "v2.0", "a-b_c", ".x", "..x" }
	for _, name in acceptedFolderNames do
		local leaf = place(tree, "ModuleScript", "Leaf", place(tree, "Folder", name, root))
		check(
			string.format("path keeps a folder named %q, as the daemon accepts it", name),
			connection:_pathOf(leaf) == name .. "/Leaf.luau"
		)
	end
end

do
	local tree, root = studioWorld(true)
	local warnings: { string } = {}
	local dependencies = deps(extentsTerrainWith(extentsRegion), tree)
	dependencies.warn = function(message: string)
		table.insert(warnings, message)
	end
	local connection = Connection.new({}, root, 1, noop, dependencies)
	local suffixed = place(tree, "ModuleScript", "x.server", root)
	connection:_pathOf(suffixed)
	connection:_pathOf(suffixed)
	check("a script-suffixed name is warned once", #warnings == 1, "warnings: " .. tostring(#warnings))
end

do
	local tree, root = studioWorld(true)
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return { cursor = 1, patches = {} }
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 8
	pcall(connection._serve, connection, 1)
	check("a completed poll resets the reconnect backoff", connection.backoff == 1)
end

do
	local tree, root = studioWorld(true)
	place(tree, "ModuleScript", "Mod", root).Source = "X"
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return { cursor = 1, patches = { { kind = "Upsert", path = "Mod.luau", class = "ModuleScript", content = "X" } } }
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check("an apply that changes nothing records no echo", connection.lastApplied["Mod.luau"] == nil)
end

do
	local tree, root = studioWorld(false)
	local polls = 0
	local connection = Connection.new(
		{
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return { cursor = 1, patches = { { kind = "Upsert", path = "New.luau", class = "ModuleScript", content = "Y" } } }
				end
				error("stop")
			end,
			postAck = function(_, _paths) end,
		},
		root,
		1,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	pcall(connection._serve, connection, 1)
	check("an apply that writes records its echo", connection.lastApplied["New.luau"] == "Y")
end

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
	check(
		"compiles " .. path,
		pcall(function()
			luau.load(fs.readFile(path))
		end)
	)
end

local function textPushProbe(failFirst: boolean)
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local events: { string } = {}
	local accepted: { string } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				local content = changes[1].content
				table.insert(events, "start " .. content)
				local slow = #events == 1
				task.wait(if slow then 0.5 else 0.05)
				table.insert(events, "end " .. content)
				if failFirst and slow then
					error("naht: /changes returned 503")
				end
				table.insert(accepted, content)
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["x.luau"] = Protocol.upsert("x.luau", "ModuleScript", "v1")
	connection:_schedulePush()
	local waited = 0
	while #events == 0 and waited < 100 do
		task.wait(0.01)
		waited += 1
	end
	connection.pendingPush["x.luau"] = Protocol.upsert("x.luau", "ModuleScript", "v2")
	connection:_schedulePush()
	task.wait(3.5)
	connection.running = false
	return events, accepted
end

do
	local events, accepted = textPushProbe(true)
	local overlapped = false
	local open = 0
	for _, event in events do
		if string.sub(event, 1, 5) == "start" then
			open += 1
			if open > 1 then
				overlapped = true
			end
		else
			open -= 1
		end
	end
	check(
		"an edit during an in-flight text push never starts a second post",
		not overlapped,
		table.concat(events, " | ")
	)
	check(
		"after the in-flight text push fails with a 503 the daemon ends with the newest edit",
		accepted[#accepted] == "v2",
		"accepted: " .. table.concat(accepted, ",")
	)
end

do
	local events = textPushProbe(false)
	check(
		"a text push waits for the in-flight post to return before the next one starts",
		table.concat(events, " | ") == "start v1 | end v1 | start v2 | end v2",
		table.concat(events, " | ")
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local posts: { { string } } = {}
	local accepted: { string } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				local paths = {}
				for _, change in changes do
					table.insert(paths, change.path)
				end
				table.insert(posts, paths)
				if table.find(paths, "Config: v2/Leaf.luau") ~= nil then
					error({
						status = 400,
						message = "naht: /changes returned 400",
						refused = { "Config: v2/Leaf.luau" },
					})
				end
				for _, path in paths do
					table.insert(accepted, path)
				end
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["Good.luau"] = Protocol.upsert("Good.luau", "ModuleScript", "G")
	connection.pendingPush["Config: v2/Leaf.luau"] = Protocol.upsert("Config: v2/Leaf.luau", "ModuleScript", "B")
	connection:_schedulePush()
	task.wait(0.8)
	connection.running = false
	local refusedPosts = 0
	for _, paths in posts do
		if table.find(paths, "Config: v2/Leaf.luau") ~= nil then
			refusedPosts += 1
		end
	end
	check(
		"a path the daemon refuses is posted once and never retried",
		refusedPosts == 1,
		string.format("posted %d times", refusedPosts)
	)
	check(
		"the good path in the same batch is accepted after the refusal",
		table.find(accepted, "Good.luau") ~= nil,
		table.concat(accepted, ",")
	)
	local warnedText = table.concat(warningsOf(log), "\n")
	check(
		"the refused path is named in a Warning",
		string.find(warnedText, "Config: v2/Leaf.luau", 1, true) ~= nil,
		warnedText
	)
	check(
		"the refused path is not merged back into the pending pushes",
		connection.pendingPush["Config: v2/Leaf.luau"] == nil,
		"still pending"
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local attempts = 0
	local keptEveryTime = true
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				attempts += 1
				if #changes ~= 1 or changes[1].path ~= "old.luau" then
					keptEveryTime = false
				end
				if attempts <= 2 then
					error({ status = 503, message = "naht: /changes returned 503", refused = {} })
				end
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["old.luau"] = Protocol.delete("old.luau")
	connection:_schedulePush()
	task.wait(0.6)
	connection.running = false
	check(
		"a 503 raised as a status error keeps the batch and retries it",
		attempts == 3 and keptEveryTime,
		string.format("attempts %d, kept %s", attempts, tostring(keptEveryTime))
	)
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local attempts = 0
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, _changes)
				attempts += 1
				error({ status = 400, message = "naht: /changes returned 400", refused = {} })
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B")
	connection:_schedulePush()
	task.wait(0.6)
	connection.running = false
	check(
		"a 400 that names no path drops the batch after one attempt",
		attempts == 1 and next(connection.pendingPush) == nil,
		string.format("attempts %d", attempts)
	)
	local refusalText = table.concat(warningsOf(log), "\n")
	check(
		"a 400 that names no path sets a Warning with the refusal",
		string.find(refusalText, "naht: /changes returned 400", 1, true) ~= nil,
		refusalText
	)
end

do
	local log, onState = stateLog()
	local ackAttempts = 0
	local patchCalls = 0
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				patchCalls += 1
				if patchCalls == 1 then
					return {
						cursor = 1,
						patches = {
							{ kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
						},
					}
				end
				if patchCalls == 2 then
					return { cursor = 1, patches = {} }
				end
				error("stop")
			end,
			postAck = function(_, _paths)
				ackAttempts += 1
				error({ status = 400, message = "naht: /ack returned 400", refused = {} })
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.ackBackoff = 0.01
	pcall(connection._serve, connection, 1)
	check("a refused ack is not retried", ackAttempts == 1, string.format("attempts %d", ackAttempts))
	local refusedText = table.concat(warningsOf(log), "\n")
	check(
		"a refused ack sets a Warning naming the refusal",
		string.find(refusedText, "ack refused: naht: /ack returned 400", 1, true) ~= nil,
		refusedText
	)
end

do
	local log, onState = stateLog()
	local ackAttempts = 0
	local patchCalls = 0
	local lengths: { number } = {}
	local tree, root = studioWorld(true)
	local connection: any = nil
	connection = Connection.new(
		{
			patches = function(_, _cursor)
				patchCalls += 1
				if patchCalls == 1 then
					local same = { kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" }
					return { cursor = 1, patches = { same, same } }
				end
				if patchCalls == 2 or patchCalls == 3 then
					return { cursor = 1, patches = {} }
				end
				error("stop")
			end,
			postAck = function(_, paths)
				ackAttempts += 1
				table.insert(lengths, #paths)
				if ackAttempts <= 2 then
					error({ status = 503, message = "naht: /ack returned 503", refused = {} })
				end
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.ackBackoff = 0.01
	pcall(connection._serve, connection, 1)
	check(
		"a transient ack failure retries one entry per path",
		ackAttempts == 3 and lengths[1] == 1 and lengths[2] == 1 and lengths[3] == 1,
		table.concat(lengths, ",")
	)
	check("a transient ack retry ends in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
end

do
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local posts: { string } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				local content = changes[1].content
				table.insert(posts, content)
				if content == "v1" then
					task.wait(0.3)
					error({ status = 400, message = "naht: /changes returned 400", refused = { "Bad.luau" } })
				end
			end,
		},
		root,
		1,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "v1")
	connection:_schedulePush()
	local waited = 0
	while #posts == 0 and waited < 100 do
		task.wait(0.01)
		waited += 1
	end
	connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "v2")
	connection:_schedulePush()
	task.wait(1)
	connection.running = false
	check(
		"an edit made during a refused post is sent once the refusal drops the only path",
		table.concat(posts, ",") == "v1,v2",
		"posts: " .. table.concat(posts, ",")
	)
end

do
	local events: { string } = {}
	local accepted: { string } = {}
	local function clientFor(slow: boolean)
		return {
			postChanges = function(_, changes)
				local content = changes[1].content
				table.insert(events, "start " .. content)
				task.wait(if slow then 0.5 else 0.05)
				table.insert(events, "end " .. content)
				table.insert(accepted, content)
			end,
		}
	end
	local tree, root = studioWorld(true)
	local _, onState = stateLog()
	local old = Connection.new(clientFor(true), root, 1, onState, deps(extentsTerrainWith(extentsRegion), tree))
	old.running = true
	old.session = 1
	old.epoch = 1
	old.pushBackoff = 0.01
	old.pendingPush["x.luau"] = Protocol.upsert("x.luau", "ModuleScript", "v1")
	old:_schedulePush()
	local waited = 0
	while #events == 0 and waited < 100 do
		task.wait(0.01)
		waited += 1
	end
	old:stop()
	local fresh = Connection.new(clientFor(false), root, 1, onState, deps(extentsTerrainWith(extentsRegion), tree))
	fresh.running = true
	fresh.session = 1
	fresh.epoch = 1
	fresh.pushBackoff = 0.01
	fresh.pendingPush["x.luau"] = Protocol.upsert("x.luau", "ModuleScript", "v2")
	fresh:_schedulePush()
	task.wait(1.5)
	fresh.running = false
	check(
		"after a stop and start the new connection waits for the old post before its own",
		table.concat(events, " | ") == "start v1 | end v1 | start v2 | end v2",
		table.concat(events, " | ")
	)
	check(
		"after a stop and start the daemon ends with the newest edit",
		accepted[#accepted] == "v2",
		"accepted: " .. table.concat(accepted, ",")
	)
end

if failures > 0 then
	error(string.format("%d harness check(s) failed", failures))
end
print("all harness checks passed")
