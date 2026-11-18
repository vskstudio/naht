--!strict

local Client = require(if script ~= nil then script.Parent.Client else "./Client")

local Connection = {}
Connection.__index = Connection

local BACKOFF_START = 1
local BACKOFF_MAX = 15
local BLOB_BACKOFF_START = 2
local BLOB_BACKOFF_MAX = 60
local PUSH_DEBOUNCE = 0.3
local PUSH_RETRY = 2
local TERRAIN_BACKOFF_START = 1
local TERRAIN_BACKOFF_MAX = 60
local TERRAIN_DEBOUNCE = 1
local TERRAIN_RESOLUTION = 4
local TERRAIN_PATH = "Terrain.terrain"
local TERRAIN_TOO_LARGE = "terrain too large for live push, use naht build"
local DECODE_FAILURE_LIMIT = 5
local ACK_BACKOFF_START = 2
local REJECTED_PATCH_START = 1
local RETRY_BACKOFF_MAX = 60
local SERVER_ERROR_LIMIT = 3
local PUSH_ABANDON_AFTER = 30
local PUSH_ABANDON_LIMIT = 3
local sharedPushGate: PushGate = { issued = 0, flight = nil }

local CLASS_SUFFIX = {
	Script = ".server.luau",
	LocalScript = ".client.luau",
	ModuleScript = ".luau",
}

local UNSAFE_NAME_CHARACTERS = { "/", "\\", ":", "<", ">", "\"", "|", "?", "*" }
for code = 0, 31 do
	table.insert(UNSAFE_NAME_CHARACTERS, string.char(code))
end
local UNSAFE_NAMES = { [""] = true, ["."] = true, [".."] = true }
local SCRIPT_SUFFIXES = { ".server", ".client", ".luau" }

local TRANSIENT_STATUSES = { [408] = true, [500] = true, [502] = true, [503] = true, [504] = true }

local function doubled(current: number, max: number): number
	return math.min(current * 2, max)
end

local function isTransientFailure(err: any): boolean
	if type(err) ~= "table" or err.status == nil then
		return true
	end
	return TRANSIENT_STATUSES[err.status] == true
end

local function failureMessage(err: any): string
	if type(err) == "table" then
		return tostring(err.message)
	end
	return tostring(err)
end

local function isServerError(err: any): boolean
	return type(err) == "table" and err.status == 500
end

local function isUndecodableFailure(err: any): boolean
	return type(err) == "table" and err.undecodable == true
end

local function refusedPathsOf(err: any): { string }
	if type(err) ~= "table" or type(err.refused) ~= "table" then
		return {}
	end
	return err.refused
end

local function namedInBatch(batch: { [string]: any }, err: any): { [string]: true }
	local named = {}
	for _, path in refusedPathsOf(err) do
		if batch[path] ~= nil then
			named[path] = true
		end
	end
	return named
end

local function countKeys(map: { [string]: any }): number
	local count = 0
	for _ in map do
		count += 1
	end
	return count
end

local function weakMap(): { [any]: any }
	return setmetatable({}, { __mode = "k" }) :: any
end

local function isSafeName(name: string): boolean
	if UNSAFE_NAMES[name] then
		return false
	end
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

export type Applier = {
	apply: (self: Applier, patch: any) -> (),
}

export type PushFlight = { token: number, batch: { [string]: any } }
export type PushGate = { issued: number, flight: PushFlight? }

export type Deps = {
	terrain: any,
	tree: any?,
	apply: { create: (tree: any, echo: any) -> Applier },
	protocol: {
		upsert: (path: string, class: string, content: string) -> any,
		delete: (path: string) -> any,
	},
	terrainBlob: {
		MAX_VOXELS: number,
		voxelCount: (number, number, number, number) -> number,
		read: (terrain: any, region: any, resolution: number) -> string,
		write: (terrain: any, region: any, blob: string) -> (),
	},
	warn: ((string) -> ())?,
	pushGate: PushGate?,
	delay: ((number, () -> ()) -> ())?,
	history: any?,
}

