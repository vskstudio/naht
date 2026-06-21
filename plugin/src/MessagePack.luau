--!strict
-- A small, hand-written MessagePack codec — only the subset Naht's wire protocol uses.
--
-- It must match `rmp_serde::to_vec_named` on the daemon side exactly, so the encoding decisions
-- here mirror serde's: structs are string-keyed maps, an `Option::None` is `nil`, a unit enum
-- variant (Direction/PatchKind) is its name as a string, and an externally-tagged struct variant
-- (Change::Upsert/Delete) is a single-entry map `{ VariantName = { fields } }`.
--
-- Values are plain Luau: strings (byte strings), numbers, booleans, nil, string-keyed tables (maps),
-- and arrays wrapped with `MessagePack.array(list)` so an empty array is never mistaken for a map.

local MessagePack = {}

export type Array = { __mpArray: true, items: { any } }

-- Wrap a Luau list so the encoder emits a MessagePack array, unambiguously (even when empty).
function MessagePack.array(items: { any }): Array
	return { __mpArray = true, items = items }
end

local function isArray(value: any): boolean
	return type(value) == "table" and (value :: any).__mpArray == true
end

export type Binary = { __mpBin: true, bytes: string }

-- Wrap a byte string so the encoder emits a MessagePack *binary* payload (`bin`), not a text string.
-- Terrain and other blobs travel this way so they survive byte-for-byte.
function MessagePack.binary(bytes: string): Binary
	return { __mpBin = true, bytes = bytes }
end

local function isBinary(value: any): boolean
	return type(value) == "table" and (value :: any).__mpBin == true
end

-- === encode ===================================================================================

local function pushByte(out: { string }, byte: number)
	table.insert(out, string.char(byte))
end

local function pushUint(out: { string }, value: number)
	-- Always emit a 64-bit unsigned; the daemon's decoder accepts any width. Split into two 32-bit
	-- halves because Luau bit ops top out at 32 bits.
	local high = math.floor(value / 0x100000000)
	local low = value % 0x100000000
	pushByte(out, 0xcf) -- uint 64
	pushByte(out, math.floor(high / 0x1000000) % 0x100)
	pushByte(out, math.floor(high / 0x10000) % 0x100)
	pushByte(out, math.floor(high / 0x100) % 0x100)
	pushByte(out, high % 0x100)
	pushByte(out, math.floor(low / 0x1000000) % 0x100)
	pushByte(out, math.floor(low / 0x10000) % 0x100)
	pushByte(out, math.floor(low / 0x100) % 0x100)
	pushByte(out, low % 0x100)
end

local function pushString(out: { string }, value: string)
	local len = #value
	if len < 32 then
		pushByte(out, 0xa0 + len) -- fixstr
	elseif len < 0x100 then
		pushByte(out, 0xd9) -- str 8
		pushByte(out, len)
	elseif len < 0x10000 then
		pushByte(out, 0xda) -- str 16
		pushByte(out, math.floor(len / 0x100))
		pushByte(out, len % 0x100)
	else
		pushByte(out, 0xdb) -- str 32
		pushByte(out, math.floor(len / 0x1000000) % 0x100)
		pushByte(out, math.floor(len / 0x10000) % 0x100)
		pushByte(out, math.floor(len / 0x100) % 0x100)
		pushByte(out, len % 0x100)
	end
	table.insert(out, value)
end

local function pushMapHeader(out: { string }, count: number)
	if count < 16 then
		pushByte(out, 0x80 + count) -- fixmap
	else
		pushByte(out, 0xde) -- map 16
		pushByte(out, math.floor(count / 0x100))
		pushByte(out, count % 0x100)
	end
end

local function pushArrayHeader(out: { string }, count: number)
	if count < 16 then
		pushByte(out, 0x90 + count) -- fixarray
	else
		pushByte(out, 0xdc) -- array 16
		pushByte(out, math.floor(count / 0x100))
		pushByte(out, count % 0x100)
	end
end

local encodeValue

