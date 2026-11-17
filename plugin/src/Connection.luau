--!strict

local Connection = {}
Connection.__index = Connection

local BACKOFF_START = 1
local BACKOFF_MAX = 15
local BLOB_BACKOFF_START = 2
local BLOB_BACKOFF_MAX = 60
local PUSH_DEBOUNCE = 0.3
local PUSH_RETRY = 2
local TERRAIN_RESOLUTION = 4
local TERRAIN_PATH = "Terrain.terrain"
local TERRAIN_TOO_LARGE = "terrain too large for live push, use naht build"

local CLASS_SUFFIX = {
	Script = ".server.luau",
	LocalScript = ".client.luau",
	ModuleScript = ".luau",
}

local UNSAFE_NAME_CHARACTERS = { "/", "\\", "\0" }
local SCRIPT_SUFFIXES = { ".server", ".client", ".luau" }

local function isSafeName(name: string): boolean
	for _, character in UNSAFE_NAME_CHARACTERS do
		if string.find(name, character, 1, true) ~= nil then
			return false
		end
	end
	return true
end

local function endsWithScriptSuffix(name: string): boolean
	for _, suffix in SCRIPT_SUFFIXES do
		if string.sub(name, -#suffix) == suffix then
			return true
		end
	end
	return false
end

export type State = "Connecting" | "Connected" | "Reconnecting" | "Conflict" | "Warning" | "Disconnected"

function Connection.new(
	client: any,
	root: Instance,
	placeId: number,
	onState: (State, string?) -> (),
	deps: { terrain: any, tree: any?, apply: any, protocol: any, terrainBlob: any, warn: ((string) -> ())? }
)
	return setmetatable({
		client = client,
		root = root,
		placeId = placeId,
		onState = onState,
		terrain = deps.terrain,
		tree = deps.tree,
		apply = deps.apply,
		protocol = deps.protocol,
		terrainBlob = deps.terrainBlob,
		warn = deps.warn or warn,
		running = false,
		session = 0,
		backoff = BACKOFF_START,
		blobBackoff = BLOB_BACKOFF_START,
		cursor = 0,
		blobCursor = 0,
		epoch = 0,
		applying = false,
		lastApplied = {},
		pendingPush = {},
		pushScheduled = false,
		studioConnections = nil,
		skippedPaths = {},
	}, Connection)
end

function Connection.nextBlobBackoff(current: number): number
	return math.min(current * 2, BLOB_BACKOFF_MAX)
end

function Connection.start(self)
	if self.running then
		return
	end
	self.running = true
	self.session += 1
	local session = self.session
	self:_attachStudio()
	task.spawn(function()
		self:_run(session)
	end)
end

function Connection.stop(self)
	self.running = false
	self.session += 1
	self.epoch += 1
	self.pendingPush = {}
	self.pushScheduled = false
	self:_detachStudio()
	self.onState("Disconnected", nil)
end

function Connection._isCurrent(self, session: number): boolean
	return self.running and self.session == session
end

function Connection._isBlobCurrent(self, epoch: number): boolean
	return self.running and self.epoch == epoch
end

function Connection._echo(self)
	return {
		expect = function(path: string, content: string)
			self.lastApplied[path] = content
		end,
		forget = function(path: string)
			self.lastApplied[path] = nil
		end,
	}
end

function Connection._unsafeReason(_self, instance: Instance, segments: { string }): string?
	for _, segment in segments do
		if not isSafeName(segment) then
			return "a name contains '/', '\\' or NUL"
		end
	end
	if instance.ClassName == "ModuleScript" and endsWithScriptSuffix(instance.Name) then
		return "a ModuleScript name ends in .server, .client or .luau"
	end
	return nil
end

function Connection._pathOf(self, instance: Instance): string?
	local suffix = CLASS_SUFFIX[instance.ClassName]
	if suffix == nil then
		return nil
	end
	local segments = { instance.Name .. suffix }
	local current = instance.Parent
	while current ~= nil and current ~= self.root do
		table.insert(segments, 1, current.Name)
		current = current.Parent
	end
	if current ~= self.root then
		return nil
	end
	local path = table.concat(segments, "/")
	local reason = self:_unsafeReason(instance, segments)
	if reason ~= nil then
		if not self.skippedPaths[path] then
			self.skippedPaths[path] = true
			self.warn(string.format("Naht: not syncing %q, %s", path, reason))
		end
		return nil
	end
	return path
end

function Connection._run(self, session: number)
	while self:_isCurrent(session) do
		self.onState("Connecting", nil)
		local ok, err = pcall(function()
			local info = self.client:info(self.placeId)
			if not self:_isCurrent(session) then
				return
			end
			self.onState("Connected", info.project_name)
			self.cursor = 0
			self.blobCursor = 0
			self.epoch += 1
			local epoch = self.epoch
			self:_pushTerrain(session)
			if not self:_isCurrent(session) then
				return
			end
			task.spawn(function()
				self:_serveBlobs(epoch)
			end)
			self:_serve(session)
		end)
		if not self:_isCurrent(session) then
			break
		end
		if not ok then
			self.onState("Reconnecting", tostring(err))
			task.wait(self.backoff)
			self.backoff = math.min(self.backoff * 2, BACKOFF_MAX)
		end
	end
end

function Connection._serve(self, session: number)
	while self:_isCurrent(session) do
		local batch = self.client:patches(self.cursor)
		if not self:_isCurrent(session) then
			return
		end
		self.applying = true
		local applier = self.apply.create(self.tree, self:_echo())
		local applied = {}
		for _, patch in batch.patches do
			if not self:_isCurrent(session) then
				break
			end
			local ok = pcall(function()
				applier:apply(patch)
			end)
			if ok then
				table.insert(applied, patch.path)
				if patch.kind == "Conflict" then
					self.onState("Conflict", patch.path)
				end
			end
		end
		self.applying = false
		if not self:_isCurrent(session) then
			return
		end
		self.cursor = batch.cursor
		self.backoff = BACKOFF_START
		if #applied > 0 and self:_isCurrent(session) then
			pcall(function()
				self.client:postAck(applied)
			end)
		end
	end
end

function Connection._serveBlobs(self, epoch: number)
	while self:_isBlobCurrent(epoch) do
		local ok, batch = pcall(function()
			return self.client:blobs(self.blobCursor)
		end)
		if not ok or not self:_isBlobCurrent(epoch) then
			return
		end
		local allApplied = true
		local rejection = ""
		for _, patch in batch.patches do
			if not self:_isBlobCurrent(epoch) then
				return
			end
			if patch.kind == "Conflict" then
				self.onState("Conflict", patch.path)
			elseif patch.direction == "ToStudio" and patch.content ~= nil then
				local applied, err = pcall(function()
					self:_applyBlob(patch.content)
				end)
				if not applied then
					allApplied = false
					rejection = tostring(err)
				end
			end
		end
		if allApplied then
			self.blobCursor = batch.cursor
			self.blobBackoff = BLOB_BACKOFF_START
		else
			self.onState("Warning", "terrain blob rejected: " .. rejection)
			task.wait(self.blobBackoff)
			self.blobBackoff = Connection.nextBlobBackoff(self.blobBackoff)
		end
	end
end

function Connection._terrainRegion(self): any
	return self.terrain.MaxExtents:ExpandToGrid(TERRAIN_RESOLUTION)
end

function Connection._applyBlob(self, blob: string)
	self.terrainBlob.write(self.terrain, self:_terrainRegion(), blob)
end

function Connection._pushTerrain(self, session: number)
	local regionOk, region = pcall(self._terrainRegion, self)
	if not regionOk then
		return
	end
	local size = region.Size
	if self.terrainBlob.voxelCount(size.X, size.Y, size.Z, TERRAIN_RESOLUTION) > self.terrainBlob.MAX_VOXELS then
		self.onState("Warning", TERRAIN_TOO_LARGE)
		return
	end
	local ok, blob = pcall(self.terrainBlob.read, self.terrain, region, TERRAIN_RESOLUTION)
	if not ok or not self:_isCurrent(session) then
		return
	end
	pcall(function()
		self.client:postBlobs({ { path = TERRAIN_PATH, class = "Terrain", content = blob } })
	end)
end

function Connection._attachStudio(self)
	if self.studioConnections ~= nil then
		return
	end
	local connections = {}
	self.studioConnections = connections

	local function listen(signal: any, handler: (...any) -> ())
		table.insert(connections, signal:Connect(handler))
	end

	local function onChanged(instance: Instance)
		local path = self:_pathOf(instance)
		if path == nil then
			return
		end
		local source = (instance :: any).Source
		local expected = self.lastApplied[path]
		if expected ~= nil and expected == source then
			self.lastApplied[path] = nil
			return
		end
		if self.applying then
			return
		end
		self.lastApplied[path] = nil
		self.pendingPush[path] = self.protocol.upsert(path, instance.ClassName, source)
		self:_schedulePush()
	end

	local function track(instance: Instance)
		if CLASS_SUFFIX[instance.ClassName] ~= nil then
			listen(instance:GetPropertyChangedSignal("Source"), function()
				onChanged(instance)
			end)
		end
	end

	for _, descendant in self.root:GetDescendants() do
		track(descendant)
	end
	listen(self.root.DescendantAdded, function(descendant: Instance)
		track(descendant)
		onChanged(descendant)
	end)
	listen(self.root.DescendantRemoving, function(descendant: Instance)
		if self.applying then
			return
		end
		local path = self:_pathOf(descendant)
		if path ~= nil then
			self.pendingPush[path] = self.protocol.delete(path)
			self:_schedulePush()
		end
	end)
end

function Connection._detachStudio(self)
	local connections = self.studioConnections
	if connections == nil then
		return
	end
	self.studioConnections = nil
	for _, connection in connections do
		connection:Disconnect()
	end
end

function Connection._schedulePush(self, delay: number?)
	if self.pushScheduled then
		return
	end
	self.pushScheduled = true
	local session = self.session
	task.delay(delay or PUSH_DEBOUNCE, function()
		if not self:_isCurrent(session) then
			return
		end
		self.pushScheduled = false
		local sent = self.pendingPush
		self.pendingPush = {}
		local changes = {}
		for _, change in sent do
			table.insert(changes, change)
		end
		if #changes == 0 then
			return
		end
		local ok = pcall(function()
			self.client:postChanges(changes)
		end)
		if not ok and self:_isCurrent(session) then
			for path, change in sent do
				if self.pendingPush[path] == nil then
					self.pendingPush[path] = change
				end
			end
			self:_schedulePush(PUSH_RETRY)
		end
	end)
end

return Connection