function Connection.new(client: any, root: Instance, onState: (State, string?) -> (), deps: Deps)
	return setmetatable({
		client = client,
		root = root,
		onState = onState,
		terrain = deps.terrain,
		tree = deps.tree,
		apply = deps.apply,
		protocol = deps.protocol,
		terrainBlob = deps.terrainBlob,
		history = deps.history,
		warn = deps.warn or warn,
		running = false,
		session = 0,
		backoff = BACKOFF_START,
		blobBackoff = BLOB_BACKOFF_START,
		terrainBackoff = TERRAIN_BACKOFF_START,
		pushBackoff = PUSH_RETRY,
		pushServerErrors = 0,
		pushAbandons = 0,
		ackBackoff = ACK_BACKOFF_START,
		rejectedBackoff = REJECTED_PATCH_START,
		terrainInFlight = false,
		terrainBaseline = nil,
		terrainSync = false,
		terrainTicket = 0,
		cursor = 0,
		blobCursor = 0,
		pendingAcks = {},
		epoch = 0,
		applying = false,
		lastApplied = {},
		pendingPush = {},
		pushScheduled = false,
		pushGate = deps.pushGate or sharedPushGate,
		delay = deps.delay or task.delay,
		studioConnections = nil,
		placed = weakMap(),
		groupOf = weakMap(),
		moves = weakMap(),
		skippedPaths = {},
		warnings = {},
		conflicts = {},
		projectName = "",
	}, Connection)
end

function Connection._setWarning(self, source: string, message: string)
	self.warnings[source] = message
	self.onState("Warning", message)
end

function Connection._clearWarning(self, source: string)
	if self.warnings[source] == nil then
		return
	end
	self.warnings[source] = nil
	self:_refreshState()
end

function Connection._setConflict(self, path: string)
	self.conflicts[path] = true
	self:_refreshState()
end

function Connection._clearConflict(self, path: string)
	if self.conflicts[path] == nil then
		return
	end
	self.conflicts[path] = nil
	self:_refreshState()
end

function Connection._sortedKeys(map: { [string]: any }): { string }
	local keys = {}
	for key in map do
		table.insert(keys, key)
	end
	table.sort(keys)
	return keys
end

function Connection._refreshState(self)
	local conflictPaths = Connection._sortedKeys(self.conflicts)
	if #conflictPaths > 0 then
		self.onState("Conflict", conflictPaths[1])
		return
	end
	local warningSources = Connection._sortedKeys(self.warnings)
	if #warningSources > 0 then
		self.onState("Warning", self.warnings[warningSources[1]])
		return
	end
	self.onState("Connected", self.projectName)
end

function Connection.nextBlobBackoff(current: number): number
	return doubled(current, BLOB_BACKOFF_MAX)
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

function Connection._terminate(self, detail: string?)
	self.running = false
	self.session += 1
	self.epoch += 1
	self:_revertPending()
	self.pendingPush = {}
	self.pushScheduled = false
	self.pushServerErrors = 0
	self.pushAbandons = 0
	self.warnings = {}
	self.conflicts = {}
	self:_detachStudio()
	self.onState("Disconnected", detail)
end

function Connection.stop(self)
	self:_terminate(nil)
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
			return "a name contains '/', '\\', ':', '< > \" | ? *' or a control byte, or is empty, '.' or '..'"
		end
	end
	if instance.ClassName == "ModuleScript" and endsWithScriptSuffix(instance.Name) then
		return "a ModuleScript name ends in .server, .client or .luau"
	end
	return nil
end