local function encodeMap(out: { string }, map: { [string]: any })
	local keys = {}
	for key in pairs(map) do
		table.insert(keys, key)
	end
	table.sort(keys)
	pushMapHeader(out, #keys)
	for _, key in ipairs(keys) do
		pushString(out, key)
		encodeValue(out, map[key])
	end
end

function encodeValue(out: { string }, value: any)
	local kind = type(value)
	if value == nil then
		pushByte(out, 0xc0) -- nil
	elseif kind == "boolean" then
		pushByte(out, if value then 0xc3 else 0xc2)
	elseif kind == "number" then
		pushUint(out, value)
	elseif kind == "string" then
		pushString(out, value)
	elseif isBinary(value) then
		local bytes = (value :: Binary).bytes
		local len = #bytes
		if len < 0x100 then
			pushByte(out, 0xc4) -- bin 8
			pushByte(out, len)
		elseif len < 0x10000 then
			pushByte(out, 0xc5) -- bin 16
			pushByte(out, math.floor(len / 0x100))
			pushByte(out, len % 0x100)
		else
			pushByte(out, 0xc6) -- bin 32
			pushByte(out, math.floor(len / 0x1000000) % 0x100)
			pushByte(out, math.floor(len / 0x10000) % 0x100)
			pushByte(out, math.floor(len / 0x100) % 0x100)
			pushByte(out, len % 0x100)
		end
		table.insert(out, bytes)
	elseif isArray(value) then
		local items = (value :: Array).items
		pushArrayHeader(out, #items)
		for _, item in ipairs(items) do
			encodeValue(out, item)
		end
	elseif kind == "table" then
		encodeMap(out, value)
	else
		error("MessagePack: cannot encode value of type " .. kind)
	end
end

-- Encode a Luau value into a MessagePack byte string.
function MessagePack.encode(value: any): string
	local out = {}
	encodeValue(out, value)
	return table.concat(out)
end

-- === decode ===================================================================================

local decodeValue: (string, number) -> (any, number)
local decodeStr: (string, number, number) -> (string, number)
local decodeArray: (string, number, number) -> ({ any }, number)
local decodeMap: (string, number, number) -> ({ [string]: any }, number)

local function readBytesUint(bytes: string, pos: number, count: number): (number, number)
	local value = 0
	for index = 0, count - 1 do
		value = value * 0x100 + string.byte(bytes, pos + index)
	end
	return value, pos + count
end

function decodeValue(bytes: string, pos: number): (any, number)
	local byte = string.byte(bytes, pos)
	pos += 1

	if byte < 0x80 then
		return byte, pos -- positive fixint
	elseif byte >= 0xe0 then
		return byte - 0x100, pos -- negative fixint
	elseif byte >= 0x80 and byte <= 0x8f then
		return decodeMap(bytes, pos, byte - 0x80)
	elseif byte >= 0x90 and byte <= 0x9f then
		return decodeArray(bytes, pos, byte - 0x90)
	elseif byte >= 0xa0 and byte <= 0xbf then
		return decodeStr(bytes, pos, byte - 0xa0)
	elseif byte == 0xc0 then
		return nil, pos
	elseif byte == 0xc2 then
		return false, pos
	elseif byte == 0xc3 then
		return true, pos
	elseif byte == 0xcc then
		return readBytesUint(bytes, pos, 1)
	elseif byte == 0xcd then
		return readBytesUint(bytes, pos, 2)
	elseif byte == 0xce then
		return readBytesUint(bytes, pos, 4)
	elseif byte == 0xcf then
		return readBytesUint(bytes, pos, 8)
	elseif byte == 0xd0 then
		local value, next = readBytesUint(bytes, pos, 1)
		return (if value >= 0x80 then value - 0x100 else value), next
	elseif byte == 0xd1 then
		local value, next = readBytesUint(bytes, pos, 2)
		return (if value >= 0x8000 then value - 0x10000 else value), next
	elseif byte == 0xd2 then
		local value, next = readBytesUint(bytes, pos, 4)
		return (if value >= 0x80000000 then value - 0x100000000 else value), next
	elseif byte == 0xc4 then
		local len, next = readBytesUint(bytes, pos, 1)
		return decodeStr(bytes, next, len) -- bin 8 — raw bytes, returned as a Luau string
	elseif byte == 0xc5 then
		local len, next = readBytesUint(bytes, pos, 2)
		return decodeStr(bytes, next, len) -- bin 16
	elseif byte == 0xc6 then
		local len, next = readBytesUint(bytes, pos, 4)
		return decodeStr(bytes, next, len) -- bin 32
	elseif byte == 0xd9 then
		local len, next = readBytesUint(bytes, pos, 1)
		return decodeStr(bytes, next, len)
	elseif byte == 0xda then
		local len, next = readBytesUint(bytes, pos, 2)
		return decodeStr(bytes, next, len)
	elseif byte == 0xdb then
		local len, next = readBytesUint(bytes, pos, 4)
		return decodeStr(bytes, next, len)
	elseif byte == 0xdc then
		local len, next = readBytesUint(bytes, pos, 2)
		return decodeArray(bytes, next, len)
	elseif byte == 0xdd then
		local len, next = readBytesUint(bytes, pos, 4)
		return decodeArray(bytes, next, len)
	elseif byte == 0xde then
		local len, next = readBytesUint(bytes, pos, 2)
		return decodeMap(bytes, next, len)
	elseif byte == 0xdf then
		local len, next = readBytesUint(bytes, pos, 4)
		return decodeMap(bytes, next, len)
	end
	error(string.format("MessagePack: unsupported byte 0x%02x at %d", byte, pos - 1))
end

function decodeStr(bytes: string, pos: number, len: number): (string, number)
	return string.sub(bytes, pos, pos + len - 1), pos + len
end

function decodeArray(bytes: string, pos: number, count: number): ({ any }, number)
	local items = {}
	for index = 1, count do
		local value
		value, pos = decodeValue(bytes, pos)
		items[index] = value
	end
	return items, pos
end

function decodeMap(bytes: string, pos: number, count: number): ({ [string]: any }, number)
	local map = {}
	for _ = 1, count do
		local key, value
		key, pos = decodeValue(bytes, pos)
		value, pos = decodeValue(bytes, pos)
		map[key] = value
	end
	return map, pos
end

-- Decode a MessagePack byte string into a Luau value.
function MessagePack.decode(bytes: string): any
	local value = decodeValue(bytes, 1)
	return value
end

return MessagePack
