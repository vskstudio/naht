local fs = require("@lune/fs")
local process = require("@lune/process")
local serde = require("@lune/serde")
local task = require("@lune/task")

local Protocol = require("../src/Protocol").new(require("../src/MessagePack"))
local Client = require("../src/Client")
local Fingerprint = require("../src/Fingerprint")

local BIN = process.env.NAHT_BIN or "target/debug/naht"
local PLACE_ID = 1
local FILE = "src/A.luau"
local STUDIO_FILE = "src/B.luau"
local CREATED_FILE = "src/C.luau"
local RENAME_FROM = "src/RenameOld.luau"
local RENAME_TO = "src/RenameNew.luau"
local UNSAVED_FILE = "src/Old.luau"
local UNSAVED_NEW = "src/New.luau"
local KEEP_FILE = "src/Keep.luau"
local HIDDEN_FILE = "src/.Hidden.luau"
local OTHER_FILE = "src/Other.luau"
local REFUSED_FILE = "src/Refused.luau"
local CHILDLESS_DIR = "src/Dir"
local CHILDLESS_INIT = "src/Dir/init.luau"
local CHILDLESS_FLAT = "src/Dir.luau"
local OTHER_PLACE_ID = 999
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
		GenerateGUID = function(_, _wrapped)
			local digits = {}
			for _ = 1, 32 do
				table.insert(digits, string.format("%x", math.random(0, 15)))
			end
			return table.concat(digits)
		end,
	}
end

local function isAlive(base)
	return request("GET", base .. "/info", { ["x-naht-client"] = "plugin" }, nil).status == 200
end

local function readToken(root)
	return string.match(fs.readFile(root .. "/.naht/token"), "^%s*(.-)%s*$") or ""
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
			daemon.client = Client.new(base, function(): number return PLACE_ID end, httpBridge())
			daemon.client:setToken(readToken(root))
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

local function entryOf(path: string, content: string)
	return { path = path, class = "ModuleScript", hash = Fingerprint.source(content) }
end

local function renamedWhileStopped(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. RENAME_FROM, "return 6")

	local first = startDaemon(root, port, log)
	local info = first.client:info()
	local initial = first.client:patches(0, info.session_id)
	ackAll(first.client, info.session_id, initial)
	stopDaemon(first)

	local second = startDaemon(root, port, log)
	local restarted = second.client:info()
	second.client:postInventory({ entryOf(RENAME_TO, "return 6") }, restarted.session_id)
	check("an inventory removes the file renamed while the plugin was stopped", not fs.isFile(root .. "/" .. RENAME_FROM))
	check(
		"an inventory writes the renamed file under its new path",
		fs.isFile(root .. "/" .. RENAME_TO) and fs.readFile(root .. "/" .. RENAME_TO) == "return 6"
	)
	stopDaemon(second)
end

local function unsavedEdits(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. UNSAVED_FILE, "v1")

	local daemon = startDaemon(root, port, log)
	local info = daemon.client:info()
	local initial = daemon.client:patches(0, info.session_id)
	ackAll(daemon.client, info.session_id, initial)
	local cursor = initial.cursor

	fs.writeFile(root .. "/" .. UNSAVED_NEW, "written in the editor")
	local created = awaitPatch(daemon.client, info.session_id, cursor, UNSAVED_NEW, "written in the editor")
	check("a file written in the editor reaches the plugin", created ~= nil)
	if created ~= nil then
		ackAll(daemon.client, info.session_id, created)
		cursor = created.cursor
	end
	task.wait(0.3)

	fs.writeFile(root .. "/" .. UNSAVED_FILE, "v2 edited in the editor")
	local edited = awaitPatch(daemon.client, info.session_id, cursor, UNSAVED_FILE, "v2 edited in the editor")
	check("an edit made in the editor reaches the plugin", edited ~= nil)
	if edited ~= nil then
		ackAll(daemon.client, info.session_id, edited)
	end
	task.wait(0.3)

	local accepted, err = pcall(daemon.client.postInventory, daemon.client, { entryOf(UNSAVED_FILE, "v1") }, info.session_id)
	check("the daemon accepts an inventory of the place as last saved", accepted and err ~= false, tostring(err))
	check(
		"an inventory of the place as last saved keeps the file written in the editor",
		fs.isFile(root .. "/" .. UNSAVED_NEW) and fs.readFile(root .. "/" .. UNSAVED_NEW) == "written in the editor"
	)
	check(
		"an inventory of the place as last saved does not overwrite the editor's edit",
		fs.isFile(root .. "/" .. UNSAVED_FILE) and fs.readFile(root .. "/" .. UNSAVED_FILE) == "v2 edited in the editor"
	)
	stopDaemon(daemon)
