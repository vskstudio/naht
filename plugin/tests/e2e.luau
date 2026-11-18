local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local task = require("@lune/task")

local Protocol = require("../src/Protocol").new(require("../src/MessagePack"))
local Client = require("../src/Client")

local BIN = process.env.NAHT_BIN or "target/debug/naht"
local PLACE_ID = 1
local FILE = "src/A.luau"
local STUDIO_FILE = "src/B.luau"
local CREATED_FILE = "src/C.luau"
local RENAME_FROM = "src/RenameOld.luau"
local RENAME_TO = "src/RenameNew.luau"
local READY_ATTEMPTS = 300
local STOP_ATTEMPTS = 300
local EDIT_ATTEMPTS = 3
local TICK = 0.05
local WORK = process.cwd .. "/target/e2e-" .. tostring(math.random(100000000, 999999999))

local failures = 0
local daemons = {}

local function check(name, ok, detail)
	if ok then
		print("ok   - " .. name)
	else
		failures += 1
		print("FAIL - " .. name .. (if detail ~= nil then ": " .. detail else ""))
	end
end

local function request(method, url, headers, body)
	local responsePath = WORK .. "/response"
	local args = { "-sS", "-o", responsePath, "-w", "%{http_code}", "-X", method }
	for name, value in headers do
		table.insert(args, "-H")
		table.insert(args, name .. ": " .. value)
	end
	if body ~= nil then
		local bodyPath = WORK .. "/request"
		fs.writeFile(bodyPath, body)
		table.insert(args, "--data-binary")
		table.insert(args, "@" .. bodyPath)
	end
	table.insert(args, url)
	local result = process.exec("curl", args)
	local status = tonumber(result.stdout) or 0
	local responseBody = if fs.isFile(responsePath) then fs.readFile(responsePath) else ""
	return { status = status, body = responseBody }
end

local function httpBridge()
	return {
		RequestAsync = function(_, options)
			local response = request(options.Method, options.Url, options.Headers or {}, options.Body)
			return {
				Success = response.status >= 200 and response.status < 300,
				StatusCode = response.status,
				Body = response.body,
			}
		end,
		JSONDecode = function(_, text)
			return serde.decode("json", text)
		end,
	}
end

local function isAlive(base)
	return request("GET", base .. "/heartbeat", { ["x-naht-client"] = "plugin" }, nil).status == 200
end

local function isRunning(pid)
	return process.exec("kill", { "-0", pid }).ok
end

local function readLog(log)
	if fs.isFile(log) then
		return fs.readFile(log)
	end
	return "(no daemon log)"
end

local function spawn(root, port, log)
	local result = process.exec("sh", {
		"-c",
		'exec "$0" serve "$1" --port "$2" >"$3" 2>&1 </dev/null & echo $!',
		BIN,
		root,
		tostring(port),
		log,
	})
	local pid = string.match(result.stdout, "%d+")
	if pid == nil then
		error("could not start " .. BIN .. ": " .. result.stderr)
	end
	return pid
end

local function startDaemon(root, port, log)
	local base = "http://127.0.0.1:" .. tostring(port)
	local daemon = { pid = spawn(root, port, log), base = base }
	table.insert(daemons, daemon)
	for _ = 1, READY_ATTEMPTS do
		if isAlive(base) then
			daemon.client = Client.new(base, PLACE_ID, httpBridge())
			return daemon
		end
		task.wait(TICK)
	end
	error("naht serve did not answer on " .. base .. "\n" .. readLog(log))
end

local function stopDaemon(daemon)
	process.exec("kill", { daemon.pid })
	for _ = 1, STOP_ATTEMPTS do
		if not isRunning(daemon.pid) then
			return
		end
		task.wait(TICK)
	end
	error("naht serve " .. daemon.pid .. " did not exit after SIGTERM")
end

local function recordedDaemonId(root)
	local lines = string.split(fs.readFile(root .. "/.naht/daemon.port"), "\n")
	return lines[2]
end

local function findPatch(batch, path, content)
	for _, sequenced in batch.patches do
		if sequenced.path == path and (content == nil or sequenced.content == content) then
			return sequenced
		end
	end
	return nil
end