function Connection._pathOf(self, instance: Instance): string?
	return self:_shapePath(instance, #instance:GetChildren() > 0)
end

function Connection._shapePath(self, instance: Instance, hasChildren: boolean): string?
	local suffix = CLASS_SUFFIX[instance.ClassName]
	if suffix == nil then
		return nil
	end
	local segments = if hasChildren then { instance.Name, "init" .. suffix } else { instance.Name .. suffix }
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
			local info = self.client:info()
			if not self:_isCurrent(session) then
				return
			end
			self.projectName = info.project_name
			self.terrainSync = info.terrain_sync == true
			self.conflicts = {}
			for _, path in info.conflicted or {} do
				self.conflicts[path] = true
			end
			self:_refreshState()
			self:_schedulePush()
			if info.session_id ~= self.sessionId then
				self.cursor = 0
				self.blobCursor = 0
				self.pendingAcks = {}
				self.sessionId = info.session_id
			end
			self.epoch += 1
			local epoch = self.epoch
			self.terrainBaseline = nil
			task.spawn(function()
				self:_pushTerrain(epoch)
			end)
			task.spawn(function()
				self:_serveBlobs(epoch, info.session_id)
			end)
			self:_serve(session, info.session_id)
		end)
		if not self:_isCurrent(session) then
			break
		end
		if not ok then
			if Client.isTerminalError(tostring(err)) then
				self:_terminate(tostring(err))
				break
			end
			self.epoch += 1
			self.onState("Reconnecting", tostring(err))
			if Client.isSessionChanged(tostring(err)) then
				self.backoff = BACKOFF_START
			end
			task.wait(self.backoff)
			if not self:_isCurrent(session) then
				break
			end
			self.backoff = math.min(self.backoff * 2, BACKOFF_MAX)
		end
	end
end

function Connection._serve(self, session: number, sessionId: string)
	if #self.pendingAcks > 0 then
		self.pendingAcks = self:_ack(session, self.pendingAcks, sessionId)
		if not self:_isCurrent(session) then
			return
		end
	end
	while self:_isCurrent(session) do
		local batch = self.client:patches(self.cursor, sessionId)
		self.applying = true
		local applier = self.apply.create(self.tree, self:_echo())
		local nextCursor = batch.cursor
		local rejected = false
		for _, patch in batch.patches do
			if not self:_isCurrent(session) then
				break
			end
			local ok, err = pcall(function()
				applier:apply(patch)
			end)
			local source = "patch:" .. patch.path
			if ok then
				if not table.find(self.pendingAcks, patch.seq) then
					table.insert(self.pendingAcks, patch.seq)
				end
				self:_clearWarning(source)
				if patch.kind == "Conflict" then
					self:_setConflict(patch.path)
				else
					self:_clearConflict(patch.path)
				end
			else
				self:_setWarning(source, string.format("patch rejected: %s: %s", patch.path, tostring(err)))
				if not rejected then
					rejected = true
					nextCursor = patch.seq
				end
			end
		end
		self.applying = false
		if not self:_isCurrent(session) then
			return
		end
		if #batch.patches > 0 then
			self:_adoptPlacement()
		end
		self.cursor = nextCursor
		self.backoff = BACKOFF_START
		if not rejected then
			self.rejectedBackoff = REJECTED_PATCH_START
		end
		if #self.pendingAcks > 0 then
			self.pendingAcks = self:_ack(session, self.pendingAcks, sessionId)
			if not self:_isCurrent(session) then
				return
			end
		end
		if rejected then
			task.wait(self:_takeRejectedDelay())
		end
	end
end

function Connection._takeRejectedDelay(self: any): number
	local delay = self.rejectedBackoff
	self.rejectedBackoff = doubled(delay, RETRY_BACKOFF_MAX)
	return delay
end

function Connection._ack(self, session: number, seqs: { number }, sessionId: string): { number }
	local ok, err = pcall(function()
		self.client:postAck(seqs, sessionId)
	end)
	if not self:_isCurrent(session) then
		return seqs
	end
	if ok then
		self.ackBackoff = ACK_BACKOFF_START
		self:_clearWarning("ack")
		return {}
	end
	if Client.isSessionChanged(tostring(err)) then
		error(err, 0)
	end
	if not isTransientFailure(err) then
		self:_setWarning("refused:ack", "ack refused: " .. failureMessage(err))
		self.ackBackoff = ACK_BACKOFF_START
		return {}
	end
	self:_setWarning("ack", "ack failed: " .. failureMessage(err))
	local delay = self.ackBackoff
	self.ackBackoff = doubled(delay, RETRY_BACKOFF_MAX)
	task.wait(delay)
	return seqs
end

