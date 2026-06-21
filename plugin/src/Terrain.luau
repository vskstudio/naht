--!strict
-- Terrain voxel sync: read the region via `ReadVoxels`, ship it as an opaque binary blob, and write
-- it back via `WriteVoxels` (architecture §9, Stage 11). There is **no diff/merge** of voxels — the
-- blob is synced whole and conflicts freeze, exactly like the daemon's binary path.
--
-- The (de)serialization is pure and works on plain numbers, so it round-trips headless in the test
-- harness; only `read`/`write` touch the Studio `Terrain` API.

local Terrain = {}

export type Grid = {
	resolution: number,
	min: { x: number, y: number, z: number },
	max: { x: number, y: number, z: number },
	-- materials[x][y][z] = Enum.Material value (a number); occupancies[x][y][z] = 0..1.
	materials: { { { number } } },
	occupancies: { { { number } } },
}

-- Serialize a voxel grid to an opaque blob. Layout: a fixed header (dims, region bounds, resolution)
-- then one `(material:u16, occupancy:f32)` record per voxel in x→y→z order. Little-endian throughout.
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

-- Reconstruct a voxel grid from a blob produced by `serialize`.
function Terrain.deserialize(blob: string): Grid
	local nx, ny, nz, minx, miny, minz, maxx, maxy, maxz, resolution, offset =
		string.unpack("<i4i4i4fffffff", blob)

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

-- Read the terrain in `region` (a `Region2int16`/`Region3` value) to a blob. Studio-only: materials
-- come back as `Enum.Material`, converted to their numeric value for the opaque blob.
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
	return Terrain.serialize({
		resolution = resolution,
		min = { x = region.CFrame.Position.X, y = region.CFrame.Position.Y, z = region.CFrame.Position.Z },
		max = { x = region.Size.X, y = region.Size.Y, z = region.Size.Z },
		materials = materialValues,
		occupancies = occupancies,
	})
end

-- Write a blob back into the terrain. Studio-only: numeric material values become `Enum.Material`.
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
