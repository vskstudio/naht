--!strict
-- The HTTP client for the daemon. The plugin must initiate every exchange (HttpService is
-- request/response only — no server push), so the patch feed is a long-poll the connection loop
-- repeats. Bodies are MessagePack.

local HttpService = game:GetService("HttpService")

local Protocol = require(script.Parent.Protocol)

local Client = {}
Client.__index = Client

export type Client = typeof(setmetatable({} :: { baseUrl: string }, Client))

-- A client targeting the daemon at `baseUrl` (e.g. `http://localhost:34872`).
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

-- Handshake. Returns the server info, or raises if the place guard rejects this session (403).
function Client.info(self: Client, placeId: number): Protocol.ServerInfo
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
	return Protocol.decodeInfo(response.Body)
end

-- Long-poll for patches past `cursor`. Held open by the daemon until a change or its timeout.
function Client.patches(self: Client, cursor: number): Protocol.PatchBatch
	local response = request(self, {
		Url = "/patches?cursor=" .. tostring(cursor),
		Method = "GET",
	})
	if not response.Success then
		error("naht: /patches returned " .. tostring(response.StatusCode))
	end
	return Protocol.decodePatches(response.Body)
end

-- Long-poll for terrain blob patches past `cursor`, on the dedicated `/blobs` channel.
function Client.blobs(self: Client, cursor: number): Protocol.BlobPatchBatch
	local response = request(self, {
		Url = "/blobs?cursor=" .. tostring(cursor),
		Method = "GET",
	})
	if not response.Success then
		error("naht: /blobs returned " .. tostring(response.StatusCode))
	end
	return Protocol.decodeBlobs(response.Body)
end

-- Push Studio-side terrain blobs to the filesystem.
function Client.postBlobs(self: Client, changes: { Protocol.BlobInstance })
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

-- Confirm to the daemon which patch paths were applied, so their base advances.
function Client.postAck(self: Client, paths: { string })
	local response = request(self, {
		Url = "/ack",
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeAck(paths),
	})
	if not response.Success then
		error("naht: /ack returned " .. tostring(response.StatusCode))
	end
end

-- Push Studio-side changes to the filesystem.
function Client.postChanges(self: Client, changes: { Protocol.Change })
	local response = request(self, {
		Url = "/changes",
		Method = "POST",
		Headers = { ["Content-Type"] = "application/msgpack" },
		Body = Protocol.encodeChanges(changes),
	})
	if not response.Success then
		error("naht: /changes returned " .. tostring(response.StatusCode))
	end
end

return Client
