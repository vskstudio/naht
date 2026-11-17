--!strict

local Terrain = {}

Terrain.MAX_VOXELS = 1048576

export type Grid = {
	resolution: number,
	min: { x: number, y: number, z: number },
	max: { x: number, y: number, z: number },
	materials: { { { number } } },
	occupancies: { { { number } } },
}

function Terrain.serialize(grid: Grid): string
	local nx = #grid.materials
	local ny = if nx > 0 then #grid.materials[1] else 0
	local nz = if ny > 0 then #grid.materials[1][1] else 0

	local parts = {
		string.pack(
			"<i4i4i4fffffff",
			nx,
			ny,
			nz,
			grid.min.x,
			grid.min.y,
			grid.min.z,
			grid.max.x,
			grid.max.y,
			grid.max.z,
			grid.resolution
		),
	}
	for x = 1, nx do
		for y = 1, ny do
			for z = 1, nz do
				table.insert(parts, string.pack("<I2f", grid.materials[x][y][z], grid.occupancies[x][y][z]))
			end
		end
	end
	return table.concat(parts)
end

local HEADER_SIZE = 40
local VOXEL_SIZE = 6
local MAX_DIMENSION = 2048

local function checkDimension(axis: string, value: number)
	if value < 1 or value > MAX_DIMENSION then
		error(string.format("Terrain blob dimension %s is %d, expected 1 to %d", axis, value, MAX_DIMENSION))
	end
end

function Terrain.deserialize(blob: string): Grid
	if #blob < HEADER_SIZE then
		error(string.format("Terrain blob is %d bytes, shorter than its %d-byte header", #blob, HEADER_SIZE))
	end
	local nx, ny, nz, minx, miny, minz, maxx, maxy, maxz, resolution, offset =
		string.unpack("<i4i4i4fffffff", blob)
	checkDimension("x", nx)
	checkDimension("y", ny)
	checkDimension("z", nz)
	if resolution <= 0 then
		error(string.format("Terrain blob resolution is %g, expected a positive value", resolution))
	end
	local expected = HEADER_SIZE + nx * ny * nz * VOXEL_SIZE
	if #blob ~= expected then
		error(string.format("Terrain blob is %d bytes, expected %d for %dx%dx%d", #blob, expected, nx, ny, nz))
	end

	local materials = {}
	local occupancies = {}
	for x = 1, nx do
		materials[x] = {}
		occupancies[x] = {}
		for y = 1, ny do
			materials[x][y] = {}
			occupancies[x][y] = {}
			for z = 1, nz do
				local material, occupancy
				material, occupancy, offset = string.unpack("<I2f", blob, offset)
				materials[x][y][z] = material
				occupancies[x][y][z] = occupancy
			end
		end
	end

	return {
		resolution = resolution,
		min = { x = minx, y = miny, z = minz },
		max = { x = maxx, y = maxy, z = maxz },
		materials = materials,
		occupancies = occupancies,
	}
end

local function regionBounds(region: any): ({ x: number, y: number, z: number }, { x: number, y: number, z: number })
	local center = region.CFrame.Position
	local half = region.Size
	return {
		x = center.X - half.X / 2,
		y = center.Y - half.Y / 2,
		z = center.Z - half.Z / 2,
	}, {
		x = center.X + half.X / 2,
		y = center.Y + half.Y / 2,
		z = center.Z + half.Z / 2,
	}
end

function Terrain.voxelCount(sizeX: number, sizeY: number, sizeZ: number, resolution: number): number
	return math.ceil(sizeX / resolution) * math.ceil(sizeY / resolution) * math.ceil(sizeZ / resolution)
end

function Terrain.read(terrain: any, region: any, resolution: number): string
	local materials, occupancies = terrain:ReadVoxels(region, resolution)
	local nx = #materials
	local ny = if nx > 0 then #materials[1] else 0
	local nz = if ny > 0 then #materials[1][1] else 0
	local materialValues = {}
	for x = 1, nx do
		materialValues[x] = {}
		for y = 1, ny do
			materialValues[x][y] = {}
			for z = 1, nz do
				materialValues[x][y][z] = materials[x][y][z].Value
			end
		end
	end
	local min, max = regionBounds(region)
	return Terrain.serialize({
		resolution = resolution,
		min = min,
		max = max,
		materials = materialValues,
		occupancies = occupancies,
	})
end

function Terrain.write(terrain: any, region: any, blob: string)
	local grid = Terrain.deserialize(blob)
	local materials = {}
	for x = 1, #grid.materials do
		materials[x] = {}
		for y = 1, #grid.materials[x] do
			materials[x][y] = {}
			for z = 1, #grid.materials[x][y] do
				materials[x][y][z] = (Enum :: any).Material:FromValue(grid.materials[x][y][z])
			end
		end
	end
	terrain:WriteVoxels(region, grid.resolution, materials, grid.occupancies)
end

return Terrain
