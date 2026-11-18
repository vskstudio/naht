
local MessagePack = require(if script ~= nil then script.Parent.MessagePack else "./MessagePack")
local Protocol = require(if script ~= nil then script.Parent.Protocol else "./Protocol").new(MessagePack)

local Client = {}
Client.__index = Client

Client.OUTDATED_PLUGIN = "naht: this plugin is outdated, update the naht plugin to match the daemon"
Client.SESSION_CHANGED = "naht: the daemon restarted, re-syncing from its new session"
Client.TOKEN_REQUIRED = "the daemon needs the project token: enter the token printed by `naht token`"
Client.TOKEN_REJECTED = "the daemon rejected the token: enter the token printed by `naht token`"

local TOKEN_REQUIRED_BODY = "token required"

export type TokenStore = {
	load: (projectId: string) -> string?,
	save: (projectId: string, token: string) -> (),
}

export type Client = typeof(setmetatable(
	{} :: { baseUrl: string, readPlaceId: () -> number, http: any, token: string?, projectId: string?, store: TokenStore? },
	Client
))

function Client.new(baseUrl: string, readPlaceId: () -> number, http: any?, store: TokenStore?): Client
	return setmetatable({
		baseUrl = baseUrl,
		readPlaceId = readPlaceId,
		http = http or game:GetService("HttpService"),
		store = store,
	}, Client)
end

local function tokenRefusal(body: string): { status: number, message: string, tokenRefused: boolean }
	local message = if string.find(body, TOKEN_REQUIRED_BODY, 1, true) ~= nil
		then Client.TOKEN_REQUIRED
		else Client.TOKEN_REJECTED
	return { status = 401, message = message, tokenRefused = true }
end

local function request(self: Client, options: { [string]: any }): { [string]: any }
	options.Url = self.baseUrl .. options.Url
	local headers = options.Headers or {}
	headers["x-naht-client"] = "plugin"
	headers["x-naht-place-id"] = tostring(self.readPlaceId())
	if self.token ~= nil then
		headers[Protocol.TOKEN_HEADER] = self.token
	end
	options.Headers = headers
	local ok, response = pcall(function()
		return self.http:RequestAsync(options)
	end)
	if not ok then
		error("naht: request failed: " .. tostring(response))
	end
	if response.StatusCode == 401 then
		error(tokenRefusal(response.Body))
	end
	return response
end

function Client.setToken(self: Client, token: string)
	self.token = token
	if self.store ~= nil and self.projectId ~= nil then
		self.store.save(self.projectId, token)
	end
end

function Client.isOutdatedPluginError(err: string): boolean
	return string.find(err, Client.OUTDATED_PLUGIN, 1, true) ~= nil
end

function Client.isSessionChanged(err: string): boolean
	return string.find(err, Client.SESSION_CHANGED, 1, true) ~= nil
end

function Client.isTerminalError(err: string): boolean
	return Client.isOutdatedPluginError(err) or string.find(err, Protocol.INCOMPATIBLE_VERSION, 1, true) ~= nil
end

local function refusal(body: string): string
	if string.find(body, Protocol.OUTDATED_PLUGIN_TOKEN, 1, true) ~= nil then
		return Client.OUTDATED_PLUGIN
	end
	return "naht: the daemon refused this session: " .. body
end

function Client.info(self: Client): any
	local response = request(self, {
		Url = "/info",
		Method = "GET",
	})
	if response.StatusCode == 403 then
		error(refusal(response.Body))
	end
	if not response.Success then
		error("naht: /info returned " .. tostring(response.StatusCode))
	end
	local info = Protocol.assertCompatible(Protocol.decodeInfo(response.Body))
	local switched = self.projectId ~= info.project_id
	self.projectId = info.project_id
	if self.store ~= nil and (self.token == nil or switched) then
		self.token = self.store.load(info.project_id)
	end
	return info
end

local function checkPoll(response: any, route: string)
	if response.StatusCode == 409 then
		error(Client.SESSION_CHANGED)
	end
	if not response.Success then
		error("naht: " .. route .. " returned " .. tostring(response.StatusCode))
	end
end

function Client.patches(self: Client, cursor: number, sessionId: string): any
	local response = request(self, {
		Url = "/patches?cursor=" .. tostring(cursor) .. "&session=" .. sessionId,
		Method = "GET",
	})
	checkPoll(response, "/patches")
	return Protocol.decodePatches(response.Body)
end

function Client.blobs(self: Client, cursor: number, sessionId: string): any
	local response = request(self, {
		Url = "/blobs?cursor=" .. tostring(cursor) .. "&session=" .. sessionId,
		Method = "GET",
	})
	checkPoll(response, "/blobs")
	local decoded, batch = pcall(Protocol.decodeBlobs, response.Body)
	if not decoded then
		error({ undecodable = true, message = "naht: undecodable blob batch: " .. tostring(batch) })
	end
	return batch
end

function Client.postBlobs(self: Client, changes: { any })
	local response = request(self, {
		Url = "/blobs",
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeBlobs(changes),
	})
	if not response.Success then
		error("naht: /blobs returned " .. tostring(response.StatusCode))
	end
end

function Client.postAck(self: Client, seqs: { number }, sessionId: string)
	local response = request(self, {
		Url = "/ack?session=" .. sessionId,
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeAck(seqs),
	})
	if response.StatusCode == 409 then
		error(Client.SESSION_CHANGED)
	end
	if not response.Success then
		error({
			status = response.StatusCode,
			message = "naht: /ack returned " .. tostring(response.StatusCode),
		})
	end
end

local function refusedPaths(self: Client, body: string?): { string }
	local ok, decoded = pcall(function()
		return self.http:JSONDecode(body or "")
	end)
	if not ok or type(decoded) ~= "table" or type(decoded.refused_paths) ~= "table" then
		return {}
	end
	return decoded.refused_paths
end

function Client.postChanges(self: Client, changes: { Protocol.Change })
	local response = request(self, {
		Url = "/changes",
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeChanges(changes),
	})
	if not response.Success then
		error({
			status = response.StatusCode,
			message = "naht: /changes returned " .. tostring(response.StatusCode),
			refused = refusedPaths(self, response.Body),
		})
	end
end

function Client.postInventory(self: Client, scripts: { Protocol.InventoryEntry }, sessionId: string): boolean
	local response = request(self, {
		Url = "/inventory?session=" .. sessionId,
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeInventory(scripts),
	})
	if response.StatusCode == 409 then
		error(Client.SESSION_CHANGED)
	end
	if response.StatusCode == 404 then
		return false
	end
	if not response.Success then
		error({
			status = response.StatusCode,
			message = "naht: /inventory returned " .. tostring(response.StatusCode),
			refused = refusedPaths(self, response.Body),
		})
	end
	return true
end

return Client
