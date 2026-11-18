
local MessagePack = require(if script ~= nil then script.Parent.MessagePack else "./MessagePack")
local Protocol = require(if script ~= nil then script.Parent.Protocol else "./Protocol").new(MessagePack)

local Client = {}
Client.__index = Client

Client.OUTDATED_PLUGIN = "naht: this plugin is outdated, update the naht plugin to match the daemon"
Client.SESSION_CHANGED = "naht: the daemon restarted, re-syncing from its new session"

export type Client = typeof(setmetatable({} :: { baseUrl: string, placeId: number, http: any }, Client))

function Client.new(baseUrl: string, placeId: number, http: any?): Client
	return setmetatable({
		baseUrl = baseUrl,
		placeId = placeId,
		http = http or game:GetService("HttpService"),
	}, Client)
end

local function request(self: Client, options: { [string]: any }): { [string]: any }
	options.Url = self.baseUrl .. options.Url
	local headers = options.Headers or {}
	headers["x-naht-client"] = "plugin"
	headers["x-naht-place-id"] = tostring(self.placeId)
	options.Headers = headers
	local ok, response = pcall(function()
		return self.http:RequestAsync(options)
	end)
	if not ok then
		error("naht: request failed: " .. tostring(response))
	end
	return response
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
	return Protocol.assertCompatible(Protocol.decodeInfo(response.Body))
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

return Client
