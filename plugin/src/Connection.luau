--!strict
-- The connection loop: handshake, long-poll + apply, Studio → filesystem change push, and
-- reconnect-with-backoff. State changes are reported through `onState` so the UI stays a dumb view.
--
-- No sync logic lives here: the daemon decides everything. The plugin only carries patches in,
-- carries edits out, and reports connection state. On every (re)connect it resets its cursor to 0 so
-- the daemon's reconnect re-diff drives a clean catch-up rather than a blind replay.

local Apply = require(if script ~= nil then script.Parent.Apply else "./Apply")
local Protocol = require(if script ~= nil then script.Parent.Protocol else "./Protocol")
local Terrain = require(if script ~= nil then script.Parent.Terrain else "./Terrain")

local Connection = {}
Connection.__index = Connection

local BACKOFF_START = 1
local BACKOFF_MAX = 15
local PUSH_DEBOUNCE = 0.3
local PUSH_RETRY = 2

-- Terrain is synced as one opaque voxel blob over the dedicated `/blobs` channel. The whole region is
-- read/written at the engine's 4-stud voxel resolution; there is only one Terrain per place, so its
-- filesystem path is fixed.
local TERRAIN_RESOLUTION = 4
local TERRAIN_PATH = "Terrain.terrain"

local CLASS_SUFFIX = {
	Script = ".server.luau",
	LocalScript = ".client.luau",
	ModuleScript = ".luau",
}

local UNSAFE_NAME_CHARACTERS = { "/", "\\", "\0" }

local function isSafeName(name: string): boolean
	for _, character in UNSAFE_NAME_CHARACTERS do
		if string.find(name, character, 1, true) ~= nil then
			return false
		end
	end
	return true
end

export type State = "Connecting" | "Connected" | "Reconnecting" | "Conflict" | "Disconnected"

function Connection.new(
	client: any,
	root: Instance,
	placeId: number,
	onState: (State, string?) -> (),
	deps: { terrain: any, tree: Apply.Tree? }
)
	return setmetatable({
		client = client,
		root = root,
		placeId = placeId,
		onState = onState,
		terrain = deps.terrain,
		tree = deps.tree,
		running = false,
		session = 0,
		backoff = BACKOFF_START,
		cursor = 0,
		blobCursor = 0,
		-- Bumped on every (re)connect so a stale blob long-poll from a prior connection stops itself.
		epoch = 0,
		applying = false,
		-- The content last written by an apply, per path. A Studio change signal whose value matches
		-- is the echo of our own apply (which may fire deferred, after `applying` clears) and is
		-- dropped instead of pushed back.
		lastApplied = {},
		pendingPush = {},
		pushScheduled = false,
		studioConnections = nil,
		skippedPaths = {},
	}, Connection)
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
	self:_detachStudio()
	self.onState("Disconnected", nil)
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
		return nil -- not under the project root
	end
	local path = table.concat(segments, "/")
	for _, segment in segments do
		if not isSafeName(segment) then
			if not self.skippedPaths[path] then
				self.skippedPaths[path] = true
				warn(string.format("Naht: not syncing %q, a name contains '/', '\\' or NUL", path))
			end
			return nil
		end
	end
	return path
end

-- The outer loop: handshake, serve patches, and on any failure back off and retry.
function Connection._run(self, session: number)
	while self.running and self.session == session do
		self.onState("Connecting", nil)
		local ok, err = pcall(function()
			local info = self.client:info(self.placeId)
			self.onState("Connected", info.project_name)
			self.cursor = 0 -- let the daemon's re-diff drive the catch-up
			self.blobCursor = 0
			self.epoch += 1
			-- Push the current terrain once on connect, then long-poll the blob channel alongside the
			-- text patch loop. The blob loop self-stops when its epoch is superseded.
			local epoch = self.epoch
			self:_pushTerrain()
			task.spawn(function()
				self:_serveBlobs(epoch)
			end)
			self:_serve(session)
		end)
		if not (self.running and self.session == session) then
			break
		end
		if not ok then
			self.onState("Reconnecting", tostring(err))
			task.wait(self.backoff)
			self.backoff = math.min(self.backoff * 2, BACKOFF_MAX)
		end
	end
