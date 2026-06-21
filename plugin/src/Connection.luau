--!strict
-- The connection loop: handshake, long-poll + apply, Studio → filesystem change push, and
-- reconnect-with-backoff. State changes are reported through `onState` so the UI stays a dumb view.
--
-- No sync logic lives here — the daemon decides everything. The plugin only carries patches in,
-- carries edits out, and reports connection state. On every (re)connect it resets its cursor to 0 so
-- the daemon's reconnect re-diff drives a clean catch-up rather than a blind replay.

local Apply = require(script.Parent.Apply)
local Protocol = require(script.Parent.Protocol)

local Connection = {}
Connection.__index = Connection

local BACKOFF_START = 1
local BACKOFF_MAX = 15
local PUSH_DEBOUNCE = 0.3

local CLASS_SUFFIX = {
	Script = ".server.luau",
	LocalScript = ".client.luau",
	ModuleScript = ".luau",
}

-- The project-relative path for an instance under `root`, mirroring the daemon's file conventions.
local function pathOf(root: Instance, instance: Instance): string?
	local suffix = CLASS_SUFFIX[instance.ClassName]
	if suffix == nil then
		return nil
	end
	local segments = { instance.Name .. suffix }
	local current = instance.Parent
	while current ~= nil and current ~= root do
		table.insert(segments, 1, current.Name)
		current = current.Parent
	end
	if current ~= root then
		return nil -- not under the project root
	end
	return table.concat(segments, "/")
end

export type State = "Connecting" | "Connected" | "Reconnecting" | "Conflict" | "Disconnected"

function Connection.new(
	client: any,
	root: Instance,
	placeId: number,
	onState: (State, string?) -> ()
)
	return setmetatable({
		client = client,
		root = root,
		placeId = placeId,
		onState = onState,
		running = false,
		cursor = 0,
		applying = false,
		-- The content last written by an apply, per path. A Studio change signal whose value matches
		-- is the echo of our own apply (which may fire deferred, after `applying` clears) and is
		-- dropped instead of pushed back.
		lastApplied = {},
		pendingPush = {},
		pushScheduled = false,
	}, Connection)
end

function Connection.start(self)
	if self.running then
		return
	end
	self.running = true
	task.spawn(function()
		self:_run()
	end)
end

function Connection.stop(self)
	self.running = false
	self.onState("Disconnected", nil)
end

-- The outer loop: handshake, serve patches, and on any failure back off and retry.
function Connection._run(self)
	local backoff = BACKOFF_START
	self:_watchStudio()
	while self.running do
		self.onState("Connecting", nil)
		local ok, err = pcall(function()
			local info = self.client:info(self.placeId)
			self.onState("Connected", info.project_name)
			backoff = BACKOFF_START
			self.cursor = 0 -- let the daemon's re-diff drive the catch-up
			self:_serve()
		end)
		if not self.running then
			break
		end
		if not ok then
			self.onState("Reconnecting", tostring(err))
			task.wait(backoff)
			backoff = math.min(backoff * 2, BACKOFF_MAX)
		end
	end
end

-- The inner long-poll loop. Returns (raising) when the connection breaks, so `_run` can back off.
function Connection._serve(self)
	while self.running do
		local batch = self.client:patches(self.cursor)
		self.applying = true
		local applier = Apply.create(Apply.studioTree(self.root))
		for _, patch in batch.patches do
			if patch.kind ~= "Delete" then
				self.lastApplied[patch.path] = patch.content
			end
			applier:apply(patch)
			if patch.kind == "Conflict" then
				self.onState("Conflict", patch.path)
			end
		end
		self.applying = false
		self.cursor = batch.cursor
	end
end

-- Watch the project subtree for Studio-side edits and push them, debounced. Edits the plugin itself
-- just applied are ignored (the `applying` guard) so a patch does not echo back as a change.
function Connection._watchStudio(self)
	local function onChanged(instance: Instance)
		local path = pathOf(self.root, instance)
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
			instance:GetPropertyChangedSignal("Source"):Connect(function()
				onChanged(instance)
			end)
		end
	end

	for _, descendant in self.root:GetDescendants() do
		track(descendant)
	end
	self.root.DescendantAdded:Connect(function(descendant)
		track(descendant)
		onChanged(descendant)
	end)
	self.root.DescendantRemoving:Connect(function(descendant)
		if self.applying then
			return
		end
		local path = pathOf(self.root, descendant)
		if path ~= nil then
			self.pendingPush[path] = Protocol.delete(path)
			self:_schedulePush()
		end
	end)
end

function Connection._schedulePush(self)
	if self.pushScheduled then
		return
	end
	self.pushScheduled = true
	task.delay(PUSH_DEBOUNCE, function()
		self.pushScheduled = false
		local changes = {}
		for _, change in self.pendingPush do
			table.insert(changes, change)
		end
		self.pendingPush = {}
		if #changes > 0 then
			pcall(function()
				self.client:postChanges(changes)
			end)
		end
	end)
end

return Connection