function Connection._serveBlobs(self, epoch: number, sessionId: string)
	local decodeFailures = 0
	while self:_isBlobCurrent(epoch) do
		local ok, batch = pcall(function()
			return self.client:blobs(self.blobCursor, sessionId)
		end)
		if not self:_isBlobCurrent(epoch) then
			return
		end
		if ok then
			decodeFailures = 0
			self:_applyBlobBatch(epoch, batch)
		else
			if isUndecodableFailure(batch) then
				decodeFailures += 1
			else
				decodeFailures = 0
			end
			if decodeFailures >= DECODE_FAILURE_LIMIT then
				self:_setWarning(
					"blob",
					string.format("terrain blob batch at cursor %d keeps failing to decode, still retrying", self.blobCursor)
				)
			else
				self:_setWarning("blob", "terrain blob poll failed: " .. failureMessage(batch))
			end
			task.wait(self.blobBackoff)
			self.blobBackoff = Connection.nextBlobBackoff(self.blobBackoff)
		end
	end
end

function Connection._applyBlobBatch(self, epoch: number, batch: any)
	local allApplied = true
	local rejection = ""
	for _, patch in batch.patches do
		if not self:_isBlobCurrent(epoch) then
			return
		end
		if patch.kind == "Conflict" then
			self:_setConflict(patch.path)
		elseif patch.direction == "ToStudio" and patch.content ~= nil and patch.path ~= TERRAIN_PATH then
			self:_setWarning("blob:" .. patch.path, "terrain blob for " .. patch.path .. " skipped, only " .. TERRAIN_PATH .. " is synced")
		elseif patch.direction == "ToStudio" and patch.content ~= nil then
			local applied, err = pcall(function()
				self:_applyBlob(patch.content)
			end)
			if applied then
				self:_clearConflict(patch.path)
			else
				allApplied = false
				rejection = tostring(err)
			end
		end
	end
	if not self:_isBlobCurrent(epoch) then
		return
	end
	if allApplied then
		self:_clearWarning("blob")
		self.blobCursor = batch.cursor
		self.blobBackoff = BLOB_BACKOFF_START
	else
		self:_setWarning("blob", "terrain blob rejected: " .. rejection)
		task.wait(self.blobBackoff)
		self.blobBackoff = Connection.nextBlobBackoff(self.blobBackoff)
	end
end

function Connection._terrainRegion(self): any
	return self.terrain.MaxExtents:ExpandToGrid(TERRAIN_RESOLUTION)
end

function Connection._applyBlob(self, blob: string)
	local region = self:_terrainRegion()
	self.terrainBlob.write(self.terrain, region, blob)
	self.terrainBaseline = self.terrainBlob.read(self.terrain, region, TERRAIN_RESOLUTION)
end

function Connection._pushTerrain(self, epoch: number)
	while self:_isBlobCurrent(epoch) do
		local ok, posted = pcall(self._sendTerrain, self, epoch)
		if not self:_isBlobCurrent(epoch) then
			return
		end
		if not ok then
			self:_setWarning("terrain", "terrain push failed: " .. tostring(posted))
			task.wait(self.terrainBackoff)
			self.terrainBackoff = math.min(self.terrainBackoff * 2, TERRAIN_BACKOFF_MAX)
		elseif not posted then
			self:_setWarning("terrain", TERRAIN_TOO_LARGE)
			return
		else
			self:_clearWarning("terrain")
			self.terrainBackoff = TERRAIN_BACKOFF_START
			return
		end
	end
end

function Connection._sendTerrain(self, epoch: number): boolean
	local region = self:_terrainRegion()
	local size = region.Size
	if self.terrainBlob.voxelCount(size.X, size.Y, size.Z, TERRAIN_RESOLUTION) > self.terrainBlob.MAX_VOXELS then
		return false
	end
	while self.terrainInFlight do
		task.wait()
	end
	if not self:_isBlobCurrent(epoch) then
		return true
	end
	local blob = self.terrainBlob.read(self.terrain, region, TERRAIN_RESOLUTION)
	if not self:_isBlobCurrent(epoch) then
		return true
	end
	if blob == self.terrainBaseline then
		return true
	end
	self.terrainInFlight = true
	local ok, err = pcall(function()
		self.client:postBlobs({ { path = TERRAIN_PATH, class = "Terrain", content = blob } })
	end)
	self.terrainInFlight = false
	if not ok then
		error(err, 0)
	end
	self.terrainBaseline = blob
	return true