end

local function hiddenNameRefused(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. KEEP_FILE, "return 1")

	local daemon = startDaemon(root, port, log)
	local info = daemon.client:info()
	local ok, err = pcall(daemon.client.postInventory, daemon.client, {
		entryOf(KEEP_FILE, "return 1"),
		entryOf(HIDDEN_FILE, "return 2"),
	}, info.session_id)
	check(
		"the daemon refuses an inventory that names a hidden script with a 400",
		not ok and type(err) == "table" and err.status == 400,
		tostring(if type(err) == "table" then err.status else err)
	)
	check(
		"the hidden name is named in the refusal",
		not ok and type(err) == "table" and type(err.refused) == "table" and table.find(err.refused, HIDDEN_FILE) ~= nil,
		if type(err) == "table" and type(err.refused) == "table" then table.concat(err.refused, ",") else tostring(err)
	)
	check("the refusal leaves the existing file on disk", fs.isFile(root .. "/" .. KEEP_FILE))
	stopDaemon(daemon)
end

local function otherPlaceRefused(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. KEEP_FILE, "return 1")

	local bound = startDaemon(root, port, log)
	local boundInfo = bound.client:info()
	bound.client:postInventory({ entryOf(KEEP_FILE, "return 1") }, boundInfo.session_id)
	stopDaemon(bound)

	local other = startDaemon(root, math.random(20000, 60000), log)
	local otherClient = Client.new(other.base, function(): number return OTHER_PLACE_ID end, httpBridge())
	local infoOk, infoErr = pcall(otherClient.info, otherClient)
	check(
		"a session from a place the project is not bound to is refused at the handshake",
		not infoOk and string.find(tostring(infoErr), "bound to place 1", 1, true) ~= nil,
		tostring(infoErr)
	)
	check("the refusal leaves the bound place's file on disk", fs.isFile(root .. "/" .. KEEP_FILE))
	stopDaemon(other)
end

