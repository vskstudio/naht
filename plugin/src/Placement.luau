--!strict

local Support = require(if script ~= nil then script.Parent.Support else "./Support")
local Fingerprint = require(if script ~= nil then script.Parent.Fingerprint else "./Fingerprint")

local Placement = {}

local CLASS_SUFFIX = Support.CLASS_SUFFIX

local UNSAFE_NAME_CHARACTERS = { "/", "\\", ":", "<", ">", "\"", "|", "?", "*" }
for code = 0, 31 do
	table.insert(UNSAFE_NAME_CHARACTERS, string.char(code))
end
local UNSAFE_NAMES = { [""] = true, ["."] = true, [".."] = true }
local SCRIPT_SUFFIXES = { ".server", ".client", ".luau" }
local INIT_SUFFIXES = { "/init.luau", "/init.server.luau", "/init.client.luau" }

local function isInitPath(path: string): boolean
	for _, suffix in INIT_SUFFIXES do
		if string.sub(path, -#suffix) == suffix then
			return true
		end
	end
	return false
end

local function isSafeName(name: string): boolean
	if UNSAFE_NAMES[name] or string.sub(name, 1, 1) == "." then
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

local WINDOWS_DEVICE_NAMES: { [string]: boolean } = {}
for _, name in { "CON", "CONIN$", "CONOUT$", "PRN", "AUX", "NUL", "CLOCK$", "COM\u{b9}", "COM\u{b2}", "COM\u{b3}", "LPT\u{b9}", "LPT\u{b2}", "LPT\u{b3}" } do
	WINDOWS_DEVICE_NAMES[name] = true
end
for digit = 0, 9 do
	WINDOWS_DEVICE_NAMES["COM" .. digit] = true
	WINDOWS_DEVICE_NAMES["LPT" .. digit] = true
end

local function isWindowsDeviceName(name: string): boolean
	local stem = string.match(name, "^[^.]*") or name
	local trimmed = (string.gsub(stem, " +$", ""))
	return WINDOWS_DEVICE_NAMES[string.upper(trimmed)] == true
end

function Placement._unsafeReason(_self, instance: Instance, segments: { string }): string?
	for _, segment in segments do
		if not isSafeName(segment) then
			return "a name contains '/', '\\', ':', '< > \" | ? *' or a control byte, starts with '.', or is empty, '.' or '..'"
		end
		if isWindowsDeviceName(segment) then
			return "a name is a Windows device name such as CON or LPT1"
		end
	end
	if instance.ClassName == "ModuleScript" and endsWithScriptSuffix(instance.Name) then
		return "a ModuleScript name ends in .server, .client or .luau"
	end
	return nil
end

function Placement._pathOf(self, instance: Instance): string?
	return self:_shapePath(instance, #instance:GetChildren() > 0 or self:_isInitPlaced(instance))
end

function Placement._isInitPlaced(self, instance: Instance): boolean
	local previous = self.placed[instance]
	return previous ~= nil and isInitPath(previous)
end

function Placement._recordApplied(self, path: string, node: Instance)
	self.placed[node] = path
end

function Placement._shapePath(self, instance: Instance, hasChildren: boolean): string?
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

function Placement._revertGroup(self, group: any)
	if group.script ~= nil and self.placed[group.script] == group.to then
		self.placed[group.script] = group.from
	end
end

function Placement._revertPending(self)
	for _, change in self.pendingPush do
		local group = self.groupOf[change]
		if group ~= nil then
			self:_revertGroup(group)
		end
	end
end

function Placement._queue(self, path: string, change: any, group: any?)
	self.pendingPush[path] = change
	if group ~= nil then
		self.groupOf[change] = group
	end
end

function Placement._indexHolders(self): { [string]: { Instance } }
	local holders: { [string]: { Instance } } = {}
	for _, descendant in self.root:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			local path = self:_pathOf(descendant)
			if path ~= nil then
				local list = holders[path]
				if list == nil then
					list = {}
					holders[path] = list
				end
				table.insert(list, descendant)
			end
		end
	end
	return holders
end

function Placement._heldByOther(self, script: Instance, path: string): boolean
	for _, holder in (self.holders or self:_indexHolders())[path] or {} do
		if holder ~= script then
			return true
		end
	end
	return false
end

function Placement._refuseHeld(self, path: string, message: string)
	local key = "held:" .. path
	if not self.skippedPaths[key] then
		self.skippedPaths[key] = true
		self.warn(message)
	end
end

function Placement._queueMove(self, script: Instance, from: string, to: string)
	if self:_heldByOther(script, to) then
		self:_refuseHeld(to, string.format("Naht: not moving %q to %q, another script already holds that path", from, to))
		return
	end
	local pendingGroup = self.groupOf[self.pendingPush[from]]
	if pendingGroup ~= nil and pendingGroup.to == from and pendingGroup.script == script then
		self.pendingPush[from] = nil
		pendingGroup.to = to
		self.lastApplied[from] = nil
		self:_queue(to, self.protocol.upsert(to, script.ClassName, (script :: any).Source), pendingGroup)
		self.placed[script] = to
		return
	end
	local group = { script = script, from = from, to = to }
	self.lastApplied[from] = nil
	self:_queue(from, self.protocol.delete(from), group)
	self:_queue(to, self.protocol.upsert(to, script.ClassName, (script :: any).Source), group)
	self.placed[script] = to
end

function Placement._pushInstance(self, instance: Instance, path: string)
	local source = (instance :: any).Source
	local expected = self.lastApplied[path]
	if expected ~= nil and expected == source then
		self.lastApplied[path] = nil
		return
	end
	if self.applying then
		return
	end
	if self.placed[instance] ~= path and self:_heldByOther(instance, path) then
		self:_refuseHeld(path, string.format("Naht: not syncing %q, another script already holds that path", path))
		return
	end
	self.lastApplied[path] = nil
	self.placed[instance] = path
	self:_queue(path, self.protocol.upsert(path, instance.ClassName, source), self.groupOf[self.pendingPush[path]])
	self:_schedulePush()
end

function Placement._onSourceChanged(self, instance: Instance)
	local path = self:_pathOf(instance)
	if path == nil then
		return
	end
	self:_pushInstance(instance, path)
end

function Placement._relocate(self, script: Instance)
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

function Placement._relocateSubtree(self, instance: Instance)
	self.holders = self:_indexHolders()
	if CLASS_SUFFIX[instance.ClassName] ~= nil then
		self:_relocate(instance)
	end
	for _, descendant in instance:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			self:_relocate(descendant)
		end
	end
	self.holders = nil
end

function Placement._reconcilePlacement(self)
	self.holders = self:_indexHolders()
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
	self.holders = nil
end

function Placement._adoptPlacement(self)
	for _, descendant in self.root:GetDescendants() do
		if CLASS_SUFFIX[descendant.ClassName] ~= nil then
			local path = self:_pathOf(descendant)
			if path ~= nil then
				self.placed[descendant] = path
			end
		end
	end
end

local function isUnder(root: Instance, instance: Instance): boolean
	local current: Instance? = instance
	while current ~= nil do
		if current == root then
			return true
		end
		current = current.Parent
	end
	return false
end

function Placement._inventory(self)
	local placedScripts = {}
	local claims: { [string]: number } = {}
	for script, path in self.placed do
		if CLASS_SUFFIX[script.ClassName] ~= nil and isUnder(self.root, script) then
			claims[path] = (claims[path] or 0) + 1
			table.insert(placedScripts, { script = script, path = path })
		end
	end
	local scripts = {}
	for _, entry in placedScripts do
		if claims[entry.path] == 1 then
			table.insert(scripts, {
				path = entry.path,
				class = entry.script.ClassName,
				hash = Fingerprint.source((entry.script :: any).Source),
			})
		end
	end
	return scripts
end

function Placement._postInventory(self, sessionId: string)
	if self.inventorySession == sessionId then
		return
	end
	local scripts = self:_inventory()
	if #scripts == 0 then
		self.inventorySession = sessionId
		self:_clearWarning("inventory")
		return
	end
	local ok, posted = pcall(self.client.postInventory, self.client, scripts, sessionId)
	if not ok then
		if Support.isTransientFailure(posted) or Support.isTokenRefusal(posted) then
			error(posted, 0)
		end
		self.inventorySession = sessionId
		self:_setWarning(
			"inventory",
			"naht: the daemon refused the inventory, renames made while the plugin was stopped were not applied: "
				.. Support.failureMessage(posted)
		)
		return
	end
	if posted == nil then
		self:_setWarning("inventory", "naht: this daemon predates renames made while the plugin was stopped, update naht to apply them")
		return
	end
	self.inventorySession = sessionId
	self:_clearWarning("inventory")
	self:_queueWanted(posted)
end

function Placement._queueWanted(self, wanted: { string })
	for _, path in wanted do
		for script, placedPath in self.placed do
			if placedPath == path and CLASS_SUFFIX[script.ClassName] ~= nil and isUnder(self.root, script) then
				self:_queue(path, self.protocol.upsert(path, script.ClassName, (script :: any).Source))
				break
			end
		end
	end
	if #wanted > 0 then
		self:_schedulePush()
	end
end

function Placement._removeScript(self, script: Instance)
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

function Placement._finishMove(self, script: Instance, group: any)
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

return Placement