end

function Connection._onWaypoint(self)
	if not self.running or not self.terrainSync then
		return
	end
	self.terrainTicket += 1
	local ticket = self.terrainTicket
	local epoch = self.epoch
	self.delay(TERRAIN_DEBOUNCE, function()
		if ticket ~= self.terrainTicket or not self:_isBlobCurrent(epoch) or not self.terrainSync then
			return
		end
		self:_pushTerrain(epoch)
	end)
end

function Connection._revertGroup(self, group: any)
	if group.script ~= nil and self.placed[group.script] == group.to then
		self.placed[group.script] = group.from
	end
end

function Connection._revertPending(self)
	for _, change in self.pendingPush do
		local group = self.groupOf[change]
		if group ~= nil then
			self:_revertGroup(group)
		end
	end
end

function Connection._queue(self, path: string, change: any, group: any?)
	self.pendingPush[path] = change
	if group ~= nil then
		self.groupOf[change] = group
	end
end

function Connection._queueMove(self, script: Instance, from: string, to: string)
	local group = { script = script, from = from, to = to }
	self.lastApplied[from] = nil
	self:_queue(from, self.protocol.delete(from), group)
	self:_queue(to, self.protocol.upsert(to, script.ClassName, (script :: any).Source), group)
	self.placed[script] = to
end

function Connection._pushInstance(self, instance: Instance, path: string)
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
	self.placed[instance] = path
	self.pendingPush[path] = self.protocol.upsert(path, instance.ClassName, source)
	self:_schedulePush()
end

function Connection._onSourceChanged(self, instance: Instance)
	local path = self:_pathOf(instance)
	if path == nil then
		return
	end
	self:_pushInstance(instance, path)
end

function Connection._relocate(self, script: Instance)
	local from = self.placed[script]
	local to = self:_pathOf(script)
	if to == nil or to == from then
		return
	end
	if from == nil then
		self:_pushInstance(script, to)
		return
	end
	self:_queueMove(script, from, to)
	self:_schedulePush()
end

function Connection._relocateSubtree(self, instance: Instance)
	if CLASS_SUFFIX[instance.ClassName] ~= nil then
		self:_relocate(instance)
	end
	for _, descendant in instance:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			self:_relocate(descendant)
		end
	end
end

function Connection._reconcilePlacement(self)
	for _, descendant in self.root:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			local from = self.placed[descendant]
			local to = self:_pathOf(descendant)
			if to ~= nil then
				if from == nil then
					self.placed[descendant] = to
				elseif to ~= from then
					self:_queueMove(descendant, from, to)
				end
			end
		end
	end
end

function Connection._adoptPlacement(self)
	for _, descendant in self.root:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			local path = self:_pathOf(descendant)
			if path ~= nil then
				self.placed[descendant] = path
			end
		end
	end
end

function Connection._removeScript(self, script: Instance)
	local from = self.placed[script] or self:_pathOf(script)
	if from == nil then
		return
	end
	local group = { script = script, from = from, previous = self.pendingPush[from] }
	self.lastApplied[from] = nil
	self:_queue(from, self.protocol.delete(from), group)
	self.moves[script] = group
	task.defer(function()
		if self.moves[script] == group then
			self.moves[script] = nil
			self.placed[script] = nil
		end
	end)
	self:_schedulePush()
end