local function childlessInitKept(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeDir(root .. "/" .. CHILDLESS_DIR)
	fs.writeFile(root .. "/" .. CHILDLESS_INIT, "return 'dir'")

	local daemon = startDaemon(root, port, log)
	local info = daemon.client:info()
	local accepted, err = pcall(daemon.client.postInventory, daemon.client, { entryOf(CHILDLESS_INIT, "return 'dir'") }, info.session_id)
	check("the daemon accepts an inventory that lists a childless init folder under its init path", accepted, tostring(err))
	check(
		"the childless init file stays where the plugin wrote it",
		fs.isFile(root .. "/" .. CHILDLESS_INIT) and fs.readFile(root .. "/" .. CHILDLESS_INIT) == "return 'dir'"
	)
	check("no flat sibling is created for the childless init folder", not fs.isFile(root .. "/" .. CHILDLESS_FLAT))
	stopDaemon(daemon)
end

local function tokenRefused(root, port, log)
	local init = process.exec(BIN, { "init", root })
	if not init.ok then
		error("naht init failed: " .. init.stderr)
	end
	fs.writeDir(root .. "/src")
	fs.writeFile(root .. "/" .. KEEP_FILE, "return 1")

	local daemon = startDaemon(root, port, log)
	local info = daemon.client:info()

	local bare = Client.new(daemon.base, function(): number return PLACE_ID end, httpBridge())
	local missingOk, missingErr = pcall(bare.info, bare)
	check(
		"a client with no token stops at the handshake and asks for it",
		not missingOk and type(missingErr) == "table" and missingErr.tokenRefused == true and missingErr.message == Client.TOKEN_REQUIRED,
		if type(missingErr) == "table" then tostring(missingErr.message) else tostring(missingErr)
	)
	local heldPoll, heldPollErr = pcall(bare.patches, bare, 0, info.session_id)
	check("a client with no token holds back its poll", not heldPoll and heldPollErr == Client.UNVERIFIED, tostring(heldPollErr))

	local tokenless = request("GET", daemon.base .. "/patches?cursor=0&session=" .. info.session_id, {
		["x-naht-client"] = "plugin",
		["x-naht-place-id"] = tostring(PLACE_ID),
	})
	check("a request without the token is refused with a 401", tokenless.status == 401, tostring(tokenless.status))
	check(
		"a request without the token is told the daemon needs it",
		string.find(tokenless.body, "token required", 1, true) ~= nil,
		tokenless.body
	)

	local wrong = Client.new(daemon.base, function(): number return PLACE_ID end, httpBridge())
	wrong:setToken("not-the-project-token")
	local provedOk, provedErr = pcall(wrong.info, wrong)
	check(
		"a client holding a wrong token finds the daemon proof does not match it",
		not provedOk and type(provedErr) == "table" and provedErr.message == Client.PROOF_FAILED,
		if type(provedErr) == "table" then tostring(provedErr.message) else tostring(provedErr)
	)
	local heldOk, heldErr = pcall(wrong.postChanges, wrong, { Protocol.upsert(REFUSED_FILE, "ModuleScript", "return 9") })
	check("the plugin holds back a write it cannot prove", not heldOk and heldErr == Client.UNVERIFIED, tostring(heldErr))

	local forced = request("POST", daemon.base .. "/changes", {
		["x-naht-client"] = "plugin",
		["x-naht-place-id"] = tostring(PLACE_ID),
		["x-naht-token"] = "not-the-project-token",
		["Content-Type"] = "application/msgpack",
	}, Protocol.encodeChanges({ Protocol.upsert(REFUSED_FILE, "ModuleScript", "return 9") }))
	check("a request with a wrong token is refused with a 401", forced.status == 401, tostring(forced.status))
	check(
		"a wrong token is told the daemon rejected it",
		string.find(forced.body, "token rejected", 1, true) ~= nil,
		forced.body
	)
	check("a refused write leaves no file on disk", not fs.isFile(root .. "/" .. REFUSED_FILE))

	local accepted, acceptedErr = pcall(daemon.client.postChanges, daemon.client, { Protocol.upsert(REFUSED_FILE, "ModuleScript", "return 9") })
	check("the right token is still accepted after the refusals", accepted, tostring(acceptedErr))
	check("the accepted write reaches disk", fs.isFile(root .. "/" .. REFUSED_FILE))
	stopDaemon(daemon)
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
	local renamedLog = WORK .. "/renamed.log"
	local renamedOk, renamedErr = pcall(renamedWhileStopped, WORK .. "/renamed", math.random(20000, 60000), renamedLog)
	if not renamedOk then
		check("the rename-while-stopped scenario ran to completion", false, tostring(renamedErr))
	end
	local extra = {
		{ name = "unsaved", run = unsavedEdits },
		{ name = "hidden", run = hiddenNameRefused },
		{ name = "other-place", run = otherPlaceRefused },
		{ name = "token", run = tokenRefused },
		{ name = "childless-init", run = childlessInitKept },
	}
	for _, scenarioEntry in extra do
		local scenarioOk, scenarioErr = pcall(scenarioEntry.run, WORK .. "/" .. scenarioEntry.name, math.random(20000, 60000), WORK .. "/" .. scenarioEntry.name .. ".log")
		if not scenarioOk then
			check(scenarioEntry.name .. " scenario ran to completion", false, tostring(scenarioErr))
		end
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
