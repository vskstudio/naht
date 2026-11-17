
local HttpService = game:GetService("HttpService")

local MessagePack = require(script.Parent.MessagePack)
local Protocol = require(script.Parent.Protocol).new(MessagePack)

local Client = {}
Client.__index = Client

export type Client = typeof(setmetatable({} :: { baseUrl: string }, Client))

function Client.new(baseUrl: string): Client
	return setmetatable({ baseUrl = baseUrl }, Client)
end

local function request(self: Client, options: { [string]: any }): { [string]: any }
	options.Url = self.baseUrl .. options.Url
	local ok, response = pcall(function()
		return HttpService:RequestAsync(options)
	end)
	if not ok then
		error("naht: request failed: " .. tostring(response))
	end
	return response
end

function Client.info(self: Client, placeId: number): any
	local response = request(self, {
		Url = "/info?place_id=" .. tostring(placeId),
		Method = "GET",
	})
	if response.StatusCode == 403 then
		error("naht: the daemon refused this place (servePlaceId mismatch)")
	end
	if not response.Success then
		error("naht: /info returned " .. tostring(response.StatusCode))
	end
	return Protocol.assertCompatible(Protocol.decodeInfo(response.Body))
end

function Client.patches(self: Client, cursor: number): any
	local response = request(self, {
		Url = "/patches?cursor=" .. tostring(cursor),
		Method = "GET",
	})
	if not response.Success then
		error("naht: /patches returned " .. tostring(response.StatusCode))
	end
	return Protocol.decodePatches(response.Body)
end

function Client.blobs(self: Client, cursor: number): any
	local response = request(self, {
		Url = "/blobs?cursor=" .. tostring(cursor),
		Method = "GET",
	})
	if not response.Success then
		error("naht: /blobs returned " .. tostring(response.StatusCode))
	end
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

function Client.postAck(self: Client, paths: { string })
	local response = request(self, {
		Url = "/ack",
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeAck(paths),
	})
	if not response.Success then
		error({
			status = response.StatusCode,
			message = "naht: /ack returned " .. tostring(response.StatusCode),
		})
	end
end

local function refusedPaths(body: string?): { string }
	local ok, decoded = pcall(function()
		return HttpService:JSONDecode(body or "")
	end)
	if not ok or type(decoded) ~= "table" or type(decoded.refused_paths) ~= "table" then
		return {}
	end
	return decoded.refused_paths
end

function Client.postChanges(self: Client, changes: { any })
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
			refused = refusedPaths(response.Body),
		})
	end
end

return Client
