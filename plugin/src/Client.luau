
local MessagePack = require(if script ~= nil then script.Parent.MessagePack else "./MessagePack")
local Protocol = require(if script ~= nil then script.Parent.Protocol else "./Protocol").new(MessagePack)
local Hmac = require(if script ~= nil then script.Parent.Hmac else "./Hmac")

local Client = {}
Client.__index = Client

Client.OUTDATED_PLUGIN = "naht: this plugin is outdated, update the naht plugin to match the daemon"
Client.SESSION_CHANGED = "naht: the daemon restarted, re-syncing from its new session"
Client.TOKEN_REQUIRED = "the daemon needs the project token: enter the token printed by `naht token`"
Client.TOKEN_REJECTED = "the daemon rejected the token: enter the token printed by `naht token`"
Client.PROOF_FAILED =
	"the program on the daemon port could not prove it holds this project's token, so the token was not sent: if `naht token` prints a new token, enter it, otherwise stop whatever else listens on the port"
Client.UNVERIFIED = "naht: request held back: the daemon has not proven it holds the token yet"
Client.HANDSHAKE_CHANGED = "naht: the daemon changed during the handshake, retrying"

Client.UNPORTED_URL = "naht: the daemon URL must name its port"
Client.INFO_BODY_LIMIT = 1048576

local TOKEN_REQUIRED_BODY = "token required"

export type TokenStore = {
	load: (projectId: string) -> string?,
	save: (projectId: string, token: string) -> (),
}

export type Client = typeof(setmetatable(
	{} :: {
		baseUrl: string,
		port: number,
		readPlaceId: () -> number,
		http: any,
		token: string?,
		verified: boolean,
		projectId: string?,
		store: TokenStore?,
		unsaved: boolean,
	},
	Client
))

local function portOf(baseUrl: string): number
	local authority = string.match(baseUrl, "^%a[%w+.-]*://([^/?#]*)")
	local port = if authority ~= nil then tonumber(string.match(authority, ":(%d+)$")) else nil
	if port == nil then
		error(Client.UNPORTED_URL .. ": " .. baseUrl, 0)
	end
	return port
end

function Client.new(baseUrl: string, readPlaceId: () -> number, http: any?, store: TokenStore?): Client
	return setmetatable({
		baseUrl = baseUrl,
		port = portOf(baseUrl),
		verified = false,
		unsaved = false,
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

local function request(self: Client, options: { [string]: any }, withholdToken: boolean?): { [string]: any }
	options.Url = self.baseUrl .. options.Url
	local headers = options.Headers or {}
	headers["x-naht-client"] = "plugin"
	headers["x-naht-place-id"] = tostring(self.readPlaceId())
	if not withholdToken then
		local token = self.token
		if not self.verified or token == nil then
			error(Client.UNVERIFIED, 0)
		end
		headers[Protocol.TOKEN_HEADER] = token
	end
	options.Headers = headers
	local ok, response = pcall(function()
		return self.http:RequestAsync(options)
	end)
	if not ok then
		self.verified = false
		error("naht: request failed: " .. tostring(response))
	end
	if response.StatusCode == 401 then
		error(tokenRefusal(response.Body))
	end
	return response
end

function Client.setToken(self: Client, token: string)
	self.token = token
	self.verified = false
	self.unsaved = true
end

local function saveProvenToken(self: Client, projectId: string?)
	local store, token = self.store, self.token
	if not self.unsaved or store == nil or projectId == nil or token == nil then
		return
	end
	store.save(projectId, token)
	self.unsaved = false
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

local function newNonce(self: Client): string
	return string.lower((string.gsub(self.http:GenerateGUID(false), "[^%w]", "")))
end

local function fetchInfo(self: Client, nonce: string, withholdToken: boolean): any
	local response = request(self, {
		Url = "/info?nonce=" .. nonce,
		Method = "GET",
	}, withholdToken)
	if response.StatusCode == 403 then
		error(refusal(response.Body))
	end
	if not response.Success then
		error("naht: /info returned " .. tostring(response.StatusCode))
	end
	if #response.Body > Client.INFO_BODY_LIMIT then
		error("naht: /info answered more than " .. tostring(Client.INFO_BODY_LIMIT) .. " bytes")
	end
	return Protocol.assertCompatible(Protocol.decodeInfo(response.Body))
end

local function proves(self: Client, info: any, nonce: string): boolean
	local token = self.token
	return token ~= nil and info.proof == Hmac.proof(token, self.port, nonce)
end

local function handshakeRefusal(message: string): { status: number, message: string, tokenRefused: boolean }
	return { status = 401, message = message, tokenRefused = true }
end

function Client.info(self: Client): any
	self.verified = false
	local nonce = newNonce(self)
	local info = fetchInfo(self, nonce, true)
	local switched = self.projectId ~= info.project_id
	self.projectId = info.project_id
	if self.store ~= nil and (self.token == nil or switched) then
		local stored = self.store.load(info.project_id)
		if stored ~= nil or not self.unsaved then
			self.token = stored
			self.unsaved = false
		end
	end
	if self.token == nil then
		error(handshakeRefusal(Client.TOKEN_REQUIRED))
	end
	if not proves(self, info, nonce) then
		error(handshakeRefusal(Client.PROOF_FAILED))
	end
	self.verified = true
	local confirmNonce = newNonce(self)
	local confirmed = fetchInfo(self, confirmNonce, false)
	if confirmed.project_id ~= info.project_id then
		self.verified = false
		error(Client.HANDSHAKE_CHANGED, 0)
	end
	if not proves(self, confirmed, confirmNonce) then
		self.verified = false
		error(handshakeRefusal(Client.PROOF_FAILED))
	end
	saveProvenToken(self, info.project_id)
	return confirmed
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