function Connection._finishMove(self, script: Instance, group: any)
	local to = self:_pathOf(script)
	if to == nil then
		self.pendingPush[group.from] = group.previous
		self.placed[script] = group.from
		return
	end
	group.to = to
	self:_queue(to, self.protocol.upsert(to, script.ClassName, (script :: any).Source), group)
	self.placed[script] = to
	self:_schedulePush()
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

	local function reshape(parent: Instance, hadChildren: boolean, hasChildren: boolean)
		if self.applying then
			return
		end
		local previous = self:_shapePath(parent, hadChildren)
		local current = self:_shapePath(parent, hasChildren)
		if previous == current then
			return
		end
		if previous ~= nil and current ~= nil then
			self:_queueMove(parent, previous, current)
		elseif previous ~= nil then
			self.lastApplied[previous] = nil
			self:_queue(previous, self.protocol.delete(previous))
			self.placed[parent] = nil
		elseif current ~= nil then
			self:_pushInstance(parent, current)
		end
		self:_schedulePush()
	end

	local function reshapeAfterRemoval(parent: Instance)
		task.defer(function()
			if self.studioConnections ~= connections then
				return
			end
			reshape(parent, true, #parent:GetChildren() > 0)
		end)
	end

	local function track(instance: Instance)
		listen(instance:GetPropertyChangedSignal("Name"), function()
			if not self.applying then
				self:_relocateSubtree(instance)
			end
		end)
		if CLASS_SUFFIX[instance.ClassName] ~= nil then
			listen(instance:GetPropertyChangedSignal("Source"), function()
				self:_onSourceChanged(instance)
			end)
		end
	end

	for _, descendant in self.root:GetDescendants() do
		track(descendant)
	end
	self:_reconcilePlacement()
	listen(self.root.DescendantAdded, function(descendant: Instance)
		track(descendant)
		if not self.applying then
			local group = self.moves[descendant]
			if group ~= nil then
				self.moves[descendant] = nil
				self:_finishMove(descendant, group)
			else
				self:_onSourceChanged(descendant)
			end
		end
		local parent = descendant.Parent
		if parent ~= nil then
			reshape(parent, #parent:GetChildren() > 1, true)
		end
	end)
	listen(self.root.DescendantRemoving, function(descendant: Instance)
		if self.applying then
			return
		end
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			self:_removeScript(descendant)
		end
		local parent = descendant.Parent
		if parent ~= nil then
			reshapeAfterRemoval(parent)
		end
	end)
	if self.history ~= nil then
		listen(self.history.OnWaypointAdded, function()
			self:_onWaypoint()
		end)
	end
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
		self:_flushPush(session)
	end)
end

function Connection._takePending(self, remaining: { [string]: any }): { [string]: any }
	for path, change in self.pendingPush do
		remaining[path] = change
	end
	self.pendingPush = {}
	return remaining
end

function Connection._restorePending(self, remaining: { [string]: any })
	for path, change in remaining do
		if self.pendingPush[path] == nil then
			self.pendingPush[path] = change
		end
	end
end

function Connection._abandonFlight(self, token: number, session: number)
	local gate = self.pushGate
	local flight = gate.flight
	if flight == nil or flight.token ~= token then
		return
	end
	gate.flight = nil
	if not self:_isCurrent(session) then
		return
	end
	self.pushAbandons += 1
	if self.pushAbandons >= PUSH_ABANDON_LIMIT then
		self:_setWarning("push", "push held: the daemon is not answering, the batch is sent on the next edit or reconnect")
		self:_restorePending(flight.batch)
		return
	end
	self:_setWarning("push", string.format("push abandoned: no reply for %d s, the batch is sent again", PUSH_ABANDON_AFTER))
	self:_restorePending(flight.batch)
	self:_schedulePush()
end

function Connection._flushPush(self, session: number)
	if self.pushGate.flight ~= nil then
		self:_schedulePush(PUSH_DEBOUNCE)
		return
	end
	local remaining = self:_takePending({})
	while next(remaining) ~= nil do
		local ok, err, owns = self:_postChanges(remaining)
		if not owns or not self:_isCurrent(session) then
			return
		end
		if ok then
			self.pushBackoff = PUSH_RETRY
			self.pushServerErrors = 0
			self:_clearWarning("push")
			for path in remaining do
				self:_clearRefused(path)
			end
			if next(self.pendingPush) ~= nil then
				self:_schedulePush()
			end
			return
		end
		if isTransientFailure(err) then
			self:_retryPush(remaining, err)
			return
		end
		if next(namedInBatch(remaining, err)) == nil and countKeys(remaining) > 1 then
			if not self:_postEach(session, remaining) then
				return
			end
		else
			remaining = self:_dropRefused(remaining, err)
		end
		remaining = self:_takePending(remaining)
	end
end

function Connection._retryPush(self, remaining: { [string]: any }, err: any)
	if isServerError(err) then
		self.pushServerErrors += 1
	end
	if self.pushServerErrors >= SERVER_ERROR_LIMIT then
		self.pushServerErrors = 0
		self.pushBackoff = PUSH_RETRY
		self:_clearWarning("push")
		for path in remaining do
			self:_setWarning("refused:" .. path, "not synced, the daemon kept failing on " .. path .. ": " .. failureMessage(err))
		end
		if next(self.pendingPush) ~= nil then
			self:_schedulePush()
		end
		return
	end
	self:_setWarning("push", "push failed: " .. failureMessage(err))
	self:_restorePending(remaining)
	local delay = self.pushBackoff
	self.pushBackoff = doubled(delay, RETRY_BACKOFF_MAX)
	self:_schedulePush(delay)
end

function Connection._clearRefused(self, path: string)
	self:_clearWarning("refused:" .. path)
end

function Connection._postChanges(self, batch: { [string]: any }): (boolean, any, boolean)
	local gate = self.pushGate
	gate.issued += 1
	local token = gate.issued
	local session = self.session
	gate.flight = { token = token, batch = batch }
	self.delay(PUSH_ABANDON_AFTER, function()
		self:_abandonFlight(token, session)
	end)
	local changes = {}
	for _, change in batch do
		table.insert(changes, change)
	end
	local ok, err = pcall(function()
		self.client:postChanges(changes)
	end)
	if ok then
		self.pushAbandons = 0
	end
	local owns = gate.flight ~= nil and gate.flight.token == token
	if owns then
		gate.flight = nil
	end
	return ok, err, owns
end

function Connection._units(self, remaining: { [string]: any }): { { [string]: any } }
	local units = {}
	local unitOf = weakMap()
	for _, path in Connection._sortedKeys(remaining) do
		local change = remaining[path]
		local group = self.groupOf[change]
		local unit = if group ~= nil then unitOf[group] else nil
		if unit == nil then
			unit = {}
			table.insert(units, unit)
			if group ~= nil then
				unitOf[group] = unit
			end
		end
		unit[path] = change
	end
	return units
end

function Connection._postEach(self, session: number, remaining: { [string]: any }): boolean
	for _, unit in self:_units(remaining) do
		local ok, err, owns = self:_postChanges(unit)
		if not owns or not self:_isCurrent(session) then
			return false
		end
		if ok then
			for path in unit do
				remaining[path] = nil
				self:_clearRefused(path)
			end
			self.pushServerErrors = 0
			self:_clearWarning("push")
		elseif isTransientFailure(err) then
			self:_retryPush(remaining, err)
			return false
		else
			for path in unit do
				remaining[path] = nil
			end
			self:_dropRefused(unit, err)
		end
	end
	return true
end

function Connection._withGroupMates(self, remaining: { [string]: any }, paths: { [string]: true }): { [string]: true }
	local expanded = table.clone(paths)
	local groups = weakMap()
	for path in paths do
		local group = self.groupOf[remaining[path]]
		if group ~= nil then
			groups[group] = true
		end
	end
	for path, change in remaining do
		local group = self.groupOf[change]
		if group ~= nil and groups[group] then
			expanded[path] = true
		end
	end
	return expanded
end

function Connection._dropRefused(self, remaining: { [string]: any }, err: any): { [string]: any }
	local refused = namedInBatch(remaining, err)
	if next(refused) == nil then
		for path in remaining do
			refused[path] = true
		end
	end
	local dropped = self:_withGroupMates(remaining, refused)
	local kept = {}
	for path, change in remaining do
		if dropped[path] == nil then
			kept[path] = change
		else
			local group = self.groupOf[change]
			if group ~= nil then
				self:_revertGroup(group)
			end
		end
	end
	for path in refused do
		self:_setWarning("refused:" .. path, "not synced, the daemon refused " .. path .. ": " .. failureMessage(err))
	end
	return kept
end

return Connection
