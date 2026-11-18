--!strict

local Client = require(if script ~= nil then script.Parent.Client else "./Client")
local Support = require(if script ~= nil then script.Parent.Support else "./Support")
local TerrainSync = require(if script ~= nil then script.Parent.TerrainSync else "./TerrainSync")
local Placement = require(if script ~= nil then script.Parent.Placement else "./Placement")
local PushQueue = require(if script ~= nil then script.Parent.PushQueue else "./PushQueue")

local Connection = {}
Connection.__index = Connection

for _, part in { TerrainSync, Placement, PushQueue } do
	for name, member in part do
		if type(member) == "function" then
			Connection[name] = member
		end
	end
end

local doubled = Support.doubled
local isTransientFailure = Support.isTransientFailure
local failureMessage = Support.failureMessage
local sortedKeys = Support.sortedKeys
local weakMap = Support.weakMap
local CLASS_SUFFIX = Support.CLASS_SUFFIX
local RETRY_BACKOFF_MAX = Support.RETRY_BACKOFF_MAX

local BACKOFF_START = 1
local BACKOFF_MAX = 15

local ACK_BACKOFF_START = 2
local REJECTED_PATCH_START = 1

local sharedPushGate: PushGate = { issued = 0, flight = nil }

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
		blobBackoff = TerrainSync.BLOB_BACKOFF_START,
		terrainBackoff = TerrainSync.TERRAIN_BACKOFF_START,
		pushBackoff = PushQueue.PUSH_RETRY,
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

function Connection._refreshState(self)
	local conflictPaths = sortedKeys(self.conflicts)
	if #conflictPaths > 0 then
		self.onState("Conflict", conflictPaths[1])
		return
	end
	local warningSources = sortedKeys(self.warnings)
	if #warningSources > 0 then
		self.onState("Warning", self.warnings[warningSources[1]])
		return
	end
	self.onState("Connected", self.projectName)
end

function Connection.start(self)
	if self.running then
		return
	end
	self.running = true
	self.session += 1
	local session = self.session
	self.inventorySession = nil
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
			if self.terrainSync then
				task.spawn(function()
					self:_pushTerrain(epoch)
				end)
				task.spawn(function()
					self:_serveBlobs(epoch, info.session_id)
				end)
			end
			self:_postInventory(info.session_id)
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

return Connection
