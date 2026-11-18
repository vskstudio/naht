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
local Client = require("../src/Client")
local Fingerprint = require("../src/Fingerprint")
local Connection = require("../src/Connection")
local Support = require("../src/Support")
local fs = require("@lune/fs")
local serde = require("@lune/serde")

local GOLDEN = {
	server_info = "8ab070726f746f636f6c5f76657273696f6e04ae7365727665725f76657273696f6ea5302e312e30ac70726f6a6563745f6e616d65a464656d6faa70726f6a6563745f6964a3702d31ae746f6b656e5f7265717569726564c3aa73657373696f6e5f6964a3616263a96461656d6f6e5f6964a3642d31ae73657276655f706c6163655f69647baa636f6e666c696374656490ac7465727261696e5f73796e63c2",
	patch_batch = "82a6637572736f7202a7706174636865739186a373657101a470617468aa7372632f412e6c756175a5636c617373ac4d6f64756c65536372697074a9646972656374696f6ea8546f53747564696fa46b696e64a6557064617465a7636f6e74656e74a872657475726e2031",
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

local cleanups: { () -> () } = {}
local WAIT_LIMIT = 5

local function onCleanup(release: () -> ())
	table.insert(cleanups, release)
end

local function suite(name: string, body: () -> ())
	local ok, err = pcall(body)
	for _, release in cleanups do
		pcall(release)
	end
	table.clear(cleanups)
	if not ok then
		failures += 1
		print("FAIL - " .. name .. ": error: " .. tostring(err))
	end
end

local function waitFor(condition: () -> boolean): boolean
	local waited = 0
	while not condition() do
		if waited >= WAIT_LIMIT then
			return false
		end
		task.wait(0.05)
		waited += 0.05
	end
	return true
end

local function fromHex(hex: string): string
	return (hex:gsub("..", function(pair)
		return string.char(tonumber(pair, 16) :: number)
	end))
end

local function noop() end

suite("the server_info golden decodes to the expected fields and a matching version is accepted", function()
	local info = MessagePack.decode(fromHex(GOLDEN.server_info))
	check("server_info protocol_version", info.protocol_version == 4)
	check("server_info project_name", info.project_name == "demo")
	check("server_info serve_place_id", info.serve_place_id == 123)
	check("server_info daemon_id", info.daemon_id == "d-1")
	check("matching protocol version is accepted", Protocol.assertCompatible(info).protocol_version == 4)
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
end)

suite("the patch and change golden batches decode to the expected fields", function()
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
end)

suite("the blob golden decodes to the expected path and bytes", function()
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
end)

suite("binary and blob pushes keep their raw bytes through encode and decode", function()
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
end)

suite("a terrain grid survives serialize and deserialize", function()
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
end)

local function terrainHeader(nx: number, ny: number, nz: number, resolution: number): string
	return string.pack("<i4i4i4fffffff", nx, ny, nz, 0, 0, 0, 1, 1, 1, resolution)
end

suite("terrain blobs with a bad header, dimension, resolution or payload are rejected", function()
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
end)

suite("terrain read from voxels round-trips through the blob format", function()
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
end)

suite("voxel counts round partial voxels up", function()
	local function voxelCountOf(sizeX: number, sizeY: number, sizeZ: number, resolution: number): number
		local ok, count = pcall(Terrain.voxelCount, sizeX, sizeY, sizeZ, resolution)
		return if ok then count else -1
	end

	check("voxel count of a 16 stud cube at 4 studs is 64", voxelCountOf(16, 16, 16, 4) == 64)
	check("voxel count rounds a partial voxel up", voxelCountOf(5, 4, 4, 4) == 2)
	check("a 1024x256x1024 region at 4 studs is 4194304 voxels", voxelCountOf(1024, 256, 1024, 4) == 4194304)
end)

suite("upserts, deletes and primitives survive encode and decode", function()
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
end)

suite("malformed MessagePack is rejected and the golden scalars decode", function()
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
end)

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
	local signals = { Name = signal(), Source = signal() }
	local properties = { Name = name }
	local node: any = {
		ClassName = className,
		Source = "",
		Parent = nil,
		children = {},
		sourceChanged = signals.Source,
	}
	setmetatable(node, {
		__index = function(_, key)
			if key == "Name" then
				return properties.Name
			end
			return nil
		end,
		__newindex = function(target, key, value)
			if key == "Name" then
				properties.Name = value
				signals.Name:Fire()
			else
				rawset(target, key, value)
			end
		end,
	})
	node.GetPropertyChangedSignal = function(_, property)
		return signals[property] or signal()
	end
	node.GetChildren = function(self)
		return table.clone(self.children)
	end
	node.GetDescendants = function(self)
		local list = {}
		local function visit(current)
			for _, child in current.children do
				table.insert(list, child)
				visit(child)
			end
		end
		visit(self)
		return list
	end
	return node
end

local function studioWorld(syncEcho: boolean)
	local root: any = newNode("Folder", "root")
	root.DescendantAdded = signal()
	root.DescendantRemoving = signal()
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
	local function fireRemoving(current: any)
		root.DescendantRemoving:Fire(current)
		for _, child in current.children do
			fireRemoving(child)
		end
	end
	local function fireAdded(current: any)
		root.DescendantAdded:Fire(current)
		for _, child in current.children do
			fireAdded(child)
		end
	end
	local function isUnderRoot(node: any): boolean
		local current: any = node
		while current ~= nil do
			if current == root then
				return true
			end
			current = current.Parent
		end
		return false
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
			if isUnderRoot(node) then
				fireRemoving(node)
			end
			detach(node)
			node.Parent = parent
			table.insert(parent.children, node)
			if isUnderRoot(node) then
				fireAdded(node)
			end
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
			fireRemoving(node)
			detach(node)
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

suite("a Script named Foo does not hide a Folder named Foo as parent", function()
		local tree, root, applier = freshApplier()
		local shadow = place(tree, "Script", "Foo", root)
		local folder = place(tree, "Folder", "Foo", root)
		applier:apply({ path = "Foo/Bar.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
		check("a Script named Foo does not hide a Folder named Foo as parent", countNamed(folder, "Bar") == 1)
		check("the Script named Foo receives no child", #tree.children(shadow) == 0)
end)

suite("apply creates intermediate folder", function()
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
end)

suite("apply strips .client.luau suffix", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "ui/main.client.luau", class = "LocalScript", kind = "Create", content = "print('hi')" })
		local ui = tree.findChild(root, "ui")
		local main = if ui ~= nil then tree.findChild(ui, "main") else nil
		check("apply strips .client.luau suffix", main ~= nil and main.Name == "main")
		check("apply maps LocalScript class", main ~= nil and main.ClassName == "LocalScript")
end)

suite("delete of a file keeps a same-named folder and its children", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "Utils/Helper.luau", class = "ModuleScript", kind = "Create", content = "" })
		applier:apply({ path = "Utils.luau", class = "ModuleScript", kind = "Delete" })
		local utils = tree.findChild(root, "Utils")
		check(
			"delete of a file keeps a same-named folder and its children",
			utils ~= nil and utils.ClassName == "Folder" and tree.findChild(utils, "Helper") ~= nil
		)
end)

suite("delete never creates missing parent folders", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "missing/A.luau", class = "ModuleScript", kind = "Delete" })
		check("delete never creates missing parent folders", tree.findChild(root, "missing") == nil)
end)

suite("delete removes only the same-class sibling among same-named instances", function()
		local tree, root, applier = freshApplier()
		place(tree, "Folder", "x", root)
		place(tree, "ModuleScript", "x", root)
		applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Delete" })
		local remaining = tree.findChild(root, "x")
		check(
			"delete removes only the same-class sibling among same-named instances",
			countNamed(root, "x") == 1 and remaining ~= nil and remaining.ClassName == "Folder"
		)
end)

suite("Script x then batch [x.luau upsert, x.server.luau delete] leaves a ModuleScript x", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "x.server.luau", class = "Script", kind = "Create", content = "s" })
		applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Update", content = "m" })
		applier:apply({ path = "x.server.luau", class = "Script", kind = "Delete" })
		local survivor = tree.findChild(root, "x")
		check(
			"Script x then batch [x.luau upsert, x.server.luau delete] leaves a ModuleScript x",
			countNamed(root, "x") == 1 and survivor ~= nil and survivor.ClassName == "ModuleScript"
		)
end)

suite("Upsert x.server.luau over a ModuleScript x becomes a Script", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "x.luau", class = "ModuleScript", kind = "Create", content = "m" })
		applier:apply({ path = "x.server.luau", class = "Script", kind = "Update", content = "s" })
		local x = tree.findChild(root, "x")
		check(
			"Upsert x.server.luau over a ModuleScript x becomes a Script",
			countNamed(root, "x") == 1 and x ~= nil and x.ClassName == "Script" and x.Source == "s"
		)
end)

suite("a class change moves the children of the replaced script", function()
		local tree, root, applier = freshApplier()
		local oldModule = place(tree, "ModuleScript", "x", root)
		place(tree, "ModuleScript", "Inner", oldModule)
		applier:apply({ path = "x.server.luau", class = "Script", kind = "Update", content = "s" })
		local x = tree.findChild(root, "x")
		check(
			"a class change moves the children of the replaced script",
			countNamed(root, "x") == 1 and x ~= nil and x.ClassName == "Script" and tree.findChild(x, "Inner") ~= nil
		)
end)

suite("a same-named Folder does not block a ModuleScript", function()
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
end)

suite("an init.luau patch makes its directory a ModuleScript of the directory's name", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "Folder/init.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
		local node = tree.findChild(root, "Folder")
		check(
			"an init.luau patch makes its directory a ModuleScript of the directory's name",
			countNamed(root, "Folder") == 1 and node ~= nil and node.ClassName == "ModuleScript" and node.Source == "return 1"
		)
		check("an init.luau patch creates no child named init", node ~= nil and tree.findChild(node, "init") == nil)
end)

suite("an init.server.luau patch makes its directory a Script", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "Boot/init.server.luau", class = "Script", kind = "Create", content = "print(1)" })
		local node = tree.findChild(root, "Boot")
		check(
			"an init.server.luau patch makes its directory a Script",
			node ~= nil and node.ClassName == "Script" and node.Source == "print(1)"
		)
end)

suite("a child of an init directory lands under the script that stands for it", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "Folder/init.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
		applier:apply({ path = "Folder/Child.luau", class = "ModuleScript", kind = "Create", content = "return 2" })
		local node = tree.findChild(root, "Folder")
		check(
			"a child of an init directory lands under the script that stands for it",
			countNamed(root, "Folder") == 1 and node ~= nil and tree.findChild(node, "Child") ~= nil
		)
end)

suite("an init patch over a Folder holding children keeps them under the script", function()
		local tree, root, applier = freshApplier()
		local folder = place(tree, "Folder", "Dir", root)
		place(tree, "ModuleScript", "Kid", folder)
		applier:apply({ path = "Dir/init.luau", class = "ModuleScript", kind = "Create", content = "return 3" })
		local node = tree.findChild(root, "Dir")
		check(
			"an init patch over a Folder holding children keeps them under the script",
			countNamed(root, "Dir") == 1 and node ~= nil and node.ClassName == "ModuleScript" and tree.findChild(node, "Kid") ~= nil
		)
end)

suite("deleting init.luau turns its directory back into a Folder keeping its children", function()
		local tree, root, applier = freshApplier()
		applier:apply({ path = "Folder/init.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
		applier:apply({ path = "Folder/Child.luau", class = "ModuleScript", kind = "Create", content = "return 2" })
		applier:apply({ path = "Folder/init.luau", class = "ModuleScript", kind = "Delete" })
		local node = tree.findChild(root, "Folder")
		check(
			"deleting init.luau turns its directory back into a Folder keeping its children",
			countNamed(root, "Folder") == 1 and node ~= nil and node.ClassName == "Folder" and tree.findChild(node, "Child") ~= nil
		)
end)

suite("apply records the expected echo before the write", function()
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
end)

suite("a failed source write is rolled back out of the echo record", function()
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
end)

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

local function memoryTerrain(initial: number, region: any?)
	local state = { value = initial }
	local terrain = {
		state = state,
		MaxExtents = {
			ExpandToGrid = function(_, _resolution)
				return region or extentsRegion
			end,
		},
		ReadVoxels = function(_, _readRegion, _resolution)
			return { { { { Value = state.value } } } }, { { { 0.5 } } }
		end,
		WriteVoxels = function(_, _writeRegion, _resolution, materials, _occupancies)
			state.value = materials[1][1][1]
		end,
	}
	return terrain
end

local function blobPatchOf(content: string)
	return { kind = "Upsert", direction = "ToStudio", path = "Terrain.terrain", class = "Terrain", content = content }
end

local function deps(terrain: any, tree: any?, history: any?)
	return {
		terrain = terrain,
		tree = tree,
		apply = Apply,
		protocol = Protocol,
		terrainBlob = Terrain,
		delay = function(_, _fire) end,
		history = history,
	}
end

local function fakeConnection(client: any, terrain: any): any
	local tree, root = studioWorld(true)
	local connection = Connection.new(client, root, noop, deps(terrain, tree))
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

suite("terrain push expands MaxExtents to the 4-stud grid", function()
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
end)

suite("terrain over the voxel cap is not read", function()
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
end)

local function blobBackoffAfter(value: number): number?
	local ok, result = pcall(Connection.nextBlobBackoff, value)
	return if ok then result else nil
end

suite("blob backoff doubles from 2 seconds and is capped at 60 seconds", function()
	check("blob backoff doubles from 2 seconds", blobBackoffAfter(2) == 4)
	check("blob backoff is capped at 60 seconds", blobBackoffAfter(32) == 60 and blobBackoffAfter(60) == 60)
end)

suite("a failed blob keeps the blob cursor where it was", function()
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
end)

suite("a rejected blob is reported through onState", function()
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
end)

suite("an empty blob batch advances the blob cursor", function()
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
end)

suite("stop during a blob poll writes no voxels", function()
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
end)

suite("stop during the handshake leaves the state Disconnected", function()
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
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		infoConnection.running = true
		infoConnection.session = 1
		infoConnection:_run(1)
		check("stop during the handshake leaves the state Disconnected", lastStateOf(log) == "Disconnected", lastStateOf(log))
		check("stop during the handshake posts no terrain", blobPosts == 0)
		check("stop during the handshake polls no patches", patchPolls == 0)
end)

suite("stop during a patch poll applies nothing", function()
		local tree, root = studioWorld(true)
		local acks = 0
		local connection: any = nil
		connection = Connection.new(
			{
				patches = function(_, _cursor)
					connection:stop()
					return {
						cursor = 1,
						patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Late.luau", class = "ModuleScript", content = "L" } },
					}
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths)
					acks += 1
				end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection:_serve(1)
		check("stop during a patch poll applies nothing", tree.findChild(root, "Late") == nil)
		check("stop during a patch poll acks nothing", acks == 0)
end)

suite("a reshape while the daemon's patch is being applied queues nothing", function()
	local tree, root = studioWorld(true)
	local connection = Connection.new({
		postChanges = function(_, _changes) end,
	}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_attachStudio()
	local folder = newNode("ModuleScript", "Folder")
	folder.Parent = root
	table.insert(root.children, folder)
	local child = newNode("ModuleScript", "Child")
	child.Parent = folder
	table.insert(folder.children, child)

	connection.applying = true
	root.DescendantAdded:Fire(child)
	local queuedWhileApplying = next(connection.pendingPush)
	connection.applying = false
	connection.pendingPush = {}
	root.DescendantAdded:Fire(child)
	local queuedOutsideApply = connection.pendingPush["Folder.luau"] ~= nil
	connection:stop()

	check(
		"a reshape while the daemon's patch is being applied queues nothing",
		queuedWhileApplying == nil,
		"queued " .. tostring(queuedWhileApplying)
	)
	check(
		"the same reshape outside an apply queues the flat file's delete",
		queuedOutsideApply,
		"control did not reshape"
	)
end)

suite("a pending push posts nothing after stop", function()
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
end)

suite("a failing push is not retried after stop", function()
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
end)

suite("a batch queued before stop is dropped, not sent after a restart", function()
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
end)

suite("a push scheduled after a restart is sent", function()
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
end)

suite("a push timer that fires after stop sends nothing", function()
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
end)

suite("a push that fails during stop does not block the next push after restart", function()
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
end)

suite("a terrain read that finishes after stop posts nothing", function()
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
end)

suite("a stop during apply leaves the patch cursor where it was", function()
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
						patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "A.luau", class = "ModuleScript", content = "A" } },
					}
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			noop,
			harnessDeps
		)
		connection.running = true
		connection.session = 1
		connection.backoff = 8
		connection:_serve(1)
		check("a stop during apply leaves the patch cursor where it was", connection.cursor == 0, "cursor " .. connection.cursor)
		check("a stop during apply leaves the reconnect backoff where it was", connection.backoff == 8, "backoff " .. connection.backoff)
end)

