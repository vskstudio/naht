--!strict

local Fingerprint = {}

local FIRST_MODULUS = 1000000007
local SECOND_MODULUS = 998244353
local FIRST_BASE = 257
local SECOND_BASE = 911

function Fingerprint.source(content: string): string
	local first = 0
	local second = 0
	for index = 1, #content do
		local byte = string.byte(content, index)
		first = (first * FIRST_BASE + byte) % FIRST_MODULUS
		second = (second * SECOND_BASE + byte) % SECOND_MODULUS
	end
	return string.format("%x-%08x%08x", #content, first, second)
end

return Fingerprint
