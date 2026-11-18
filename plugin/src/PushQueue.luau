--!strict

local Support = require(if script ~= nil then script.Parent.Support else "./Support")

local PushQueue = {}

local doubled = Support.doubled
local isTransientFailure = Support.isTransientFailure
local failureMessage = Support.failureMessage
local isServerError = Support.isServerError
local sortedKeys = Support.sortedKeys
local weakMap = Support.weakMap
local RETRY_BACKOFF_MAX = Support.RETRY_BACKOFF_MAX

local PUSH_DEBOUNCE = 0.3
local PUSH_RETRY = 2
local SERVER_ERROR_LIMIT = 3
local PUSH_ABANDON_AFTER = 30
local PUSH_ABANDON_LIMIT = 3

PushQueue.PUSH_RETRY = PUSH_RETRY

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

function PushQueue._schedulePush(self, delay: number?)
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

function PushQueue._takePending(self, remaining: { [string]: any }): { [string]: any }
	for path, change in self.pendingPush do
		remaining[path] = change
	end
	self.pendingPush = {}
	return remaining
end

function PushQueue._restorePending(self, remaining: { [string]: any })
	for path, change in remaining do
		if self.pendingPush[path] == nil then
			self.pendingPush[path] = change
		end
	end
end

function PushQueue._abandonFlight(self, token: number, session: number)
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

function PushQueue._flushPush(self, session: number)
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
		if not ok and self:_refuseToken(err) then
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

function PushQueue._retryPush(self, remaining: { [string]: any }, err: any)
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

function PushQueue._clearRefused(self, path: string)
	self:_clearWarning("refused:" .. path)
end

function PushQueue._postChanges(self, batch: { [string]: any }): (boolean, any, boolean)
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

function PushQueue._units(self, remaining: { [string]: any }): { { [string]: any } }
	local units = {}
	local unitOf = weakMap()
	for _, path in sortedKeys(remaining) do
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

function PushQueue._postEach(self, session: number, remaining: { [string]: any }): boolean
	for _, unit in self:_units(remaining) do
		local ok, err, owns = self:_postChanges(unit)
		if not owns or not self:_isCurrent(session) then
			return false
		end
		if not ok and self:_refuseToken(err) then
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

function PushQueue._withGroupMates(self, remaining: { [string]: any }, paths: { [string]: true }): { [string]: true }
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

function PushQueue._dropRefused(self, remaining: { [string]: any }, err: any): { [string]: any }
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

return PushQueue