suite("a stop during a blob poll with no patches keeps the blob cursor", function()
		local connection: any = nil
		connection = fakeConnection({
			blobs = function(_, _cursor)
				connection:stop()
				return { cursor = 4, patches = {} }
			end,
		}, extentsTerrainWith(extentsRegion))
		connection:_serveBlobs(1)
		check("a stop during a blob poll with no patches keeps the blob cursor", connection.blobCursor == 0, "cursor " .. connection.blobCursor)
end)

suite("a stop during a blob write applies no later blob", function()
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
end)

suite("a connect bumps the epoch before polling", function()
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
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection:_run(1)
		check("a connect bumps the epoch before polling", epochAtPoll == 2, "epoch " .. tostring(epochAtPoll))
end)

suite("a stop during the backoff wait leaves the backoff unchanged", function()
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
end)

local function cursorsAcrossReconnect(sessionIds: { string }): { number }
	local tree, root = studioWorld(true)
	local cursors: { number } = {}
	local connects = 0
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				connects += 1
				return { project_name = "demo", session_id = sessionIds[connects] }
			end,
			patches = function(_, cursor)
				table.insert(cursors, cursor)
				if #cursors == 1 then
					return { cursor = 3, patches = {} }
				elseif #cursors == 2 then
					error("daemon down")
				end
				connection:stop()
				return { cursor = 3, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection:_run(1)
	return cursors
end

suite("a reconnect to the same daemon session resumes from the cursor it held", function()
		local cursors = cursorsAcrossReconnect({ "s1", "s1" })
		check(
			"a reconnect to the same daemon session resumes from the cursor it held",
			cursors[3] == 3,
			"cursors " .. table.concat(cursors, ",")
		)
end)

suite("a reconnect to a restarted daemon session starts the cursor at 0", function()
		local cursors = cursorsAcrossReconnect({ "s1", "s2" })
		check(
			"a reconnect to a restarted daemon session starts the cursor at 0",
			cursors[3] == 0,
			"cursors " .. table.concat(cursors, ",")
		)
end)

suite("a restarted daemon session drops the acks it was still owed", function()
	local tree, root = studioWorld(true)
	local ackedSeqs: { number } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				return { project_name = "demo", session_id = "s2" }
			end,
			patches = function(_, _cursor)
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
			postInventory = function() return true end,
			postAck = function(_, seqs, _sessionId)
				for _, seq in seqs do
					table.insert(ackedSeqs, seq)
				end
			end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection.sessionId = "s1"
	connection.pendingAcks = { 7 }
	connection:_run(1)
	check(
		"a restarted daemon session drops the acks it was still owed",
		not table.find(ackedSeqs, 7),
		"acked " .. table.concat(ackedSeqs, ",")
	)
end)

suite("an ack is sent with the session the plugin is attached to", function()
	local tree, root = studioWorld(true)
	local ackedSession: any = nil
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				return { project_name = "demo", session_id = "s1" }
			end,
			patches = function(_, _cursor)
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
			postInventory = function() return true end,
			postAck = function(_, _seqs, sessionId)
				ackedSession = sessionId
			end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection.sessionId = "s1"
	connection.pendingAcks = { 7 }
	connection:_run(1)
	check("an ack is sent with the session the plugin is attached to", ackedSession == "s1", tostring(ackedSession))
end)

suite("a daemon restart during an ack reconnects instead of retrying the ack", function()
	local tree, root = studioWorld(true)
	local infos = 0
	local ackCalls = 0
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				infos += 1
				return { project_name = "demo", session_id = if infos == 1 then "s1" else "s2" }
			end,
			patches = function(_, _cursor)
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
			postInventory = function() return true end,
			postAck = function(_, _seqs, _sessionId)
				ackCalls += 1
				if ackCalls > 2 then
					connection:stop()
					return
				end
				error(Client.SESSION_CHANGED)
			end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection.ackBackoff = 0.01
	connection.sessionId = "s1"
	connection.pendingAcks = { 7 }
	connection:_run(1)
	check(
		"a daemon restart during an ack reconnects instead of retrying the ack",
		ackCalls == 1 and infos == 2,
		"acks " .. tostring(ackCalls) .. ", connects " .. tostring(infos)
	)
end)

suite("a restarted daemon session during a poll resets the reconnect backoff to its start", function()
	local tree, root = studioWorld(true)
	local infos = 0
	local polls = 0
	local backoffOnReconnect: any = nil
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				infos += 1
				if infos == 2 then
					backoffOnReconnect = connection.backoff
				end
				return { project_name = "demo", session_id = "s1" }
			end,
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					error(Client.SESSION_CHANGED)
				end
				connection:stop()
				return { cursor = 0, patches = {} }
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection.sessionId = "s1"
	connection:_run(1)
	check(
		"a restarted daemon session during a poll resets the reconnect backoff to its start",
		backoffOnReconnect == 2,
		"backoff " .. tostring(backoffOnReconnect)
	)
end)

suite("a failed text poll ends the blob poll of its epoch before the next handshake", function()
	local tree, root = studioWorld(true)
	local textFailed = false
	local blobCallsAfterFailure = 0
	local infos = 0
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				infos += 1
				if infos == 1 then
					return { project_name = "demo", session_id = "s1" }
				end
				if infos == 3 then
					connection:stop()
				end
				error("daemon down")
			end,
			patches = function(_, _cursor, _sessionId)
				textFailed = true
				error("daemon down")
			end,
			blobs = function(_, _cursor, _sessionId)
				if textFailed then
					blobCallsAfterFailure += 1
				end
				task.wait(0.01)
				return { cursor = 0, patches = {} }
			end,
			postBlobs = function(_, _blobs) end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection:_run(1)
	task.wait(0.05)
	check(
		"no blob poll starts once the text poll has failed",
		blobCallsAfterFailure == 0,
		"blob polls after the failure " .. tostring(blobCallsAfterFailure)
	)
end)

suite("a reconnect to the same daemon session resumes the blob cursor it held", function()
		local tree, root = studioWorld(true)
		local blobCursors: { number } = {}
		local textPolls = 0
		local connection: any = nil
		connection = Connection.new(
			{
				info = function(_, _placeId)
					return { project_name = "demo", session_id = "s1" }
				end,
				patches = function(_, _cursor)
					textPolls += 1
					if textPolls == 1 then
						return { cursor = 0, patches = {} }
					elseif textPolls == 2 then
						error("daemon down")
					end
					connection:stop()
					return { cursor = 0, patches = {} }
				end,
				blobs = function(_, cursor)
					table.insert(blobCursors, cursor)
					if #blobCursors == 1 then
						return { cursor = 5, patches = {} }
					end
					error("no blob patch expected")
				end,
				postBlobs = function(_, _blobs) end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.backoff = 0.01
		connection.blobBackoff = 0.01
		connection:_run(1)
		local resumed = true
		for index = 2, #blobCursors do
			if blobCursors[index] ~= 5 then
				resumed = false
			end
		end
		check(
			"a reconnect to the same daemon session resumes the blob cursor it held",
			resumed,
			"blob cursors " .. table.concat(blobCursors, ",")
		)
end)

suite("a rejected text patch holds the cursor at its seq so the next poll asks for it again", function()
		local _, onState = stateLog()
		local tree, root = studioWorld(true)
		lockedSource(tree, "Bad", 1000)
		local cursors: { number } = {}
		local connection = Connection.new(
			{
				patches = function(_, cursor)
					table.insert(cursors, cursor)
					if #cursors == 1 then
						return {
							cursor = 3,
							patches = {
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
								{ seq = 2, kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
							},
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function() end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		pcall(connection._serve, connection, 1)
		check("a rejected patch holds the cursor at its seq", cursors[2] == 1, "cursors " .. table.concat(cursors, ","))
end)

suite("a rejected text patch sets a Warning naming the path and reason", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		lockedSource(tree, "Bad", 1000)
		local acks: { { number } } = {}
		local polls = 0
		local connection = Connection.new(
			{
				patches = function(_, _cursor)
					polls += 1
					if polls == 1 then
						return {
							cursor = 1,
							patches = {
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
								{ seq = 2, kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
							},
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, paths)
					table.insert(acks, paths)
				end,
			},
			root,
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
		check("a rejected text patch is left out of the ack", #acks == 1 and #acks[1] == 1 and acks[1][1] == 2)
end)

suite("a rejected patch that later applies ends in Connected", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		lockedSource(tree, "Bad", 1)
		local bad = { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" }
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
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		pcall(connection._serve, connection, 1)
		check("a rejected patch that later applies ends in Connected", lastStateOf(log) == "Connected", lastStateOf(log))
end)

suite("a conflict plus a success in one batch ends in Conflict", function()
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
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
								{ seq = 2, kind = "Conflict", direction = "ToStudio", path = "C.luau", class = "ModuleScript" },
								{ seq = 3, kind = "Upsert", direction = "ToStudio", path = "Bad.luau", class = "ModuleScript", content = "B" },
							},
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
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
end)

suite("a later success on the conflicted path ends the Conflict", function()
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
							patches = { { seq = 1, kind = "Conflict", direction = "ToStudio", path = "C.luau", class = "ModuleScript" } },
						}
					end
					if polls == 2 then
						return {
							cursor = 2,
							patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "C.luau", class = "ModuleScript", content = "C" } },
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
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
end)

suite("a Conflict is reported while a permanent Warning is set", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
		connection:_setWarning("terrain", "terrain too large for live push, use naht build")
		connection:_setConflict("C.luau")
		check(
			"a Conflict is reported while a permanent Warning is set",
			lastStateOf(log) == "Conflict" and log[#log].detail == "C.luau",
			lastStateOf(log)
		)
end)

suite("a blob that later applies ends the Warning in Connected", function()
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
end)

suite("the newest terrain snapshot arrives last across an epoch change", function()
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
			postInventory = function() return true end,
			postAck = function() end,
		}
		local terrain = memoryTerrain(1)
		local connection = Connection.new(client, root, noop, deps(terrain, tree))
		connection.running = true
		connection.epoch = 1
		task.spawn(function()
			connection:_pushTerrain(1)
		end)
		task.wait(0.01)
		terrain.state.value = 2
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
end)

suite("a batch that keeps failing to decode is reported as still retrying", function()
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
end)

suite("a plain error that only mentions an undecodable batch is not counted as a decode failure", function()
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
end)

suite("a later successful poll clears the stalled blob warning", function()
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
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.blobBackoff = 0.01
		connection:_serveBlobs(1)
		check("a later successful poll clears the stalled blob warning", lastStateOf(log) == "Connected", lastStateOf(log))
end)

suite("a 503 on changes keeps the batch and retries it", function()
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
end)

suite("a 503 on ack keeps the applied paths and retries them", function()
		local log, onState = stateLog()
		local acks: { { number } } = {}
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
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
							},
						}
					end
					if patchCalls == 2 then
						return { cursor = 1, patches = {} }
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, paths)
					ackAttempts += 1
					if ackAttempts == 1 then
						error("naht: /ack returned 503")
					end
					table.insert(acks, paths)
				end,
			},
			root,
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
			ackAttempts == 2 and #acks == 1 and #acks[1] == 1 and acks[1][1] == 1,
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
end)

suite("a connect with a frozen path reports it as a Conflict and never Connected", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local connection: any = nil
		connection = Connection.new(
			{
				info = function(_, _placeId)
					return { protocol_version = 4, project_name = "demo", conflicted = { "Frozen.luau" } }
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
end)

suite("a blob Conflict is reported", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
		connection.running = true
		connection.epoch = 1
		connection:_applyBlobBatch(1, { cursor = 1, patches = { { kind = "Conflict", direction = "ToStudio", path = "Terrain.terrain", class = "Terrain" } } })
		check("a blob Conflict is reported", lastStateOf(log) == "Conflict", lastStateOf(log))
		connection:_applyBlobBatch(1, { cursor = 2, patches = { blobPatchOf(terrainHeader(1, 1, 1, 4) .. string.pack("<I2f", 1, 0.5)) } })
		check("a later blob success on the same path ends the blob Conflict", lastStateOf(log) == "Connected", lastStateOf(log))
end)

suite("a terrain push under the cap clears the too-large Warning", function()
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
end)

suite("a stop between patch poll and apply writes nothing", function()
		local tree, root = studioWorld(true)
		local acks = 0
		local connection: any = nil
		connection = Connection.new(
			{
				patches = function(_, _cursor)
					connection:stop()
					return {
						cursor = 3,
						patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "A.luau", class = "ModuleScript", content = "A" } },
					}
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths)
					acks += 1
				end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection:_serve(1)
		check("a stop between patch poll and apply writes nothing", countNamed(root, "A") == 0, "A count " .. countNamed(root, "A"))
		check("a stop between patch poll and apply acks nothing", acks == 0, "acks " .. tostring(acks))
end)

suite("a stop between blob poll and apply writes no blob", function()
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
end)

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
	local connection = Connection.new(client, root, onState, deps(terrain, tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	return connection
end

suite("a failed first terrain post is retried", function()
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
end)

suite("a stop during the terrain retry wait posts no further attempt", function()
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
end)

suite("a terrain post that fails during stop sets no Warning", function()
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
end)

suite("a terrain post that keeps failing sets a Warning naming the reason", function()
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
end)

suite("the failed terrain post raises one Warning", function()
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
end)

suite("a blob poll failure while running is retried until superseded", function()
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
end)

suite("a blob poll failure after stop sets no Warning", function()
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
end)

suite("a blob poll failure after a reconnect sets no Warning", function()
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
end)

suite("a transient blob poll failure is retried", function()
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
end)

suite("a stop during a blob poll sets no Warning", function()
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
end)

suite("a stop during a blob write leaves Disconnected as the last state, not a refreshed state", function()
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
end)

suite("the largest region the cap admits is exactly MAX_VOXELS", function()
		local voxels = Terrain.voxelCount(1024, 1024, 64, 4)
		local blob = string.pack("<i4i4i4fffffff", 256, 256, 16, 0, 0, 0, 1, 1, 1, 4) .. string.rep("\0", 6 * voxels)
		local body = Protocol.encodeBlobs({ { path = "Terrain.terrain", class = "Terrain", content = blob } })
		local daemonBodyLimit = 64 * 1024 * 1024
		check("the largest region the cap admits is exactly MAX_VOXELS", voxels == Terrain.MAX_VOXELS, "voxels " .. tostring(voxels))
		check("the worst-case blob is a 40-byte header plus 6 bytes per voxel", #blob == 40 + 6 * Terrain.MAX_VOXELS, "bytes " .. tostring(#blob))
		check("the MessagePack envelope around the worst-case blob is under 256 bytes", #body - #blob < 256, "envelope " .. tostring(#body - #blob))
		check("the worst-case terrain body is under the 64 MiB daemon body limit", #body < daemonBodyLimit, "body " .. tostring(#body))
end)

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
						patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Z.luau", class = "ModuleScript", content = "A" } },
					}
				end
				error("stop")
			end,
			postInventory = function() return true end,
			postAck = function(_, _paths) end,
		},
		root,
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

suite("a script with children is named by its init file, the way the daemon writes it", function()
		local initNames = { ModuleScript = "init.luau", Script = "init.server.luau", LocalScript = "init.client.luau" }
		for className, initName in initNames do
			local tree, root = studioWorld(true)
			local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
			local script = place(tree, className, "Folder", root)
			place(tree, "ModuleScript", "Child", script)
			local path = connection:_pathOf(script)
			check(
				"a " .. className .. " with children is named Folder/" .. initName,
				path == "Folder/" .. initName,
				"path " .. tostring(path)
			)
			local freshTree, freshRoot, applier = freshApplier()
			applier:apply({ path = "Folder/" .. initName, class = className, kind = "Create", content = "return 1" })
			local made = freshTree.findChild(freshRoot, "Folder")
			check(
				"the daemon path for a " .. className .. " with children rebuilds a " .. className,
				made ~= nil and made.ClassName == className,
				"made " .. tostring(made and made.ClassName)
			)
		end
end)

local TOO_LARGE = "terrain too large for live push, use naht build"

local function recordingClient(refuse: ((any) -> ())?)
	local posts: { { any } } = {}
	local attempts: { { any } } = {}
	local client = {
		postChanges = function(_, changes)
			table.insert(attempts, changes)
			if refuse ~= nil then
				refuse(changes)
			end
			table.insert(posts, changes)
		end,
		postInventory = function() return true end,
		postAck = noop,
	}
	return client, posts, attempts
end

local function hasChange(batch: { any }, kind: string, path: string): boolean
	for _, change in batch do
		if change.kind == kind and change.path == path then
			return true
		end
	end
	return false
end

local function anyChange(posts: { { any } }, kind: string, path: string): boolean
	for _, batch in posts do
		if hasChange(batch, kind, path) then
			return true
		end
	end
	return false
end

local function renameWorld(client: any)
	local tree, root = studioWorld(true)
	local connection = Connection.new(client, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_attachStudio()
	return connection, tree, root
end

suite("renaming a tracked script sends its old path's Delete and its new path's Upsert in one batch", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "Old", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = "New"
		waitFor(function() return #attempts >= 2 end)
		local batch = attempts[2] or {}
		check(
			"renaming a script deletes its old file and upserts its new file in one batch",
			#attempts == 2 and hasChange(batch, "Delete", "Old.luau") and hasChange(batch, "Upsert", "New.luau"),
			tostring(#attempts) .. " batches"
		)
		connection:stop()
end)

suite("renaming a folder sends each child script's old-path Delete and new-path Upsert in one batch", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local pack = place(tree, "Folder", "Pack", root)
		place(tree, "ModuleScript", "A", pack)
		place(tree, "ModuleScript", "B", pack)
		waitFor(function() return #attempts >= 1 end)
		pack.Name = "Box"
		waitFor(function() return #attempts >= 2 end)
		local batch = attempts[2] or {}
		check(
			"renaming a folder moves every child script in one batch",
			hasChange(batch, "Delete", "Pack/A.luau")
				and hasChange(batch, "Upsert", "Box/A.luau")
				and hasChange(batch, "Delete", "Pack/B.luau")
				and hasChange(batch, "Upsert", "Box/B.luau"),
			tostring(#attempts) .. " batches"
		)
		connection:stop()
end)

suite("a rename the daemon refuses by its new path leaves the old file in place", function()
		local client, posts, attempts = recordingClient(function(changes)
			if hasChange(changes, "Upsert", ".Hidden.luau") then
				error({ status = 400, message = "refused", refused = { ".Hidden.luau" } })
			end
		end)
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "Old", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = ".Hidden"
		waitFor(function() return #attempts >= 2 end)
		task.wait(0.6)
		check("a refused rename never sends the old file's Delete", not anyChange(posts, "Delete", "Old.luau"))
		check(
			"a refused rename restores the old path as the placed path",
			connection.placed[script] == "Old.luau",
			tostring(connection.placed[script])
		)
		connection:stop()
end)

suite("a rename refused without a named path sends its Delete only alongside its Upsert", function()
		local client, posts, attempts = recordingClient(function(changes)
			if hasChange(changes, "Upsert", ".Hidden.luau") then
				error({ status = 400, message = "refused" })
			end
		end)
		local connection, tree, root = renameWorld(client)
		place(tree, "ModuleScript", "Old", root)
		local script = findByClass(root, "Old", "ModuleScript")
		waitFor(function() return #attempts >= 1 end)
		script.Name = ".Hidden"
		waitFor(function() return #attempts >= 2 end)
		task.wait(0.6)
		check(
			"a 400 that names no path never sends the old file's Delete alone",
			not anyChange(posts, "Delete", "Old.luau"),
			tostring(#attempts) .. " attempts"
		)
		connection:stop()
end)

suite("moving a script into a folder sends its old Delete and its new Upsert in one batch", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		place(tree, "Folder", "Dir", root)
		local dir = findByClass(root, "Dir", "Folder")
		waitFor(function() return #attempts >= 1 end)
		tree.setParent(script, dir)
		waitFor(function() return #attempts >= 2 end)
		local batch = attempts[2] or {}
		check(
			"moving a script deletes its old file and upserts its new file in one batch",
			#attempts == 2 and hasChange(batch, "Delete", "A.luau") and hasChange(batch, "Upsert", "Dir/A.luau"),
			tostring(#attempts) .. " batches"
		)
		connection:stop()
end)

suite("a reparent the daemon refuses leaves the old file in place", function()
		local client, posts, attempts = recordingClient(function(changes)
			if hasChange(changes, "Upsert", ".Hidden/A.luau") then
				error({ status = 400, message = "refused", refused = { ".Hidden/A.luau" } })
			end
		end)
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		place(tree, "Folder", ".Hidden", root)
		local hidden = findByClass(root, ".Hidden", "Folder")
		waitFor(function() return #attempts >= 1 end)
		tree.setParent(script, hidden)
		waitFor(function() return #attempts >= 2 end)
		task.wait(0.6)
		check("a refused reparent never sends the old file's Delete", not anyChange(posts, "Delete", "A.luau"))
		check(
			"a refused reparent restores the old path as the placed path",
			connection.placed[script] == "A.luau",
			tostring(connection.placed[script])
		)
		connection:stop()
end)

suite("an edit made after a refused rename does not send the old file's Delete", function()
		local client, posts, attempts = recordingClient(function(changes)
			if hasChange(changes, "Upsert", ".Hidden.luau") then
				error({ status = 400, message = "refused", refused = { ".Hidden.luau" } })
			end
		end)
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "Old", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = ".Hidden"
		tree.setSource(script, "return 'edited'")
		waitFor(function() return #attempts >= 2 end)
		task.wait(1)
		check(
			"an edit after a refused rename never sends the old file's Delete",
			not anyChange(posts, "Delete", "Old.luau"),
			tostring(#attempts) .. " attempts"
		)
		check(
			"an edit after a refused rename restores the old path as the placed path",
			connection.placed[script] == "Old.luau",
			tostring(connection.placed[script])
		)
		connection:stop()
end)

suite("a Windows device name is warned once and yields no path", function()
		local names = { "CON", "con", "nul.Extra", "LPT1", "COM\u{b2}", "CLOCK$" }
		for _, name in names do
			local tree, root = studioWorld(true)
			local warnings: { string } = {}
			local settings = deps(extentsTerrainWith(extentsRegion), tree)
			settings.warn = function(message: string)
				table.insert(warnings, message)
			end
			local connection = Connection.new({}, root, noop, settings)
			local script = place(tree, "ModuleScript", name, root)
			local first = connection:_pathOf(script)
			local second = connection:_pathOf(script)
			check(
				"the device-named script " .. name .. " has no path",
				first == nil and second == nil,
				tostring(first)
			)
			check(
				"the device-named script " .. name .. " is warned once",
				#warnings == 1,
				tostring(#warnings) .. " warnings"
			)
		end
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		local console = place(tree, "ModuleScript", "CONSOLE", root)
		check(
			"a name that only starts with a device name keeps its path",
			connection:_pathOf(console) == "CONSOLE.luau",
			tostring(connection:_pathOf(console))
		)
end)

suite("a rename onto a path another script holds is refused, warned once, and overwrites nothing", function()
		local client, posts, attempts = recordingClient()
		local tree, root = studioWorld(true)
		local warnings: { string } = {}
		local settings = deps(extentsTerrainWith(extentsRegion), tree)
		settings.warn = function(message: string)
			table.insert(warnings, message)
		end
		local connection = Connection.new(client, root, noop, settings)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection:_attachStudio()
		local foo = place(tree, "ModuleScript", "Foo", root)
		tree.setSource(foo, "return 'foo'")
		local bar = place(tree, "ModuleScript", "Bar", root)
		tree.setSource(bar, "return 'bar'")
		task.wait(0.6)
		local before = #posts
		foo.Name = "Bar"
		tree.setSource(foo, "return 'edited'")
		task.wait(0.6)
		local later = table.move(posts, before + 1, #posts, 1, {})
		check(
			"a rename onto a held path never upserts over the other script's file",
			not anyChange(later, "Upsert", "Bar.luau") and not anyChange(later, "Delete", "Foo.luau"),
			tostring(#later) .. " later batches"
		)
		check(
			"a refused rename keeps the old path as the placed path",
			connection.placed[foo] == "Foo.luau",
			tostring(connection.placed[foo])
		)
		check(
			"a refused rename onto a held path warns exactly once",
			#warnings == 1,
			tostring(#warnings) .. " warnings"
		)
		connection:stop()
end)

suite("a second rename before the post keeps the first rename's Delete in the refused group", function()
		local client, posts, attempts = recordingClient(function(changes)
			if hasChange(changes, "Upsert", "C.luau") then
				error({ status = 400, message = "refused", refused = { "C.luau" } })
			end
		end)
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = "B"
		script.Name = "C"
		task.wait(1)
		check(
			"a chain of renames whose last name is refused never sends the first old file's Delete",
			not anyChange(posts, "Delete", "A.luau"),
			tostring(#posts) .. " posts"
		)
		check(
			"a chain of renames whose last name is refused restores the first old path as the placed path",
			connection.placed[script] == "A.luau",
			tostring(connection.placed[script])
		)
		connection:stop()
end)

suite("a rename made while the first post is in flight ends with the old file deleted and the new one upserted", function()
		local gate = true
		local client, posts, attempts = recordingClient(function(_changes)
			while gate do
				task.wait(0.05)
			end
		end)
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = "B"
		task.wait(0.5)
		script.Name = "C"
		gate = false
		task.wait(1.5)
		check(
			"a rename made during an in-flight post deletes the first old file and upserts the last name",
			anyChange(posts, "Delete", "A.luau") and anyChange(posts, "Upsert", "C.luau"),
			tostring(#posts) .. " posts"
		)
		connection:stop()
end)

suite("moving a script out of the root deletes its file", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		waitFor(function() return #attempts >= 1 end)
		local outside = tree.createInstance("Folder", "Outside")
		tree.setParent(script, outside)
		waitFor(function() return #attempts >= 2 end)
		local batch = attempts[2] or {}
		check(
			"moving a script out of the root deletes its old file",
			#attempts == 2 and hasChange(batch, "Delete", "A.luau"),
			tostring(#attempts) .. " batches"
		)
		connection:stop()
end)

suite("the daemon's answer to a rename queues nothing back", function()
		local tree, root = studioWorld(true)
		place(tree, "ModuleScript", "Old", root)
		local polls = 0
		local client = {
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 2,
						patches = {
							{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "New.luau", class = "ModuleScript", content = "" },
							{ seq = 2, kind = "Delete", direction = "ToStudio", path = "Old.luau", class = "ModuleScript" },
						},
					}
				end
				error("stop")
			end,
			postInventory = function() return true end,
			postAck = noop,
		}
		local connection = Connection.new(client, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		connection.running = true
		connection.session = 1
		connection:_attachStudio()
		pcall(connection._serve, connection, 1, "s-1")
		check(
			"the daemon's rename answer queues no push",
			next(connection.pendingPush) == nil,
			tostring(next(connection.pendingPush))
		)
		check("the daemon's rename answer creates the new name", tree.findChild(root, "New") ~= nil)
		connection:stop()
end)

suite("a second rename of a script deletes the path its first rename produced", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "A", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = "B"
		waitFor(function() return #attempts >= 2 end)
		script.Name = "C"
		waitFor(function() return #attempts >= 3 end)
		local batch = attempts[3] or {}
		check(
			"the second rename deletes the B file the first rename produced",
			hasChange(batch, "Delete", "B.luau") and hasChange(batch, "Upsert", "C.luau")
		)
		check("the second rename does not repeat the first rename's Delete", not hasChange(batch, "Delete", "A.luau"))
		connection:stop()
end)

suite("renaming a script to a name the plugin cannot write sends nothing", function()
		local client, posts, attempts = recordingClient()
		local connection, tree, root = renameWorld(client)
		local script = place(tree, "ModuleScript", "Old", root)
		waitFor(function() return #attempts >= 1 end)
		script.Name = "Bad/Name"
		task.wait(0.6)
		check("a rename to an unwritable name sends no batch", #attempts == 1, tostring(#attempts))
		check("a rename to an unwritable name does not delete the old file", not anyChange(posts, "Delete", "Old.luau"))
		connection:stop()
end)

local function liveTerrainWorld(terrain: any, client: any, history: any, onState: ((string, string?) -> ())?)
	local tree, root = studioWorld(true)
	local settings = deps(terrain, tree, history)
	settings.delay = task.delay
	local connection = Connection.new(client, root, onState or noop, settings)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.terrainSync = true
	connection:_attachStudio()
	return connection
end

local function blobRecorder()
	local blobs: { string } = {}
	local client = {
		postBlobs = function(_, batch)
			table.insert(blobs, batch[1].content)
		end,
		postInventory = function() return true end,
		postAck = noop,
		postChanges = noop,
	}
	return client, blobs
end

suite("a waypoint that changes the terrain pushes the blob during the session", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection:_pushTerrain(1)
		terrain.state.value = 2
		history.OnWaypointAdded:Fire("sculpt")
		waitFor(function() return #blobs >= 2 end)
		check(
			"a sculpted waypoint pushes the new terrain",
			#blobs == 2 and blobs[2] == Terrain.read(terrain, extentsRegion, 4),
			tostring(#blobs) .. " blobs"
		)
		connection:stop()
end)

suite("a burst of waypoints pushes the terrain once, after the last one", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection:_pushTerrain(1)
		for value = 2, 4 do
			terrain.state.value = value
			history.OnWaypointAdded:Fire("sculpt")
			task.wait(0.3)
		end
		check("no push lands while the burst is still within its debounce", #blobs == 1, tostring(#blobs))
		waitFor(function() return #blobs >= 2 end)
		task.wait(1.2)
		check(
			"the burst pushes once, with the terrain as it was after the last waypoint",
			#blobs == 2 and blobs[2] == Terrain.read(terrain, extentsRegion, 4),
			tostring(#blobs) .. " blobs"
		)
		connection:stop()
end)

suite("a waypoint that leaves the terrain unchanged pushes nothing", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection:_pushTerrain(1)
		history.OnWaypointAdded:Fire("select")
		task.wait(1.5)
		check("an unchanged terrain is not pushed again", #blobs == 1, tostring(#blobs))
		connection:stop()
end)

suite("a terrain blob applied from the daemon is not pushed back", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection:_pushTerrain(1)
		terrain.state.value = 5
		local incoming = Terrain.read(terrain, extentsRegion, 4)
		terrain.state.value = 1
		connection:_applyBlob(incoming)
		history.OnWaypointAdded:Fire("apply")
		task.wait(1.5)
		check("a blob written from the daemon is not echoed on the next waypoint", #blobs == 1, tostring(#blobs))
		connection:stop()
end)

suite("a waypoint pushes no terrain while terrain_sync is off", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection.terrainSync = false
		connection:_pushTerrain(1)
		terrain.state.value = 2
		history.OnWaypointAdded:Fire("sculpt")
		task.wait(1.5)
		check("a changed terrain is not pushed while the daemon disables terrain_sync", #blobs == 1, tostring(#blobs))
		connection:stop()
end)

suite("stopping the connection cancels a terrain push waiting on its debounce", function()
		local terrain = memoryTerrain(1)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local connection = liveTerrainWorld(terrain, client, history)
		connection:_pushTerrain(1)
		terrain.state.value = 2
		history.OnWaypointAdded:Fire("sculpt")
		connection:stop()
		task.wait(1.5)
		check("a stopped connection pushes no terrain from its pending debounce", #blobs == 1, tostring(#blobs))
end)

suite("a live terrain change over the size cap warns instead of pushing", function()
		local huge = { CFrame = extentsRegion.CFrame, Size = { X = 2048, Y = 2048, Z = 2048 } }
		local terrain = memoryTerrain(1, huge)
		local history = { OnWaypointAdded = signal() }
		local client, blobs = blobRecorder()
		local log, onState = stateLog()
		local connection = liveTerrainWorld(terrain, client, history, onState)
		terrain.state.value = 2
		history.OnWaypointAdded:Fire("sculpt")
		waitFor(function() return #warningsOf(log) > 0 end)
		check(
			"an oversized live push is refused with the build hint",
			#blobs == 0 and warningsOf(log)[1] == TOO_LARGE,
			tostring(warningsOf(log)[1])
		)
		connection:stop()
end)

local function connectWithoutTerrainSync(terrain: any, onState: (string, string?) -> ())
	local tree, root = studioWorld(true)
	local blobPosts = 0
	local blobPolls = 0
	local client = {
		info = function(_)
			return { project_name = "demo", session_id = "s-1", conflicted = {}, terrain_sync = false }
		end,
		patches = function(_, _cursor)
			error("stop")
		end,
		blobs = function(_, _cursor)
			blobPolls += 1
			error("stop")
		end,
		postBlobs = function(_, _batch)
			blobPosts += 1
		end,
		postChanges = noop,
		postInventory = function() return true end,
		postAck = noop,
	}
	local connection = Connection.new(client, root, onState, deps(terrain, tree))
	connection.running = true
	connection.session = 1
	task.spawn(function()
		connection:_run(1)
	end)
	task.wait(0.5)
	connection:stop()
	return blobPosts, blobPolls
end

suite("a connect with terrain_sync off posts no terrain snapshot and polls no blobs", function()
		local blobPosts, blobPolls = connectWithoutTerrainSync(memoryTerrain(1), noop)
		check(
			"a connect with terrain_sync off posts no terrain blob",
			blobPosts == 0,
			tostring(blobPosts) .. " posts"
		)
		check(
			"a connect with terrain_sync off polls no blobs",
			blobPolls == 0,
			tostring(blobPolls) .. " polls"
		)
end)

suite("a connect with terrain_sync off sets no terrain warning, even over the size cap", function()
		local huge = { CFrame = extentsRegion.CFrame, Size = { X = 2048, Y = 2048, Z = 2048 } }
		local log, onState = stateLog()
		connectWithoutTerrainSync(memoryTerrain(1, huge), onState)
		check(
			"a connect with terrain_sync off sets no terrain warning",
			#warningsOf(log) == 0,
			table.concat(warningsOf(log), " | ")
		)
end)

suite("the connection takes live terrain sync from the daemon's handshake", function()
		local terrain = memoryTerrain(1)
		local tree, root = studioWorld(true)
		local client = {
			info = function(_)
				return { project_name = "demo", session_id = "s-1", conflicted = {}, terrain_sync = true }
			end,
			patches = function(_, _cursor)
				error("stop")
			end,
			blobs = function(_, _cursor)
				error("stop")
			end,
			postBlobs = noop,
			postChanges = noop,
			postInventory = function() return true end,
			postAck = noop,
		}
		local connection = Connection.new(client, root, noop, deps(terrain, tree))
		connection.running = true
		connection.session = 1
		task.spawn(function()
			connection:_run(1)
		end)
		local synced = waitFor(function() return connection.terrainSync end)
		check("a handshake with terrain_sync enables live terrain sync", synced)
		connection:stop()
end)

local function connectWithReportedLimits(terrain: any, limits: any?, onState: (string, string?) -> ()): number
	local tree, root = studioWorld(true)
	local blobPosts = 0
	local client = {
		info = function(_)
			return { project_name = "demo", session_id = "s-1", conflicted = {}, terrain_sync = true, limits = limits }
		end,
		patches = function(_, _cursor)
			error("stop")
		end,
		blobs = function(_, _cursor)
			error("stop")
		end,
		postBlobs = function(_, _batch)
			blobPosts += 1
		end,
		postChanges = noop,
		postInventory = function() return true end,
		postAck = noop,
	}
	local connection = Connection.new(client, root, onState, deps(terrain, tree))
	connection.running = true
	connection.session = 1
	task.spawn(function()
		connection:_run(1)
	end)
	task.wait(0.5)
	connection:stop()
	return blobPosts
end

suite("a terrain over the cap the daemon reports is not posted and warns with the build hint", function()
		local log, onState = stateLog()
		local posts = connectWithReportedLimits(memoryTerrain(1), { terrain_voxels = 32 }, onState)
		check("a 64-voxel terrain is not posted under a reported cap of 32", posts == 0, tostring(posts) .. " posts")
		check("the refused terrain warns with the build hint", warningsOf(log)[1] == TOO_LARGE, table.concat(warningsOf(log), " | "))
end)

suite("a terrain within the cap the daemon reports is posted", function()
		local posts = connectWithReportedLimits(memoryTerrain(1), { terrain_voxels = 64 }, noop)
		check("a 64-voxel terrain is posted under a reported cap of 64", posts == 1, tostring(posts) .. " posts")
end)

suite("a daemon that reports no limits keeps the built-in terrain cap", function()
		local posts = connectWithReportedLimits(memoryTerrain(1), nil, noop)
		check("a 64-voxel terrain is posted when no cap is reported", posts == 1, tostring(posts) .. " posts")
end)

suite("a terrain blob for another path is skipped with a Warning and never written over the terrain", function()
		local terrain = memoryTerrain(1)
		local tree, root = studioWorld(true)
		local client = { postAck = noop, postBlobs = noop, postChanges = noop }
		local log, onState = stateLog()
		local connection = Connection.new(client, root, onState, deps(terrain, tree))
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		terrain.state.value = 9
		local terrainBlob = Terrain.read(terrain, extentsRegion, 4)
		terrain.state.value = 7
		local otherBlob = Terrain.read(terrain, extentsRegion, 4)
		terrain.state.value = 1
		connection:_applyBlobBatch(1, {
			cursor = 3,
			patches = {
				{ kind = "Upsert", direction = "ToStudio", path = "Terrain.terrain", class = "Terrain", content = terrainBlob },
				{ kind = "Upsert", direction = "ToStudio", path = "Other.terrain", class = "Terrain", content = otherBlob },
			},
		})
		check(
			"the Terrain.terrain blob is applied and the other path's blob is not written",
			terrain.state.value == 9,
			tostring(terrain.state.value)
		)
		local found = warningsOf(log)
		check(
			"the skipped path is named in a Warning",
			#found == 1 and string.find(found[1], "Other.terrain", 1, true) ~= nil,
			tostring(found[1])
		)
		check("the skipped blob still advances the blob cursor", connection.blobCursor == 3, tostring(connection.blobCursor))
end)

suite("a script that gains its first child moves from its file to init, and back", function()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		connection.running = true
		connection.session = 1
		connection:_attachStudio()
		local script = place(tree, "ModuleScript", "Folder", root)
		connection.pendingPush = {}
		local child = place(tree, "ModuleScript", "Child", script)
		local removedFile = connection.pendingPush["Folder.luau"]
		local newInit = connection.pendingPush["Folder/init.luau"]
		check(
			"gaining a child deletes the childless file",
			removedFile ~= nil and removedFile.kind == "Delete",
			"Folder.luau " .. tostring(removedFile and removedFile.kind)
		)
		check(
			"gaining a child upserts the init file",
			newInit ~= nil and newInit.kind == "Upsert",
			"Folder/init.luau " .. tostring(newInit and newInit.kind)
		)
		connection.pendingPush = {}
		tree.destroy(child)
		task.wait(0.01)
		local deletedInit = connection.pendingPush["Folder/init.luau"]
		local restoredFile = connection.pendingPush["Folder.luau"]
		check(
			"losing the last child deletes the init file",
			deletedInit ~= nil and deletedInit.kind == "Delete",
			"Folder/init.luau " .. tostring(deletedInit and deletedInit.kind)
		)
		check(
			"losing the last child upserts the childless file",
			restoredFile ~= nil and restoredFile.kind == "Upsert",
			"Folder.luau " .. tostring(restoredFile and restoredFile.kind)
		)
		connection:stop()
end)

suite("destroying a script with children leaves no file behind", function()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		connection.running = true
		connection.session = 1
		connection:_attachStudio()
		local script = place(tree, "Script", "Folder", root)
		place(tree, "ModuleScript", "Child", script)
		connection.pendingPush = {}
		tree.destroy(script)
		task.wait(0.01)
		local initDelete = connection.pendingPush["Folder/init.server.luau"]
		check(
			"destroying the script deletes its init file",
			initDelete ~= nil and initDelete.kind == "Delete",
			"Folder/init.server.luau " .. tostring(initDelete and initDelete.kind)
		)
		check(
			"destroying the script does not resurrect its childless file",
			connection.pendingPush["Folder.server.luau"] == nil,
			"Folder.server.luau " .. tostring(connection.pendingPush["Folder.server.luau"] and connection.pendingPush["Folder.server.luau"].kind)
		)
		connection:stop()
end)

suite("a rejected patch batch waits 1 s, then doubles per rejection up to the cap", function()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		local waits = {}
		for _ = 1, 8 do
			table.insert(waits, connection:_takeRejectedDelay())
		end
		check(
			"a rejected batch waits 1, 2, 4, 8, 16, 32, then the 60 s cap",
			table.concat(waits, " ") == "1 2 4 8 16 32 60 60",
			"waits " .. table.concat(waits, " ")
		)
end)

suite("a clean batch resets the rejected wait to its first step", function()
		local tree, root = studioWorld(true)
		local polls = 0
		local connection = Connection.new(
			{
				patches = function(_, _cursor)
					polls += 1
					if polls == 1 then
						return {
							cursor = 1,
							patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Z.luau", class = "ModuleScript", content = "A" } },
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.rejectedBackoff = 8
		connection:_attachStudio()
		pcall(connection._serve, connection, 1)
		check(
			"a clean batch resets the rejected wait to 1 s",
			connection.rejectedBackoff == 1,
			"rejected wait " .. tostring(connection.rejectedBackoff)
		)
		connection:stop()
end)

suite("sync echo: edit to B then revert to A pushes A", function()
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
end)

suite("deferred echo: edit to B then revert to A pushes A", function()
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
end)

suite("a deferred echo of the applied source is dropped", function()
		local connection, _node, flush = echoConnection(false)
		flush()
		check("a deferred echo of the applied source is dropped", connection.pendingPush["Z.luau"] == nil)
		connection:stop()
end)

suite("removing a folder with a script child posts the child's Delete alone", function()
		local posts: { string } = {}
		local tree, root = studioWorld(true)
		local folder = place(tree, "Folder", "Folder", root)
		place(tree, "ModuleScript", "A", folder)
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, changes)
					local described = {}
					for _, change in changes do
						table.insert(described, change.kind .. " " .. change.path)
					end
					table.insert(posts, table.concat(described, ","))
				end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection:_attachStudio()
		tree.destroy(folder)
		connection:_flushPush(1)
		check(
			"removing a folder with a script child posts the child's Delete alone",
			#posts == 1 and posts[1] == "Delete Folder/A.luau",
			table.concat(posts, " | ")
		)
		connection:stop()
end)

suite("a script deleted and recreated with its applied source is pushed", function()
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
end)

suite("a synchronous echo of the applied source is dropped", function()
		local connection = echoConnection(true)
		check("a synchronous echo of the applied source is dropped", connection.pendingPush["Z.luau"] == nil)
		connection:stop()
end)

suite("a same-named folder does not block a patch, so it is acked", function()
		local tree, root = studioWorld(true)
		local acked: { number } = {}
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
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Utils.luau", class = "ModuleScript", content = "x" },
								{ seq = 2, kind = "Upsert", direction = "ToStudio", path = "Other.luau", class = "ModuleScript", content = "y" },
							},
						}
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, paths)
					acked = paths
				end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		pcall(connection._serve, connection, 1)
		check(
			"a same-named folder does not block a patch, so it is acked",
			#acked == 2 and acked[1] == 1 and acked[2] == 2,
			"acked " .. table.concat(acked, ",")
		)
end)

suite("stop reports Disconnected", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local connection = Connection.new(
			{
				info = function(_, _placeId)
					error("offline")
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection:stop()
		check("stop reports Disconnected", lastStateOf(log) == "Disconnected")
end)

suite("stop clears the Warnings left by the last run", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
		connection.running = true
		connection.session = 1
		connection:_setWarning("blob", "terrain blob rejected: stale")
		connection.conflicts["C.luau"] = true
		connection:stop()
		check("stop clears the Warnings left by the last run", next(connection.warnings) == nil)
		check("stop clears the Conflicts left by the last run", next(connection.conflicts) == nil)
		connection:_refreshState()
		check("the first state after a restart is not a stale Warning", lastStateOf(log) == "Connected", lastStateOf(log))
end)

suite("start attaches one set of Studio listeners", function()
		local tree, root = studioWorld(true)
		local lifecycle = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		lifecycle:start()
		check("start attaches one set of Studio listeners", root.DescendantAdded:count() == 1)
		lifecycle:stop()
		check("stop detaches the Studio listeners", root.DescendantAdded:count() == 0)
		lifecycle:start()
		lifecycle:start()
		check("restart does not stack Studio listeners", root.DescendantAdded:count() == 1)
		lifecycle:stop()
		check("second stop detaches the Studio listeners", root.DescendantAdded:count() == 0)
end)

suite("start after a stop reconnects", function()
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
			postInventory = function() return true end,
			postAck = function() end,
			postBlobs = function() end,
			postChanges = function() end,
		}
		local lifecycle = Connection.new(client, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		lifecycle:start()
		task.wait(0.05)
		lifecycle:stop()
		lifecycle:start()
		task.wait(0.05)
		lifecycle:stop()
		check("start after a stop reconnects", infos == 2, infos)
end)

suite("path of a plain script keeps its folders", function()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
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
end)

suite("block at line 2204", function()
		local tree, root = studioWorld(true)
		local connection = Connection.new({}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
		local parityNames = serde.decode("json", fs.readFile("crates/naht-core/tests/fixtures/project_path_names.json"))
		local dir = place(tree, "Folder", "Dir", root)
		for _, name in parityNames.refused do
			local leaf = place(tree, "ModuleScript", "Leaf", place(tree, "Folder", name, dir))
			check(string.format("path skips a folder named %q, as the daemon refuses it", name), connection:_pathOf(leaf) == nil)
		end
		for _, name in parityNames.accepted do
			local leaf = place(tree, "ModuleScript", "Leaf", place(tree, "Folder", name, dir))
			check(
				string.format("path keeps a folder named %q, as the daemon accepts it", name),
				connection:_pathOf(leaf) == "Dir/" .. name .. "/Leaf.luau"
			)
		end
		for _, name in parityNames.pluginOnlyRefused do
			local leaf = place(tree, "ModuleScript", "Leaf", place(tree, "Folder", name, dir))
			check(
				string.format("path skips a folder named %q, which the plugin refuses even though the daemon splits it", name),
				connection:_pathOf(leaf) == nil
			)
		end
end)

suite("a script-suffixed name is warned once", function()
		local tree, root = studioWorld(true)
		local warnings: { string } = {}
		local dependencies = deps(extentsTerrainWith(extentsRegion), tree)
		dependencies.warn = function(message: string)
			table.insert(warnings, message)
		end
		local connection = Connection.new({}, root, noop, dependencies)
		local suffixed = place(tree, "ModuleScript", "x.server", root)
		connection:_pathOf(suffixed)
		connection:_pathOf(suffixed)
		check("a script-suffixed name is warned once", #warnings == 1, "warnings: " .. tostring(#warnings))
end)

suite("a completed poll resets the reconnect backoff", function()
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
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.backoff = 8
		pcall(connection._serve, connection, 1)
		check("a completed poll resets the reconnect backoff", connection.backoff == 1)
end)

suite("an apply that changes nothing records no echo", function()
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
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		pcall(connection._serve, connection, 1)
		check("an apply that changes nothing records no echo", connection.lastApplied["Mod.luau"] == nil)
end)

suite("an apply that writes records its echo", function()
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
				postInventory = function() return true end,
				postAck = function(_, _paths) end,
			},
			root,
			noop,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		pcall(connection._serve, connection, 1)
		check("an apply that writes records its echo", connection.lastApplied["New.luau"] == "Y")
end)

local luau = require("@lune/luau")
for _, path in
	{
		"plugin/src/Protocol.luau",
		"plugin/src/Client.luau",
		"plugin/src/Support.luau",
		"plugin/src/TerrainSync.luau",
		"plugin/src/Placement.luau",
		"plugin/src/PushQueue.luau",
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

suite("an edit during an in-flight text push never starts a second post", function()
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
end)

suite("a text push waits for the in-flight post to return before the next one starts", function()
		local events = textPushProbe(false)
		check(
			"a text push waits for the in-flight post to return before the next one starts",
			table.concat(events, " | ") == "start v1 | end v1 | start v2 | end v2",
			table.concat(events, " | ")
		)
end)

suite("a path the daemon refuses is posted once and never retried", function()
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
					if #posts > 10 then
						return
					end
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
end)

suite("a 503 raised as a status error keeps the batch and retries it", function()
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
end)

suite("a 400 that names no path drops the batch after one attempt", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local attempts = 0
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, _changes)
					attempts += 1
					if attempts > 10 then
						return
					end
					error({ status = 400, message = "naht: /changes returned 400", refused = {} })
				end,
			},
			root,
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
end)

suite("a refused ack is not retried", function()
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
								{ seq = 1, kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" },
							},
						}
					end
					if patchCalls == 2 then
						return { cursor = 1, patches = {} }
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, _paths)
					ackAttempts += 1
					if ackAttempts > 10 then
						return
					end
					error({ status = 400, message = "naht: /ack returned 400", refused = {} })
				end,
			},
			root,
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
end)

suite("a transient ack failure retries one entry per path", function()
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
						local same = { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Good.luau", class = "ModuleScript", content = "G" }
						return { cursor = 1, patches = { same, same } }
					end
					if patchCalls == 2 or patchCalls == 3 then
						return { cursor = 1, patches = {} }
					end
					error("stop")
				end,
				postInventory = function() return true end,
				postAck = function(_, paths)
					ackAttempts += 1
					table.insert(lengths, #paths)
					if ackAttempts <= 2 then
						error({ status = 503, message = "naht: /ack returned 503", refused = {} })
					end
				end,
			},
			root,
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
end)

suite("an edit made during a refused post is sent once the refusal drops the only path", function()
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
end)

suite("after a stop and start the new connection waits for the old post before its own", function()
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
		local old = Connection.new(clientFor(true), root, onState, deps(extentsTerrainWith(extentsRegion), tree))
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
		local fresh = Connection.new(clientFor(false), root, onState, deps(extentsTerrainWith(extentsRegion), tree))
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
end)

suite("a 400 that names no path posts the batch once and then each path alone", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local posts: { string } = {}
		local accepted: { string } = {}
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, changes)
					local paths = {}
					for _, change in changes do
						table.insert(paths, change.path)
					end
					table.insert(posts, table.concat(paths, "+"))
					if #posts > 10 then
						return
					end
					if table.find(paths, "Bad.luau") ~= nil then
						error({ status = 400, message = "naht: /changes returned 400", refused = {} })
					end
					for _, path in paths do
						table.insert(accepted, path)
					end
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["Good.luau"] = Protocol.upsert("Good.luau", "ModuleScript", "G")
		connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(0.8)
		connection.running = false
		local badPosts = 0
		for _, post in posts do
			if string.find(post, "Bad.luau", 1, true) ~= nil then
				badPosts += 1
			end
		end
		check(
			"a 400 that names no path posts the batch once and then each path alone",
			badPosts == 2 and #posts == 3,
			"posts: " .. table.concat(posts, " | ")
		)
		check(
			"a path accepted alone after an unnamed refusal of its batch is sent",
			table.find(accepted, "Good.luau") ~= nil and #accepted == 1,
			"accepted: " .. table.concat(accepted, ",")
		)
		local warnedText = table.concat(warningsOf(log), "\n")
		check(
			"the path refused alone is named in a Warning",
			string.find(warnedText, "not synced, the daemon refused Bad.luau", 1, true) ~= nil,
			warnedText
		)
end)

suite("a /changes 500 that keeps recurring is dropped after a bounded number of attempts", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local attempts = 0
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, _changes)
					attempts += 1
					error({ status = 500, message = "naht: /changes returned 500", refused = {} })
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(1.5)
		connection.running = false
		check(
			"a /changes 500 that keeps recurring is dropped after a bounded number of attempts",
			attempts == 3 and next(connection.pendingPush) == nil,
			string.format("attempts %d", attempts)
		)
		check(
			"the batch dropped after repeated 500s is named in a Warning",
			string.find(table.concat(warningsOf(log), "\n"), "not synced, the daemon kept failing on Bad.luau", 1, true) ~= nil,
			table.concat(warningsOf(log), "\n")
		)
end)

suite("ten 503s in a row are each retried and the batch is still accepted after them", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local statuses = { 503, 503, 503, 503, 503, 503, 503, 503, 503, 503 }
		local attempts = 0
		local accepted: { string } = {}
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, changes)
					attempts += 1
					local status = statuses[attempts]
					if status ~= nil then
						error({ status = status, message = "naht: /changes returned " .. status, refused = {} })
					end
					table.insert(accepted, changes[1].path)
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.001
		connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(2)
		connection.running = false
		check(
			"ten 503s in a row are each retried and the batch is still accepted after them",
			attempts == 11 and #accepted == 1,
			string.format("attempts %d, accepted %d", attempts, #accepted)
		)
		check(
			"no batch is dropped while its failures are transient",
			string.find(table.concat(warningsOf(log), "\n"), "not synced", 1, true) == nil,
			table.concat(warningsOf(log), "\n")
		)
end)

suite("the refusal of a path is named in a Warning before the path is edited again", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local posts = 0
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, changes)
					posts += 1
					if posts == 1 then
						error({ status = 400, message = "naht: /changes returned 400", refused = { "Bad.luau" } })
					end
					assert(changes[1].path == "Bad.luau")
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(0.5)
		check(
			"the refusal of a path is named in a Warning before the path is edited again",
			connection.warnings["refused:Bad.luau"] ~= nil,
			string.format("posts %d", posts)
		)
		connection.pendingPush["Bad.luau"] = Protocol.upsert("Bad.luau", "ModuleScript", "B2")
		connection:_schedulePush()
		task.wait(0.5)
		connection.running = false
		check(
			"a later accepted edit to a refused path clears its refused Warning",
			posts == 2 and connection.warnings["refused:Bad.luau"] == nil,
			string.format("posts %d, warnings: %s", posts, table.concat(warningsOf(log), " | "))
		)
		check(
			"the widget returns to Connected once the refused path is accepted",
			lastStateOf(log) == "Connected",
			lastStateOf(log)
		)
end)

suite("a 500 that is followed by an accepted post does not count toward the bound of consecutive 500s", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local script = { 500, nil, 500, 500, nil }
		local calls = 0
		local accepted = {}
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, changes)
					calls += 1
					local outcome = script[calls]
					if outcome == 500 then
						error({ status = 500, message = "naht: /changes returned 500", refused = {} })
					end
					table.insert(accepted, changes[1].path)
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		task.wait(0.5)
		connection.pushBackoff = 0.01
		connection.pendingPush["B.luau"] = Protocol.upsert("B.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(1)
		connection.running = false
		check(
			"a 500 that is followed by an accepted post does not count toward the bound of consecutive 500s",
			table.concat(accepted, ",") == "A.luau,B.luau" and next(connection.pendingPush) == nil,
			string.format("calls %d, accepted: %s", calls, table.concat(accepted, ","))
		)
		check(
			"no path is reported as kept failing after a 500 that was followed by an accepted post",
			string.find(table.concat(warningsOf(log), "\n"), "kept failing", 1, true) == nil,
			table.concat(warningsOf(log), "\n")
		)
end)

local function hangUntilReleased(hold: { done: boolean })
	if not waitFor(function()
		return hold.done
	end) then
		check("a hung post is released by its test within the bound", false, "the bound expired")
	end
end

local function fireWatchdog(watchdogs: { () -> () }, index: number)
	if not waitFor(function()
		return watchdogs[index] ~= nil
	end) then
		check("the watchdog of post " .. index .. " is armed within the bound", false, "no watchdog was armed")
		return
	end
	watchdogs[index]()
end

local function abandoningConnection(onState: any, holds: { [number]: { done: boolean } })
	for _, hold in holds do
		onCleanup(function()
			hold.done = true
		end)
	end
	local tree, root = studioWorld(true)
	local dependencies: any = deps(extentsTerrainWith(extentsRegion), tree)
	dependencies.pushGate = { issued = 0, flight = nil }
	local watchdogs: { () -> () } = {}
	dependencies.delay = function(_, fire)
		table.insert(watchdogs, fire)
	end
	local sent: { { any } } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			postChanges = function(_, changes)
				table.insert(sent, changes)
				local hold = holds[#sent]
				if hold ~= nil then
					hangUntilReleased(hold)
				end
			end,
			info = function(_, _placeId)
				return { project_name = "demo" }
			end,
			patches = function(_, _cursor)
				error("no poll expected")
			end,
			blobs = function(_, _cursor)
				error("no blob poll expected")
			end,
			postBlobs = function(_, _blobs) end,
			postInventory = function() return true end,
			postAck = function(_, _paths) end,
		},
		root,
		onState,
		dependencies
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection.pushBackoff = 0.01
	return connection, sent, watchdogs
end

suite("a hung post with no later edit is abandoned by the watchdog and its batch is sent again", function()
		local hold = { done = false }
		local log, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { [1] = hold })
		local first = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection.pendingPush["A.luau"] = first
		connection:_schedulePush()
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		check(
			"a hung post with no later edit is abandoned by the watchdog and its batch is sent again",
			#sent == 2 and rawequal(sent[1][1], first) and rawequal(sent[2][1], first),
			string.format("posts %d", #sent)
		)
		check(
			"the abandoned post is named in a Warning",
			string.find(table.concat(warningsOf(log), "\n"), "push abandoned", 1, true) ~= nil,
			table.concat(warningsOf(log), " | ")
		)
end)

suite("a newer pending edit to a path wins over the batch restored from an abandoned post", function()
		local hold = { done = false }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { [1] = hold })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		task.wait(0.5)
		local newer = Protocol.upsert("A.luau", "ModuleScript", "A2")
		connection.pendingPush["A.luau"] = newer
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		check(
			"a newer pending edit to a path wins over the batch restored from an abandoned post",
			#sent == 2 and rawequal(sent[2][1], newer),
			string.format("posts %d", #sent)
		)
end)

suite("a late return from an abandoned post does not release the gate of the newer post", function()
		local firstHold = { done = false }
		local secondHold = { done = false }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { [1] = firstHold, [2] = secondHold })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		task.wait(0.5)
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		local newerToken = connection.pushGate.flight and connection.pushGate.flight.token
		firstHold.done = true
		task.wait(0.5)
		check(
			"a late return from an abandoned post does not release the gate of the newer post",
			#sent == 2 and connection.pushGate.flight ~= nil and connection.pushGate.flight.token == newerToken,
			string.format("posts %d", #sent)
		)
		connection.pendingPush["C.luau"] = Protocol.upsert("C.luau", "ModuleScript", "C")
		connection:_schedulePush()
		task.wait(0.5)
		check(
			"no third post is sent while the newer post is still in flight",
			#sent == 2,
			string.format("posts %d", #sent)
		)
		secondHold.done = true
		task.wait(0.5)
		check(
			"the push after the newer post returns is sent and the gate is released",
			#sent == 3 and connection.pushGate.flight == nil,
			string.format("posts %d", #sent)
		)
end)

suite("a third consecutive abandonment stops the automatic re-send", function()
		local hold = { done = false }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { hold, hold, hold })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		fireWatchdog(watchdogs, 2)
		task.wait(0.5)
		fireWatchdog(watchdogs, 3)
		task.wait(0.5)
		check(
			"a third consecutive abandonment stops the automatic re-send",
			#sent == 3 and watchdogs[4] == nil,
			string.format("posts %d", #sent)
		)
		check(
			"the batch of the third abandoned post is kept pending",
			connection.pendingPush["A.luau"] ~= nil,
			"nothing pending"
		)
		check(
			"the held batch raises a Warning that the daemon is not answering",
			string.find(connection.warnings.push or "", "not answering", 1, true) ~= nil,
			tostring(connection.warnings.push)
		)
		connection.pendingPush["B.luau"] = Protocol.upsert("B.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(0.5)
		check(
			"the next edit sends the held batch together with the new edit",
			#sent == 4 and #sent[4] == 2,
			string.format("posts %d", #sent)
		)
		check(
			"the accepted post clears the not-answering Warning",
			connection.warnings.push == nil,
			tostring(connection.warnings.push)
		)
end)

suite("a reconnect sends the batch held after three abandonments", function()
		local hold = { done = false }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { hold, hold, hold })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		fireWatchdog(watchdogs, 2)
		task.wait(0.5)
		fireWatchdog(watchdogs, 3)
		task.wait(0.5)
		connection.client.patches = function(_, _cursor)
			task.wait(0.5)
			connection:stop()
			return { cursor = 0, patches = {} }
		end
		connection:_run(1)
		check(
			"a reconnect sends the batch held after three abandonments",
			#sent == 4,
			string.format("posts %d", #sent)
		)
end)

suite("an accepted post resets the count, so three more consecutive abandonments are needed to stop the re-send", function()
		local hold = { done = false }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { [1] = hold, [3] = hold, [4] = hold, [5] = hold })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		connection.pendingPush["C.luau"] = Protocol.upsert("C.luau", "ModuleScript", "C")
		connection:_schedulePush()
		task.wait(0.5)
		fireWatchdog(watchdogs, 3)
		task.wait(0.5)
		fireWatchdog(watchdogs, 4)
		task.wait(0.5)
		fireWatchdog(watchdogs, 5)
		task.wait(0.5)
		check(
			"an accepted post resets the count, so three more consecutive abandonments are needed to stop the re-send",
			#sent == 5 and watchdogs[6] == nil,
			string.format("posts %d", #sent)
		)
		check(
			"the batch is kept pending after the third abandonment that follows the reset",
			connection.pendingPush["C.luau"] ~= nil,
			"nothing pending"
		)
end)

suite("a late success from an abandoned post resets the count while a newer post is still hung", function()
		local holds = { { done = false }, { done = false }, { done = false }, { done = false } }
		local _, onState = stateLog()
		local connection, sent, watchdogs = abandoningConnection(onState, { [1] = holds[1], [2] = holds[2], [3] = holds[3], [4] = holds[4] })
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		fireWatchdog(watchdogs, 1)
		task.wait(0.5)
		fireWatchdog(watchdogs, 2)
		task.wait(0.5)
		fireWatchdog(watchdogs, 3)
		task.wait(0.5)
		check(
			"three abandonments are counted before the late success",
			connection.pushAbandons == 3,
			string.format("count %d", connection.pushAbandons)
		)
		connection.pendingPush["B.luau"] = Protocol.upsert("B.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(0.5)
		local newerToken = connection.pushGate.flight and connection.pushGate.flight.token
		holds[1].done = true
		task.wait(0.5)
		check(
			"a late success from an abandoned post resets the count while a newer post is still hung",
			#sent == 4 and connection.pushAbandons == 0 and connection.pushGate.flight ~= nil and connection.pushGate.flight.token == newerToken,
			string.format("posts %d, count %d", #sent, connection.pushAbandons)
		)
end)

suite("a 500 alternating with a 503 is still dropped at the third 500", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local script = { 500, 503, 500, 503, 500 }
		local calls = 0
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, _changes)
					calls += 1
					local status = script[calls]
					error({ status = status, message = "naht: /changes returned " .. tostring(status), refused = {} })
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		task.wait(2)
		connection.running = false
		check(
			"a 500 alternating with a 503 is still dropped at the third 500",
			calls == 5 and next(connection.pendingPush) == nil,
			string.format("calls %d", calls)
		)
		check(
			"the batch dropped at the third alternating 500 is named in a Warning",
			string.find(table.concat(warningsOf(log), "\n"), "not synced, the daemon kept failing on A.luau", 1, true) ~= nil,
			table.concat(warningsOf(log), "\n")
		)
end)

suite("a push after a dropped batch waits the first retry delay, not the grown one", function()
		local log, onState = stateLog()
		local tree, root = studioWorld(true)
		local attempts = 0
		local delayOfNextRetry: number? = nil
		local connection: any = nil
		connection = Connection.new(
			{
				postChanges = function(_, _changes)
					attempts += 1
					if attempts <= 3 then
						error({ status = 500, message = "naht: /changes returned 500", refused = {} })
					end
					delayOfNextRetry = connection.pushBackoff
					error({ status = 503, message = "naht: /changes returned 503", refused = {} })
				end,
			},
			root,
			onState,
			deps(extentsTerrainWith(extentsRegion), tree)
		)
		connection.running = true
		connection.session = 1
		connection.epoch = 1
		connection.pushBackoff = 0.01
		connection.pendingPush["A.luau"] = Protocol.upsert("A.luau", "ModuleScript", "A")
		connection:_schedulePush()
		task.wait(0.6)
		check(
			"the third 500 drops the batch and resets the push backoff to the first retry delay",
			attempts == 3 and next(connection.pendingPush) == nil and connection.pushBackoff == 2,
			string.format("attempts %d, backoff %s", attempts, tostring(connection.pushBackoff))
		)
		connection.pendingPush["B.luau"] = Protocol.upsert("B.luau", "ModuleScript", "B")
		connection:_schedulePush()
		task.wait(0.6)
		connection.running = false
		check(
			"a push after a dropped batch waits the first retry delay, 2 seconds",
			delayOfNextRetry == 2,
			tostring(delayOfNextRetry)
		)
end)

local function placeReader(placeId: number): () -> number
	return function()
		return placeId
	end
end

local function fakeHttp(respond: (any) -> any)
	local fake = { calls = {} :: { any } }
	function fake.RequestAsync(_self, options)
		table.insert(fake.calls, options)
		return respond(options)
	end
	function fake.JSONDecode(_self, text)
		return serde.decode("json", text)
	end
	return fake
end

local function reply(status: number, body: string)
	return { Success = status >= 200 and status < 300, StatusCode = status, Body = body }
end

suite("an ack names its session and a 409 on /ack reports a changed session", function()
	local acks = fakeHttp(function(_options)
		return reply(409, "")
	end)
	local client = Client.new("http://127.0.0.1:34872", placeReader(123), acks)
	local ok, err = pcall(function()
		client:postAck({ 7 }, "s1")
	end)
	check("a 409 on /ack is reported as a session change", not ok and Client.isSessionChanged(tostring(err)), tostring(err))
	check(
		"an ack names its session in the query",
		#acks.calls == 1 and string.find(acks.calls[1].Url, "/ack?session=s1", 1, true) ~= nil,
		if #acks.calls == 1 then acks.calls[1].Url else "no call"
	)
end)

suite("the client loads the saved token by the project id from /info and sends it on gated requests", function()
	local saved: { [string]: string } = { ["p-1"] = "stored-token" }
	local store = {
		load = function(projectId: string): string?
			return saved[projectId]
		end,
		save = function(projectId: string, token: string)
			saved[projectId] = token
		end,
	}
	local http = fakeHttp(function(options)
		if string.find(options.Url, "/info", 1, true) ~= nil then
			return reply(200, fromHex(GOLDEN.server_info))
		end
		return reply(200, fromHex(GOLDEN.patch_batch))
	end)
	local client = Client.new("http://127.0.0.1:34872", placeReader(123), http, store)
	client:info()
	check("the client takes the saved token for the project id /info reports", client.token == "stored-token")
	client:patches(0, "s1")
	local sent = http.calls[#http.calls].Headers["x-naht-token"]
	check("a gated request carries the saved token", sent == "stored-token", tostring(sent))
	client:setToken("typed-token")
	check("setToken saves the token under the project id", saved["p-1"] == "typed-token")
end)

suite("a 401 from the daemon is a token refusal that says whether the token was missing or wrong", function()
	local missing = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(401, '{"error": "token required"}')
		end)
	)
	local okMissing, errMissing = pcall(function()
		missing:patches(0, "s1")
	end)
	check("a 401 without a token is a token refusal", not okMissing and Support.isTokenRefusal(errMissing))
	check("a missing token asks for the token", not okMissing and errMissing.message == Client.TOKEN_REQUIRED)
	local wrong = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(401, '{"error": "token rejected"}')
		end)
	)
	local okWrong, errWrong = pcall(function()
		wrong:patches(0, "s1")
	end)
	check("a 401 for a wrong token says the daemon rejected it", not okWrong and errWrong.message == Client.TOKEN_REJECTED)
	check("a 401 is never classed as a transient failure", not Support.isTransientFailure(errWrong))
end)

suite("a token refusal on a poll stops the connection at TokenRequired without polling again", function()
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local infos = 0
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				infos += 1
				if infos > 1 then
					connection.running = false
				end
				return { project_name = "demo", session_id = "s1" }
			end,
			patches = function(_, _cursor)
				error({ status = 401, message = Client.TOKEN_REQUIRED, tokenRefused = true })
			end,
		},
		root,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_run(1)
	local last = log[#log]
	check("a token refusal on a poll reports TokenRequired", lastStateOf(log) == "TokenRequired", lastStateOf(log))
	check("a token refusal shows the message to enter the token", last ~= nil and last.detail == Client.TOKEN_REQUIRED)
	check("a token refusal does not reconnect silently", infos == 1, "info calls " .. tostring(infos))
	check("a token refusal stops the connection", connection.running == false)
end)

suite("submitting a token restarts the connection with that token", function()
	local _log, onState = stateLog()
	local tree, root = studioWorld(true)
	local stored: { string } = {}
	local polls = 0
	local runningAtInfo = false
	local connection: any = nil
	local client = {
		setToken = function(_, token: string)
			table.insert(stored, token)
		end,
		info = function(_, _placeId)
			runningAtInfo = connection.running
			return { project_name = "demo", session_id = "s1" }
		end,
		patches = function(_, _cursor)
			polls += 1
			connection:stop()
			return { cursor = 0, patches = {} }
		end,
	}
	connection = Connection.new(client, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection:submitToken("typed-token")
	check("submitToken hands the token to the client", stored[1] == "typed-token")
	check("submitToken starts the connection", runningAtInfo, tostring(runningAtInfo))
	check("the restarted connection polls the daemon", polls == 1, "polls " .. tostring(polls))
end)

suite("a token refusal on a terrain push stops the push instead of retrying it", function()
	local posts = 0
	local connection: any = nil
	connection = fakeConnection({
		postBlobs = function(_, _changes)
			posts += 1
			if posts >= 2 then
				connection.running = false
			end
			error({ status = 401, message = Client.TOKEN_REJECTED, tokenRefused = true })
		end,
	}, extentsTerrainWith(extentsRegion))
	connection:_pushTerrain(1)
	check("a token refusal on a terrain push is not retried", posts == 1, "posts " .. tostring(posts))
	check("a token refusal on a terrain push stops the connection", connection.running == false)
end)

suite("a token refusal on a blob poll stops the blob loop instead of backing off", function()
	local polls = 0
	local connection: any = nil
	connection = fakeConnection({
		blobs = function(_, _cursor, _sessionId)
			polls += 1
			if polls >= 2 then
				connection.running = false
			end
			error({ status = 401, message = Client.TOKEN_REQUIRED, tokenRefused = true })
		end,
	}, extentsTerrainWith(extentsRegion))
	connection:_serveBlobs(1, "s1")
	check("a token refusal on a blob poll is not polled again", polls == 1, "polls " .. tostring(polls))
	check("a token refusal on a blob poll stops the connection", connection.running == false)
end)

suite("a token refusal on an ack keeps the ack queued and stops the connection", function()
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local connection = Connection.new(
		{
			postAck = function(_, _seqs, _sessionId)
				error({ status = 401, message = Client.TOKEN_REJECTED, tokenRefused = true })
			end,
		},
		root,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	local kept = connection:_ack(1, { 7 }, "s1")
	check("a token refusal on an ack keeps it queued", kept[1] == 7)
	check("a token refusal on an ack stops the connection", connection.running == false)
	check("a token refusal on an ack reports TokenRequired", lastStateOf(log) == "TokenRequired", lastStateOf(log))
end)

suite("the client sends the headers on every request and reports an outdated plugin", function()
	local function answerEverything(options)
		if string.find(options.Url, "/info", 1, true) then
			return reply(200, fromHex(GOLDEN.server_info))
		elseif string.find(options.Url, "/patches", 1, true) then
			return reply(200, fromHex(GOLDEN.patch_batch))
		elseif string.find(options.Url, "/blobs", 1, true) and options.Method == "GET" then
			return reply(200, fromHex(GOLDEN.blob_patch_batch))
		end
		return reply(200, "")
	end

	local everything = fakeHttp(answerEverything)
	local everyRequest = Client.new("http://127.0.0.1:34872", placeReader(123), everything)
	everyRequest:info()
	everyRequest:patches(0, "s1")
	everyRequest:blobs(0, "s1")
	everyRequest:postBlobs({})
	everyRequest:postAck({}, "s1")
	everyRequest:postChanges({})
	local headersOnEveryRequest = #everything.calls == 6
	for _, call in everything.calls do
		if call.Headers["x-naht-client"] ~= "plugin" or call.Headers["x-naht-place-id"] ~= "123" then
			headersOnEveryRequest = false
		end
	end
	check("every client request sends x-naht-client and x-naht-place-id", headersOnEveryRequest)

	local missingClient = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(403, "outdated-plugin: missing x-naht-client header")
		end)
	)
	local okMissing, errMissing = pcall(function()
		missingClient:info()
	end)
	check(
		"a client-header refusal is reported as an outdated plugin",
		not okMissing and Client.isOutdatedPluginError(tostring(errMissing))
	)

	local wrongPlace = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(403, "place id mismatch")
		end)
	)
	local okPlace, errPlace = pcall(function()
		wrongPlace:info()
	end)
	check(
		"a place refusal keeps the daemon's reason in the error",
		not okPlace and string.find(tostring(errPlace), "place id mismatch", 1, true) ~= nil
	)
	check("a place refusal is not an outdated plugin", not Client.isOutdatedPluginError(tostring(errPlace)))

	local wordingOnly = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(403, "missing x-naht-client header")
		end)
	)
	local okWording, errWording = pcall(function()
		wordingOnly:info()
	end)
	check(
		"a refusal without the outdated-plugin token is not an outdated plugin",
		not okWording and not Client.isOutdatedPluginError(tostring(errWording))
	)

	local accepting = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(200, "")
		end)
	)
	local okAccepted = pcall(function()
		accepting:postChanges({ Protocol.upsert("src/B.luau", "ModuleScript", "return 2") })
	end)
	check("an accepted batch answers an empty 200 and raises nothing", okAccepted)
end)

suite("the place id header is read on each request, so a publish needs no reconnect", function()
	local placeId = 0
	local http = fakeHttp(function(_options)
		return reply(200, "")
	end)
	local client = Client.new("http://127.0.0.1:34872", function()
		return placeId
	end, http)
	client:postAck({}, "s1")
	placeId = 456
	client:postAck({}, "s1")
	check(
		"a request before the publish names the unpublished place",
		#http.calls == 2 and http.calls[1].Headers["x-naht-place-id"] == "0"
	)
	check(
		"the next request after the publish names the published place",
		#http.calls == 2 and http.calls[2].Headers["x-naht-place-id"] == "456"
	)
end)

suite("a 400 refused_paths body is surfaced with its paths", function()
	local refusing = Client.new(
		"http://127.0.0.1:34872",
		placeReader(123),
		fakeHttp(function()
			return reply(400, '{"refused_paths":["src/Dir"]}')
		end)
	)
	local ok, err = pcall(function()
		refusing:postChanges({ Protocol.upsert("src/Dir", "Folder", "") })
	end)
	check("a refused directory fails the push", not ok)
	check(
		"the refused directory paths are carried in the error",
		not ok and type(err) == "table" and err.refused[1] == "src/Dir"
	)
end)

suite("an outdated plugin ends the connection: pending pushes dropped, listeners detached, no retry", function()
	local tree, root = studioWorld(true)
	local log, onState = stateLog()
	local infos = 0
	local client = {
		info = function(_, _placeId)
			infos += 1
			error(Client.OUTDATED_PLUGIN, 0)
		end,
	}
	local connection = Connection.new(client, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.pendingPush = { ["Queued.luau"] = Protocol.upsert("Queued.luau", "ModuleScript", "return 1") }
	connection:start()
	waitFor(function()
		return lastStateOf(log) == "Disconnected"
	end)
	task.wait(0.2)

	local last = log[#log]
	check(
		"an outdated daemon ends in Disconnected with the outdated detail",
		last ~= nil and last.state == "Disconnected" and last.detail ~= nil
			and string.find(last.detail, Client.OUTDATED_PLUGIN, 1, true) ~= nil
	)
	check("an outdated daemon is not retried", infos == 1)
	local reconnecting = false
	for _, entry in log do
		reconnecting = reconnecting or entry.state == "Reconnecting"
	end
	check("an outdated daemon never reports Reconnecting", not reconnecting)
	check("an outdated daemon drops the pending pushes", next(connection.pendingPush) == nil)
	check("an outdated daemon detaches the Studio listeners", root.DescendantAdded:count() == 0)
	check("an outdated daemon clears the running flag", not connection.running)
end)

suite("an incompatible protocol version is terminal and names the side to update", function()
	local tree, root = studioWorld(true)
	local log, onState = stateLog()
	local http = fakeHttp(function(_options)
		return reply(
			200,
			MessagePack.encode({
				protocol_version = 1,
				server_version = "0.1.0",
				project_name = "demo",
				session_id = "old",
				daemon_id = "d-1",
			})
		)
	end)
	local connection = Connection.new(Client.new("http://127.0.0.1:34872", placeReader(123), http), root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection:start()
	waitFor(function()
		return lastStateOf(log) == "Disconnected"
	end)
	task.wait(0.2)

	local last = log[#log]
	check("an incompatible daemon ends in Disconnected", last ~= nil and last.state == "Disconnected")
	check(
		"the incompatible detail says to update the naht daemon",
		last ~= nil and last.detail ~= nil and string.find(last.detail, "update the naht daemon", 1, true) ~= nil
	)
	check("an incompatible daemon is asked for once", #http.calls == 1)
	local reconnecting = false
	for _, entry in log do
		reconnecting = reconnecting or entry.state == "Reconnecting"
	end
	check("an incompatible daemon never reports Reconnecting", not reconnecting)
	connection:stop()
	check("an incompatible daemon detaches the Studio listeners", root.DescendantAdded:count() == 0)

	local newerDaemon = select(2, pcall(Protocol.assertCompatible, { protocol_version = 5 }))
	check(
		"a newer daemon tells the user to update the naht plugin",
		string.find(tostring(newerDaemon), "update the naht plugin", 1, true) ~= nil
	)
end)

suite("an ack that failed is resent after a reconnect in the same session", function()
	local tree, root = studioWorld(true)
	local infos = 0
	local polls = 0
	local ackBodies: { { number } } = {}
	local connection: any = nil
	connection = Connection.new(
		{
			info = function(_, _placeId)
				infos += 1
				return { project_name = "demo", session_id = "s1", conflicted = {} }
			end,
			patches = function(_, _cursor)
				polls += 1
				if polls == 1 then
					return {
						cursor = 2,
						patches = { { seq = 1, kind = "Upsert", direction = "ToStudio", path = "Pending.luau", class = "ModuleScript", content = "P" } },
					}
				end
				if polls >= 4 then
					connection:stop()
				end
				error({ status = 500, message = "naht: /patches returned 500" })
			end,
			blobs = function(_, _cursor)
				task.wait(0.01)
				return { cursor = 0, patches = {} }
			end,
			postBlobs = function(_, _blobs) end,
			postInventory = function() return true end,
			postAck = function(_, seqs)
				table.insert(ackBodies, seqs)
				if #ackBodies == 1 then
					error({ status = 500, message = "naht: /ack returned 500" })
				end
				connection:stop()
			end,
		},
		root,
		noop,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection.running = true
	connection.session = 1
	connection.backoff = 0.01
	connection.ackBackoff = 0.01
	connection:_run(1)

	check("the reconnect handshook again in the same session", infos >= 2, "handshakes " .. tostring(infos))
	check(
		"the first ack is refused, so seq 1 is still unacked",
		ackBodies[1] ~= nil and ackBodies[1][1] == 1,
		"acks " .. tostring(#ackBodies) .. " polls " .. tostring(polls)
	)
	check(
		"the reconnect resends seq 1 before polling again",
		ackBodies[2] ~= nil and ackBodies[2][1] == 1,
		"acks " .. tostring(#ackBodies) .. " polls " .. tostring(polls)
	)
end)

suite("a poll names the session it handshook with, and a 409 sends the plugin back to /info", function()
	local tree, root = studioWorld(true)
	local log, onState = stateLog()
	local infos = 0
	local polls: { string } = {}
	local connection: any = nil
	local http = fakeHttp(function(options)
		if string.find(options.Url, "/info", 1, true) ~= nil then
			infos += 1
			return reply(
				200,
				MessagePack.encode({
					protocol_version = 4,
					server_version = "0.1.0",
					project_name = "demo",
					session_id = if infos == 1 then "s1" else "s2",
					daemon_id = "d-1",
				})
			)
		end
		if string.find(options.Url, "/blobs", 1, true) ~= nil then
			task.wait(0.01)
			return reply(200, MessagePack.encode({ cursor = 0, patches = {} }))
		end
		table.insert(polls, options.Url)
		if #polls == 1 then
			return reply(409, "")
		end
		connection:stop()
		return reply(200, MessagePack.encode({ cursor = 0, patches = {} }))
	end)
	connection = Connection.new(
		Client.new("http://127.0.0.1:34872", placeReader(123), http),
		root,
		onState,
		deps(extentsTerrainWith(extentsRegion), tree)
	)
	connection:start()
	waitFor(function()
		return #polls >= 2
	end)
	task.wait(0.05)
	connection:stop()

	check(
		"a poll names the session the handshake returned",
		polls[1] ~= nil and string.find(polls[1], "/patches?cursor=0&session=s1", 1, true) ~= nil,
		"polls " .. table.concat(polls, " ")
	)
	check("a 409 sends the plugin back to /info", infos == 2, "handshakes " .. tostring(infos))
	check(
		"the re-handshake polls the new session from cursor 0",
		polls[2] ~= nil and string.find(polls[2], "/patches?cursor=0&session=s2", 1, true) ~= nil,
		"polls " .. table.concat(polls, " ")
	)
	local reportedRestart = false
	for _, entry in log do
		reportedRestart = reportedRestart or (entry.detail ~= nil and Client.isSessionChanged(entry.detail))
	end
	check("a 409 is reported as a restart, not an unknown failure", reportedRestart)
end)

suite("a 400 naming an invalid path drops it, warns about it and never resends it", function()
	local tree, root = studioWorld(true)
	local log, onState = stateLog()
	local posts: { any } = {}
	local client = {
		postChanges = function(_, changes)
			table.insert(posts, changes)
			if #posts == 1 then
				error({
					status = 400,
					message = "naht: /changes returned 400",
					refused = { "Config: v2/Leaf.luau" },
				})
			end
		end,
	}
	local connection = Connection.new(client, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection.pendingPush = {
		["Config: v2/Leaf.luau"] = Protocol.upsert("Config: v2/Leaf.luau", "ModuleScript", "return 2"),
		["Good.luau"] = Protocol.upsert("Good.luau", "ModuleScript", "return 1"),
	}
	connection:_flushPush(1)
	connection:_flushPush(1)

	check("the refused path is never posted again", #posts == 2 and #posts[2] == 1)
	check("the second post carries only the accepted path", posts[2][1].path == "Good.luau")
	check(
		"a 400 naming an invalid path warns about it",
		connection.warnings["refused:Config: v2/Leaf.luau"] ~= nil
	)
	check("an accepted path gets no warning", connection.warnings["refused:Good.luau"] == nil)
	check("the last state is Warning", lastStateOf(log) == "Warning")
	connection:stop()
end)

suite("a 400 refusal drops the named path, warns about it, and posts the rest", function()
	local tree, root = studioWorld(true)
	local log, onState = stateLog()
	local posts: { any } = {}
	local client = {
		postChanges = function(_, changes)
			table.insert(posts, changes)
			if #posts == 1 then
				error({ status = 400, message = "naht: /changes returned 400", refused = { "Dir" } })
			end
		end,
	}
	local connection = Connection.new(client, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection.pendingPush = {
		Dir = Protocol.upsert("Dir", "Folder", ""),
		["Good.luau"] = Protocol.upsert("Good.luau", "ModuleScript", "return 1"),
	}
	connection:_flushPush(1)

	check("a refused batch is posted again without the refused path", #posts == 2 and #posts[2] == 1)
	check("the second post carries only the accepted path", posts[2][1].path == "Good.luau")
	check("a 400 refusal warns about the dropped path", connection.warnings["refused:Dir"] ~= nil)
	check("the last state after a 400 refusal is Warning", lastStateOf(log) == "Warning")
	connection:stop()
end)

local function inventoryClient()
	local posts: { { any } } = {}
	local client = {
		postChanges = noop,
		postInventory = function(_, scripts, _sessionId)
			table.insert(posts, scripts)
			return true
		end,
		postAck = noop,
	}
	return client, posts
end

local function pathsOf(scripts: { any }): { string }
	local paths: { string } = {}
	for _, entry in scripts do
		table.insert(paths, entry.path)
	end
	return paths
end

local function entryAt(scripts: { any }, path: string): any?
	for _, entry in scripts do
		if entry.path == path then
			return entry
		end
	end
	return nil
end

local function attachedConnection(client: any, tree: any, root: any, onState: ((string, string?) -> ())?): any
	local connection = Connection.new(client, root, onState or noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_attachStudio()
	return connection
end

suite("a rename made while the plugin was stopped is posted as an inventory", function()
	local tree, root = studioWorld(true)
	local script = place(tree, "ModuleScript", "Old", root)
	script.Source = "return 1"
	script.Name = "New"
	local posted: any = nil
	local connection: any = nil
	connection = Connection.new({
		info = function(_, _placeId)
			return { project_name = "demo", session_id = "s1", terrain_sync = false, conflicted = {} }
		end,
		patches = function(_, _cursor)
			connection:stop()
			return { cursor = 0, patches = {} }
		end,
		postInventory = function(_, scripts, _sessionId)
			posted = scripts
			return true
		end,
	}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection:_attachStudio()
	connection:_run(1)

	check(
		"the inventory lists the renamed script under its new path",
		posted ~= nil and #posted == 1 and posted[1].path == "New.luau",
		tostring(posted ~= nil and #posted or -1)
	)
	local entry = if posted ~= nil then entryAt(posted, "New.luau") else nil
	check("the inventory entry names the script class", entry ~= nil and entry.class == "ModuleScript")
	check(
		"the inventory entry carries the fingerprint of the source Studio holds",
		entry ~= nil and entry.hash == "8-0a60e4b81d078f22",
		if entry ~= nil then entry.hash else "not listed"
	)
end)

suite("an empty tree posts no inventory", function()
	local tree, root = studioWorld(true)
	local posts = 0
	local connection: any = nil
	connection = Connection.new({
		info = function(_, _placeId)
			return { project_name = "demo", session_id = "s1", terrain_sync = false, conflicted = {} }
		end,
		patches = function(_, _cursor)
			connection:stop()
			return { cursor = 0, patches = {} }
		end,
		postInventory = function(_, _changes, _sessionId)
			posts += 1
			return true
		end,
	}, root, noop, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection:_attachStudio()
	connection:_run(1)

	check("an empty tree posts no inventory", posts == 0, tostring(posts))
end)

suite("the source fingerprint matches its golden values", function()
	check(
		"the fingerprint of 'return 1' is the golden value",
		Fingerprint.source("return 1") == "8-0a60e4b81d078f22",
		Fingerprint.source("return 1")
	)
	check(
		"the fingerprint of 'return 2' is the golden value",
		Fingerprint.source("return 2") == "8-0a60e4b91d078f23",
		Fingerprint.source("return 2")
	)
	check("the fingerprint of an empty source is zero", Fingerprint.source("") == "0-0000000000000000", Fingerprint.source(""))
end)

suite("an inventory entry carries the fingerprint of the source Studio holds now", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local script = place(tree, "ModuleScript", "Old", root)
	script.Source = "return 2"
	local connection = attachedConnection(client, tree, root)
	connection:_postInventory("s1")
	local entry = if posts[1] ~= nil then entryAt(posts[1], "Old.luau") else nil
	check("the inventory lists the placed script under its path", entry ~= nil)
	check(
		"the inventory hashes the source Studio holds",
		entry ~= nil and entry.hash == Fingerprint.source("return 2"),
		if entry ~= nil then entry.hash else "not listed"
	)
	connection:stop()
end)

suite("a rename refused as a device name is listed under the path the daemon still holds", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local script = place(tree, "ModuleScript", "Foo", root)
	tree.setSource(script, "return 1")
	local connection = attachedConnection(client, tree, root)
	script.Name = "CON"
	connection:_postInventory("s1")
	local listed: { string } = if posts[1] ~= nil then pathsOf(posts[1]) else {}
	check(
		"the inventory keeps Foo.luau, the path the daemon holds",
		table.find(listed, "Foo.luau") ~= nil,
		table.concat(listed, ",")
	)
	check("the inventory never lists the refused CON.luau", table.find(listed, "CON.luau") == nil, table.concat(listed, ","))
	connection:stop()
end)

suite("a rename onto a held name is refused, and each script is listed under its own path and source", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local a = place(tree, "ModuleScript", "A", root)
	tree.setSource(a, "return 'a'")
	local b = place(tree, "ModuleScript", "B", root)
	tree.setSource(b, "return 'b'")
	local connection = attachedConnection(client, tree, root)
	b.Name = "A"
	connection:_postInventory("s1")
	local scripts: { any } = if posts[1] ~= nil then posts[1] else {}
	local listedA = entryAt(scripts, "A.luau")
	local listedB = entryAt(scripts, "B.luau")
	check(
		"the holder A.luau is listed with its own hash",
		listedA ~= nil and listedA.hash == Fingerprint.source("return 'a'"),
		if listedA ~= nil then listedA.hash else "not listed"
	)
	check(
		"the refused rename's script is listed as B.luau with its own hash",
		listedB ~= nil and listedB.hash == Fingerprint.source("return 'b'"),
		if listedB ~= nil then listedB.hash else "not listed"
	)
	connection:stop()
end)

suite("two scripts claiming one placed path are both left out of the inventory", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local a = place(tree, "ModuleScript", "A", root)
	tree.setSource(a, "return 'a'")
	local b = place(tree, "ModuleScript", "B", root)
	tree.setSource(b, "return 'b'")
	local connection = attachedConnection(client, tree, root)
	connection.placed[b] = "A.luau"
	connection:_postInventory("s1")
	local listed: { string } = if posts[1] ~= nil then pathsOf(posts[1]) else {}
	check("neither script claiming A.luau is listed", table.find(listed, "A.luau") == nil, table.concat(listed, ","))
	connection:stop()
end)

suite("a script named .Hidden is never listed in the inventory", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local keep = place(tree, "ModuleScript", "Keep", root)
	tree.setSource(keep, "return 1")
	local hidden = place(tree, "ModuleScript", ".Hidden", root)
	tree.setSource(hidden, "return 2")
	local connection = attachedConnection(client, tree, root)
	connection:_postInventory("s1")
	local listed: { string } = if posts[1] ~= nil then pathsOf(posts[1]) else {}
	check("the inventory lists Keep.luau", table.find(listed, "Keep.luau") ~= nil, table.concat(listed, ","))
	check(
		"the inventory never lists the hidden name",
		table.find(listed, ".Hidden.luau") == nil and #listed == 1,
		table.concat(listed, ",")
	)
	connection:stop()
end)

suite("a 400 on /inventory warns and the run still pulls patches, without a Reconnecting state", function()
	local real = Client.new("http://127.0.0.1:34872", placeReader(123), fakeHttp(function(_options)
		return reply(400, '{"refused_paths":[".Hidden.luau"]}')
	end))
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local keep = place(tree, "ModuleScript", "Keep", root)
	tree.setSource(keep, "return 1")
	local pulls = 0
	local connection: any = nil
	connection = Connection.new({
		info = function(_, _placeId)
			return { project_name = "demo", session_id = "s1", terrain_sync = false, conflicted = {} }
		end,
		patches = function(_, _cursor)
			pulls += 1
			connection:stop()
			return { cursor = 0, patches = {} }
		end,
		postInventory = function(_, scripts, sessionId)
			return real:postInventory(scripts, sessionId)
		end,
		postAck = noop,
		postChanges = noop,
	}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection:_attachStudio()
	connection:_run(1)

	local reconnecting = false
	local warned = false
	for _, entry in log do
		if entry.state == "Reconnecting" then
			reconnecting = true
		end
		if entry.state == "Warning" and string.find(entry.detail or "", "/inventory returned 400", 1, true) ~= nil then
			warned = true
		end
	end
	check("a 400 on /inventory leaves the run pulling patches", pulls > 0, tostring(pulls))
	check("a 400 on /inventory does not enter Reconnecting", not reconnecting)
	check("a 400 on /inventory is reported as a Warning", warned)
end)

suite("a 413 on /inventory warns once, and a second post in the same session is not sent", function()
	local posts = 0
	local client = {
		postChanges = noop,
		postAck = noop,
		postInventory = function(_, _scripts, _sessionId)
			posts += 1
			error({ status = 413, message = "naht: /inventory returned 413" })
		end,
	}
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local keep = place(tree, "ModuleScript", "Keep", root)
	tree.setSource(keep, "return 1")
	local connection = attachedConnection(client, tree, root, onState)
	connection:_postInventory("s1")
	connection:_postInventory("s1")
	local warnings = 0
	for _, entry in log do
		if entry.state == "Warning" then
			warnings += 1
		end
	end
	check("a 413 posts the inventory once in the session", posts == 1, tostring(posts))
	check("a 413 raises one Warning", warnings == 1, tostring(warnings))
	check(
		"the 413 Warning names the refusal",
		connection.warnings.inventory ~= nil and string.find(connection.warnings.inventory, "413", 1, true) ~= nil
	)
	connection:stop()
end)

suite("a lone Dir/init.luau is listed under the path the plugin wrote it to", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local connection = attachedConnection(client, tree, root)
	local applier = Apply.create(tree, connection:_echo())
	connection.applying = true
	connection:_applyPatch(applier, { kind = "Create", direction = "ToStudio", path = "Dir/init.luau", class = "ModuleScript", content = "return 'dir'" })
	connection.applying = false
	connection:_adoptPlacement()
	connection:_postInventory("s1")
	local listed: { string } = if posts[1] ~= nil then pathsOf(posts[1]) else {}
	check("the inventory names the written path Dir/init.luau", table.find(listed, "Dir/init.luau") ~= nil, table.concat(listed, ","))
	check("the inventory does not name the flat Dir.luau", table.find(listed, "Dir.luau") == nil, table.concat(listed, ","))
	connection:stop()
end)

suite("a source edit to a childless Dir/init.luau upserts Dir/init.luau, not Dir.luau", function()
	local client = inventoryClient()
	local tree, root = studioWorld(true)
	local connection = attachedConnection(client, tree, root)
	local applier = Apply.create(tree, connection:_echo())
	connection.applying = true
	connection:_applyPatch(applier, { kind = "Create", direction = "ToStudio", path = "Dir/init.luau", class = "ModuleScript", content = "return 'dir'" })
	connection.applying = false
	local node = findByClass(root, "Dir", "ModuleScript")
	tree.setSource(node, "return 'edited'")
	connection:_onSourceChanged(node)
	check("the source edit is queued under Dir/init.luau", connection.pendingPush["Dir/init.luau"] ~= nil)
	check("the source edit is not queued under Dir.luau", connection.pendingPush["Dir.luau"] == nil)
	connection:stop()
end)

suite("applying a patch returns the path it wrote and the node, and a delete returns nothing", function()
	local tree, root, applier = freshApplier()
	local written, node = applier:apply({ path = "Dir/init.luau", class = "ModuleScript", kind = "Create", content = "return 1" })
	check("an init patch returns the init path it wrote", written == "Dir/init.luau", tostring(written))
	check("an init patch returns the node it wrote", node ~= nil and node.Name == "Dir" and node.Parent == root)
	local flatWritten = applier:apply({ path = "Flat.luau", class = "ModuleScript", kind = "Create", content = "return 2" })
	check("a flat patch returns the flat path it wrote", flatWritten == "Flat.luau", tostring(flatWritten))
	local deletedWritten, deletedNode = applier:apply({ path = "Flat.luau", class = "ModuleScript", kind = "Delete" })
	check("a delete returns no path and no node", deletedWritten == nil and deletedNode == nil)
end)

suite("a rename followed by a removal in one window sends only Deletes", function()
	local client, posts, attempts = recordingClient()
	local connection, tree, root = renameWorld(client)
	local script = place(tree, "ModuleScript", "A", root)
	tree.setSource(script, "return 1")
	waitFor(function() return #attempts >= 1 end)
	script.Name = "B"
	tree.destroy(script)
	waitFor(function() return #attempts >= 2 end)
	task.wait(0.6)
	local later = table.move(posts, 2, #posts, 1, {})
	check("the renamed-from path is deleted", anyChange(later, "Delete", "A.luau"), tostring(#attempts) .. " attempts")
	check("the removed path is deleted", anyChange(later, "Delete", "B.luau"))
	check("the removed path is never upserted", not anyChange(later, "Upsert", "B.luau"))
	connection:stop()
end)

suite("a folder rename of several hundred scripts reads the root's descendants a bounded number of times", function()
	local client = inventoryClient()
	local tree, root = studioWorld(true)
	local folder = place(tree, "Folder", "Lib", root)
	local last: any = nil
	for index = 1, 300 do
		last = place(tree, "ModuleScript", "M" .. index, folder)
		tree.setSource(last, "return " .. index)
	end
	local connection = attachedConnection(client, tree, root)
	local original = root.GetDescendants
	local reads = 0
	root.GetDescendants = function(self)
		reads += 1
		return original(self)
	end
	folder.Name = "Lib2"
	check(
		"the folder rename moves the last script under the new folder",
		connection.placed[last] == "Lib2/M300.luau",
		tostring(connection.placed[last])
	)
	check("the folder rename reads the root's descendants a bounded number of times", reads <= 2, tostring(reads))
	connection:stop()
end)

suite("a table error from /info is reported by its message in the Reconnecting state", function()
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local infos = 0
	local connection: any = nil
	connection = Connection.new({
		info = function(_, _placeId)
			infos += 1
			if infos > 1 then
				connection:stop()
			end
			error({ status = 503, message = "naht: /info returned 503" })
		end,
	}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection.running = true
	connection.session = 1
	connection:_run(1)

	local detail: string? = nil
	for _, entry in log do
		if entry.state == "Reconnecting" then
			detail = entry.detail
		end
	end
	check("a 503 on /info enters Reconnecting", detail ~= nil)
	check(
		"the Reconnecting detail is the message, not the table address",
		detail == "naht: /info returned 503",
		tostring(detail)
	)
end)

suite("a move that ends with no path restores the pending edit of its old path", function()
	local client, posts, attempts = recordingClient()
	local connection, tree, root = renameWorld(client)
	local script = place(tree, "ModuleScript", "Old", root)
	waitFor(function() return #attempts >= 1 end)
	tree.setSource(script, "return 2")
	tree.destroy(script)
	script.Name = ".Hidden"
	tree.setParent(script, root)
	waitFor(function() return #attempts >= 2 end)
	task.wait(0.6)
	local later = table.move(posts, 2, #posts, 1, {})
	check("the pending edit of the old path is still upserted", anyChange(later, "Upsert", "Old.luau"), tostring(#posts) .. " posts")
	check("the old path's Delete is not sent after the move ends with no path", not anyChange(later, "Delete", "Old.luau"))
	connection:stop()
end)

suite("an inventory is posted once per session id", function()
	local client, posts = inventoryClient()
	local tree, root = studioWorld(true)
	local script = place(tree, "ModuleScript", "A", root)
	script.Source = "return 1"
	local connection = attachedConnection(client, tree, root)
	connection:_postInventory("s1")
	connection:_postInventory("s1")
	check("a second inventory in the same session is not posted", #posts == 1, tostring(#posts))
	connection:_postInventory("s2")
	check("an inventory for a new session id is posted again", #posts == 2, tostring(#posts))
	connection:stop()
end)

suite("a waypoint schedules no terrain push while terrain_sync is off", function()
	local terrain = memoryTerrain(1)
	local history = { OnWaypointAdded = signal() }
	local client = blobRecorder()
	local tree, root = studioWorld(true)
	local delays = 0
	local settings = deps(terrain, tree, history)
	settings.delay = function(_, _fire)
		delays += 1
	end
	local connection = Connection.new(client, root, noop, settings)
	connection.running = true
	connection.session = 1
	connection.epoch = 1
	connection:_attachStudio()
	history.OnWaypointAdded:Fire("sculpt")
	check("a waypoint with terrain_sync off schedules no delay", delays == 0, tostring(delays))
	connection:stop()
end)

suite("a daemon without the inventory route sets an inventory Warning", function()
	local client = {
		postChanges = noop,
		postAck = noop,
		postInventory = function(_, _scripts, _sessionId)
			return false
		end,
	}
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local script = place(tree, "ModuleScript", "A", root)
	script.Source = "return 1"
	local connection = attachedConnection(client, tree, root, onState)
	connection:_postInventory("s1")
	check(
		"a 404 on /inventory sets an inventory Warning",
		connection.warnings.inventory ~= nil,
		tostring(connection.warnings.inventory)
	)
	check("the Warning reaches onState", lastStateOf(log) == "Warning", lastStateOf(log))
	connection:stop()
end)

suite("an inventory refused for a missing token asks for the token and is posted once it is submitted", function()
	local log, onState = stateLog()
	local tree, root = studioWorld(true)
	local renamed = place(tree, "ModuleScript", "Renamed", root)
	renamed.Source = "return 1"
	local posts: { any } = {}
	local tokenGiven = false
	local connection: any = nil
	connection = Connection.new({
		info = function(_, _placeId)
			return { project_name = "demo", session_id = "s1", terrain_sync = false, conflicted = {} }
		end,
		patches = function(_, _cursor)
			connection:stop()
			return { cursor = 0, patches = {} }
		end,
		postInventory = function(_, scripts, _sessionId)
			if not tokenGiven then
				error({ status = 401, message = Client.TOKEN_REQUIRED, tokenRefused = true })
			end
			table.insert(posts, scripts)
			return true
		end,
		setToken = function(_, _token)
			tokenGiven = true
		end,
		postAck = noop,
	}, root, onState, deps(extentsTerrainWith(extentsRegion), tree))
	connection:start()

	check("a 401 on /inventory asks for the token", lastStateOf(log) == "TokenRequired", lastStateOf(log))
	local inventoryWarnings = 0
	for _, entry in log do
		if entry.state == "Warning" and string.find(entry.detail or "", "inventory", 1, true) ~= nil then
			inventoryWarnings += 1
		end
	end
	check("a 401 on /inventory raises no inventory Warning", inventoryWarnings == 0, tostring(inventoryWarnings))
	check("no inventory is posted before the token is given", #posts == 0, tostring(#posts))

	connection:submitToken("typed-token")
	check("the typed token is handed to the client", tokenGiven)
	check("the inventory is posted again once the token is submitted", #posts == 1, tostring(#posts))
	check(
		"the re-posted inventory lists the renamed script",
		#posts == 1 and entryAt(posts[1], "Renamed.luau") ~= nil,
		if #posts == 1 then table.concat(pathsOf(posts[1]), ",") else "no post"
	)
end)

suite("a daemon that switches project reloads the stored token for the new project id", function()
	local saved: { [string]: string } = { ["p-1"] = "token-one", ["p-2"] = "token-two" }
	local store = {
		load = function(projectId: string): string?
			return saved[projectId]
		end,
		save = function(projectId: string, token: string)
			saved[projectId] = token
		end,
	}
	local serving = fromHex(GOLDEN.server_info)
	local http = fakeHttp(function(options)
		if string.find(options.Url, "/info", 1, true) ~= nil then
			return reply(200, serving)
		end
		local presented = options.Headers["x-naht-token"]
		if presented == "token-two" then
			return reply(200, fromHex(GOLDEN.patch_batch))
		end
		if presented == nil then
			return reply(401, '{"error": "token required"}')
		end
		return reply(401, '{"error": "token rejected"}')
	end)
	local client = Client.new("http://127.0.0.1:34872", placeReader(123), http, store)
	client:info()
	check("the first project takes its stored token", client.token == "token-one", tostring(client.token))

	serving = fromHex((string.gsub(GOLDEN.server_info, "a3702d31", "a3702d32")))
	client:info()
	check("the client now reports the second project", client.projectId == "p-2", tostring(client.projectId))
	check("a daemon switch takes the token stored for the new project", client.token == "token-two", tostring(client.token))
	local okSwitched = pcall(function()
		client:patches(0, "s1")
	end)
	check("a gated request to the switched daemon is accepted without a prompt", okSwitched)

	serving = fromHex((string.gsub(GOLDEN.server_info, "a3702d31", "a3702d33")))
	client:info()
	check("a project with no stored token leaves the token empty", client.token == nil, tostring(client.token))
	local okUnstored, errUnstored = pcall(function()
		client:patches(0, "s1")
	end)
	check(
		"a project with no stored token is prompted for the token",
		not okUnstored and Support.isTokenRefusal(errUnstored) and errUnstored.message == Client.TOKEN_REQUIRED,
		tostring(errUnstored)
	)
end)

if failures > 0 then
	error(string.format("%d harness check(s) failed", failures))
end
print("all harness checks passed")
