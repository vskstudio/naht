--!strict

local Support = {}

Support.CLASS_SUFFIX = {
	Script = ".server.luau",
	LocalScript = ".client.luau",
	ModuleScript = ".luau",
}
Support.RETRY_BACKOFF_MAX = 60

local TRANSIENT_STATUSES = { [408] = true, [500] = true, [502] = true, [503] = true, [504] = true }

function Support.doubled(current: number, max: number): number
	return math.min(current * 2, max)
end

function Support.isTransientFailure(err: any): boolean
	if type(err) ~= "table" or err.status == nil then
		return true
	end
	return TRANSIENT_STATUSES[err.status] == true
end

function Support.isTokenRefusal(err: any): boolean
	return type(err) == "table" and err.tokenRefused == true
end

function Support.failureMessage(err: any): string
	if type(err) == "table" then
		return tostring(err.message)
	end
	return tostring(err)
end

function Support.isServerError(err: any): boolean
	return type(err) == "table" and err.status == 500
end

function Support.weakMap(): { [any]: any }
	return setmetatable({}, { __mode = "k" }) :: any
end

function Support.sortedKeys(map: { [string]: any }): { string }
	local keys = {}
	for key in map do
		table.insert(keys, key)
	end
	table.sort(keys)
	return keys
end

return Support