local function ackAll(client, sessionId, batch)
	local seqs = {}
	for _, sequenced in batch.patches do
		table.insert(seqs, sequenced.seq)
	end
	if #seqs > 0 then
		client:postAck(seqs, sessionId)
	end
end

local function awaitPatch(client, sessionId, cursor, path, content)
	for _ = 1, EDIT_ATTEMPTS do
		local batch = client:patches(cursor, sessionId)
		if findPatch(batch, path, content) ~= nil then
			return batch
		end
		ackAll(client, sessionId, batch)
		cursor = batch.cursor
	end
	return nil
end

local function countDeletes(batch)
	local deletes = 0
	for _, sequenced in batch.patches do
		if sequenced.kind == "Delete" then
			deletes += 1
		end
	end
	return deletes
end

local function scenario(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. FILE, "return 1")

	local first = startDaemon(root, port, log)
	local info = first.client:info()
	check("the handshake reports the daemon id recorded in daemon.port", info.daemon_id == recordedDaemonId(root))

	local initial = first.client:patches(0, info.session_id)
	check("a fresh run sends the existing file to the plugin", findPatch(initial, FILE, "return 1") ~= nil)
	ackAll(first.client, info.session_id, initial)

	fs.writeFile(root .. "/" .. FILE, "return 2")
	local edited = awaitPatch(first.client, info.session_id, initial.cursor, FILE, "return 2")
	local edit = if edited ~= nil then findPatch(edited, FILE, "return 2") else nil
	check(
		"a disk edit reaches the plugin as a ToStudio update",
		edit ~= nil and edit.direction == "ToStudio" and edit.kind == "Update"
	)
	if edited ~= nil then
		ackAll(first.client, info.session_id, edited)
	end

	first.client:postChanges({ Protocol.upsert(STUDIO_FILE, "ModuleScript", "return 3") })
	local written, studioContent = pcall(fs.readFile, root .. "/" .. STUDIO_FILE)
	check("a plugin edit is written to disk", written and studioContent == "return 3")

	stopDaemon(first)
	fs.writeFile(root .. "/" .. CREATED_FILE, "return 4")

	local second = startDaemon(root, port, log)
	local restarted = second.client:info()
	check("a restart gives the plugin a new session id", restarted.session_id ~= info.session_id)

	local replay = second.client:patches(0, restarted.session_id)
	check("a restart does not replay patches the plugin already acked", findPatch(replay, FILE, nil) == nil)
	check("a file created while the daemon was down reaches the plugin", findPatch(replay, CREATED_FILE, "return 4") ~= nil)
	check("a file the plugin pushed is not sent back after a restart", findPatch(replay, STUDIO_FILE, nil) == nil)
	check("a restart emits no delete", countDeletes(replay) == 0)
	ackAll(second.client, restarted.session_id, replay)

	second.client:postChanges({ Protocol.upsert(RENAME_FROM, "ModuleScript", "return 5") })
	check("the file to rename is written to disk", fs.isFile(root .. "/" .. RENAME_FROM))

	second.client:postChanges({
		Protocol.delete(RENAME_FROM),
		Protocol.upsert(RENAME_TO, "ModuleScript", "return 5"),
	})
	check("a rename batch removes the old file from disk", not fs.isFile(root .. "/" .. RENAME_FROM))
	check(
		"a rename batch writes the new file to disk",
		fs.isFile(root .. "/" .. RENAME_TO) and fs.readFile(root .. "/" .. RENAME_TO) == "return 5"
	)

	stopDaemon(second)
end

local function main()
	local root = WORK .. "/demo"
	local log = WORK .. "/naht.log"
	local port = math.random(20000, 60000)
	fs.writeDir(WORK)

	local ok, err = pcall(scenario, root, port, log)
	if not ok then
		check("the daemon scenario ran to completion", false, tostring(err))
	end
	for _, daemon in daemons do
		pcall(stopDaemon, daemon)
	end
	if failures > 0 then
		print(readLog(log))
	end
	fs.removeDir(WORK)

	if failures > 0 then
		error(string.format("%d daemon e2e check(s) failed", failures))
	end
	print("all daemon e2e checks passed")
end

main()