end

-- The inner long-poll loop. Returns (raising) when the connection breaks, so `_run` can back off.
function Connection._serve(self, session: number)
	while self.running and self.session == session do
		local batch = self.client:patches(self.cursor)
		self.applying = true
		local applier = Apply.create(self.tree :: Apply.Tree, function(path: string, content: string)
			self.lastApplied[path] = content
		end)
		local applied = {}
		for _, patch in batch.patches do
			-- Apply each patch independently so one failure doesn't sink the whole batch; only the
			-- ones that land are acked, and the rest re-diff on the next cycle (no loss).
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
		self.cursor = batch.cursor
		self.backoff = BACKOFF_START
		if #applied > 0 then
			pcall(function()
				self.client:postAck(applied)
			end)
		end
	end
end

-- The blob long-poll loop: terrain patches flow filesystem → Studio here, applied via `WriteVoxels`.
-- It runs in its own task so it doesn't block the text patch loop; it stops when its epoch is
-- superseded by a reconnect or when a request raises (the outer `_run` then re-handshakes).
function Connection._serveBlobs(self, epoch: number)
	while self.running and self.epoch == epoch do
		local ok, batch = pcall(function()
			return self.client:blobs(self.blobCursor)
		end)
		if not ok then
			return
		end
		local allApplied = true
		for _, patch in batch.patches do
			if patch.kind == "Conflict" then
				self.onState("Conflict", patch.path)
			elseif patch.direction == "ToStudio" and patch.content ~= nil then
				-- Apply independently so one bad blob doesn't sink the loop.
				local applied = pcall(function()
					self:_applyBlob(patch.content)
				end)
				allApplied = allApplied and applied
			end
		end
		if allApplied then
			self.blobCursor = batch.cursor
		else
			task.wait(PUSH_RETRY)
		end
	end
end

-- The Region3 covering the whole terrain, at voxel resolution, used for both read and write so the
-- blob round-trips against a stable region.
function Connection._terrainRegion(self): any
	return self.terrain.MaxExtents:ExpandToGrid(TERRAIN_RESOLUTION)
end

-- Write an incoming voxel blob into the live terrain.
function Connection._applyBlob(self, blob: string)
	Terrain.write(self.terrain, self:_terrainRegion(), blob)
end

-- Read the current terrain and push it to the filesystem as one blob. Best-effort: a read or post
-- failure is left for the next connect rather than killing the session.
function Connection._pushTerrain(self)
	local ok, blob = pcall(function()
		return Terrain.read(self.terrain, self:_terrainRegion(), TERRAIN_RESOLUTION)
	end)
	if not ok then
		return
	end
	pcall(function()
		self.client:postBlobs({ { path = TERRAIN_PATH, class = "Terrain", content = blob } })
	end)
end

-- Watch the project subtree for Studio-side edits and push them, debounced. Edits the plugin itself
-- just applied are ignored (the `applying` guard) so a patch does not echo back as a change.
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
		-- Drop the echo of a patch we just applied (handles deferred change signals too).
		if self.applying or self.lastApplied[path] == source then
			self.lastApplied[path] = nil
			return
		end
		self.pendingPush[path] = Protocol.upsert(path, instance.ClassName, source)
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
			self.pendingPush[path] = Protocol.delete(path)
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
	task.delay(delay or PUSH_DEBOUNCE, function()
		self.pushScheduled = false
		-- Snapshot and clear, so edits arriving during the request aren't lost.
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
		if not ok then
			-- Never drop an edit: restore anything not superseded by a newer pending change, and
			-- retry after a backoff so a downed daemon doesn't spin a hot loop.
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
