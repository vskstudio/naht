--!strict

local Support = require(if script ~= nil then script.Parent.Support else "./Support")

local TerrainSync = {}

local doubled = Support.doubled
local failureMessage = Support.failureMessage

local BLOB_BACKOFF_START = 2
local BLOB_BACKOFF_MAX = 60

local TERRAIN_BACKOFF_START = 1
local TERRAIN_BACKOFF_MAX = 60
local TERRAIN_DEBOUNCE = 1
local TERRAIN_RESOLUTION = 4
local TERRAIN_PATH = "Terrain.terrain"
local TERRAIN_TOO_LARGE = "terrain too large for live push, use naht build"
local DECODE_FAILURE_LIMIT = 5

TerrainSync.BLOB_BACKOFF_START = BLOB_BACKOFF_START
TerrainSync.TERRAIN_BACKOFF_START = TERRAIN_BACKOFF_START

local function isUndecodableFailure(err: any): boolean
	return type(err) == "table" and err.undecodable == true
end

function TerrainSync.nextBlobBackoff(current: number): number
	return doubled(current, BLOB_BACKOFF_MAX)
end

function TerrainSync._serveBlobs(self, epoch: number, sessionId: string)
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
			self.blobBackoff = TerrainSync.nextBlobBackoff(self.blobBackoff)
		end
	end
end

function TerrainSync._applyBlobBatch(self, epoch: number, batch: any)
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
		self.blobBackoff = TerrainSync.nextBlobBackoff(self.blobBackoff)
	end
end

function TerrainSync._terrainRegion(self): any
	return self.terrain.MaxExtents:ExpandToGrid(TERRAIN_RESOLUTION)
end

function TerrainSync._applyBlob(self, blob: string)
	local region = self:_terrainRegion()
	self.terrainBlob.write(self.terrain, region, blob)
	self.terrainBaseline = self.terrainBlob.read(self.terrain, region, TERRAIN_RESOLUTION)
end

function TerrainSync._pushTerrain(self, epoch: number)
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

function TerrainSync._sendTerrain(self, epoch: number): boolean
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

function TerrainSync._onWaypoint(self)
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

return TerrainSync
