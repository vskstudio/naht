--!strict

local Hmac = {}

local WORD = 4294967296
local BLOCK_BYTES = 64

local ROUND_CONSTANTS = {
	0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
	0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
	0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
	0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
	0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
	0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
	0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
	0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
}

local INITIAL_STATE = {
	0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
}

local band, bnot, bxor, rrotate, rshift = bit32.band, bit32.bnot, bit32.bxor, bit32.rrotate, bit32.rshift

local function padded(message: string): string
	local length = #message
	local zeros = (BLOCK_BYTES - 9 - length) % BLOCK_BYTES
	return message .. "\128" .. string.rep("\0", zeros) .. string.pack(">I8", length * 8)
end

local function compress(state: { number }, schedule: { number }, message: string, offset: number)
	for index = 1, 16 do
		schedule[index] = string.unpack(">I4", message, offset + (index - 1) * 4)
	end
	for index = 17, 64 do
		local early, late = schedule[index - 15], schedule[index - 2]
		local sigmaEarly = bxor(rrotate(early, 7), rrotate(early, 18), rshift(early, 3))
		local sigmaLate = bxor(rrotate(late, 17), rrotate(late, 19), rshift(late, 10))
		schedule[index] = (schedule[index - 16] + sigmaEarly + schedule[index - 7] + sigmaLate) % WORD
	end
	local a, b, c, d, e, f, g, h = table.unpack(state)
	for index = 1, 64 do
		local sumE = bxor(rrotate(e, 6), rrotate(e, 11), rrotate(e, 25))
		local choice = bxor(band(e, f), band(bnot(e), g))
		local first = (h + sumE + choice + ROUND_CONSTANTS[index] + schedule[index]) % WORD
		local sumA = bxor(rrotate(a, 2), rrotate(a, 13), rrotate(a, 22))
		local majority = bxor(band(a, b), band(a, c), band(b, c))
		local second = (sumA + majority) % WORD
		h, g, f, e, d, c, b, a = g, f, e, (d + first) % WORD, c, b, a, (first + second) % WORD
	end
	for index, word in { a, b, c, d, e, f, g, h } do
		state[index] = (state[index] + word) % WORD
	end
end

function Hmac.sha256(message: string): string
	local state = table.clone(INITIAL_STATE)
	local schedule = table.create(64, 0)
	local blocks = padded(message)
	for offset = 1, #blocks, BLOCK_BYTES do
		compress(state, schedule, blocks, offset)
	end
	return string.pack(">I4I4I4I4I4I4I4I4", table.unpack(state))
end

local function masked(key: string, mask: number): string
	return (string.gsub(key, ".", function(character: string): string
		return string.char(bxor(string.byte(character), mask))
	end))
end

function Hmac.hmacSha256(key: string, message: string): string
	local blockKey = if #key > BLOCK_BYTES then Hmac.sha256(key) else key
	blockKey ..= string.rep("\0", BLOCK_BYTES - #blockKey)
	local inner = Hmac.sha256(masked(blockKey, 0x36) .. message)
	return Hmac.sha256(masked(blockKey, 0x5c) .. inner)
end

function Hmac.hex(bytes: string): string
	return (string.gsub(bytes, ".", function(character: string): string
		return string.format("%02x", string.byte(character))
	end))
end

function Hmac.proofMessage(port: number, nonce: string): string
	return string.format("naht-daemon-proof-v1\n%d\n%s", port, nonce)
end

function Hmac.proof(token: string, port: number, nonce: string): string
	return Hmac.hex(Hmac.hmacSha256(token, Hmac.proofMessage(port, nonce)))
end

return Hmac
