--!strict
-- Apply patches received from the daemon to the DataModel.
--
-- This is transport-and-apply, not sync logic: the daemon already decided what each patch does and
-- in which direction (only `ToStudio` patches reach here). The one mapping the plugin owns is
-- *where* a path lives in the tree — folders mirror the path's directories, and the leaf is a script
-- named after the file. All DataModel mutation goes through an injected `Tree` interface so the apply
-- logic is unit-testable without Studio; `Apply.studioTree` is the real binding.

local Apply = {}

-- The minimal tree interface Apply needs. The Studio binding wraps real `Instance` calls; tests pass
-- a mock over plain tables.
export type Node = any
export type Tree = {
	root: Node,
	createInstance: (className: string, name: string) -> Node,
	findChild: (parent: Node, name: string) -> Node?,
	setParent: (node: Node, parent: Node) -> (),
	setSource: (node: Node, content: string) -> (),
	destroy: (node: Node) -> (),
}

export type Patch = {
	path: string,
	class: string,
	kind: string,
	content: string?,
}

local SCRIPT_SUFFIXES = { ".server.luau", ".client.luau", ".luau" }

-- The instance name for a file name, stripping the known script suffix.
local function instanceName(fileName: string): string
	for _, suffix in SCRIPT_SUFFIXES do
		if #fileName > #suffix and string.sub(fileName, -#suffix) == suffix then
			return string.sub(fileName, 1, #fileName - #suffix)
		end
	end
	return fileName
end

local function splitPath(path: string): { string }
	local segments = {}
	for segment in string.gmatch(path, "[^/\\]+") do
		table.insert(segments, segment)
	end
	return segments
end

local Applier = {}
Applier.__index = Applier

-- Create an applier rooted at `tree.root`.
function Apply.create(tree: Tree)
	return setmetatable({ tree = tree }, Applier)
end

-- Walk (creating as needed) the folder chain for every segment but the last, returning the parent the
-- leaf should live under.
function Applier._ensureParent(self, segments: { string }): Node
	local tree = self.tree
	local parent = tree.root
	for index = 1, #segments - 1 do
		local name = segments[index]
		local existing = tree.findChild(parent, name)
		if existing == nil then
			existing = tree.createInstance("Folder", name)
			tree.setParent(existing, parent)
		end
		parent = existing
	end
	return parent
end

-- Apply one patch. `Conflict` carries the marker text and is applied like an `Update` so the
-- conflict is visible in Studio; the caller surfaces it in the UI.
function Applier.apply(self, patch: Patch)
	local tree = self.tree
	local segments = splitPath(patch.path)
	if #segments == 0 then
		return
	end
	local leafName = instanceName(segments[#segments])
	local parent = self:_ensureParent(segments)

	if patch.kind == "Delete" then
		local existing = tree.findChild(parent, leafName)
		if existing ~= nil then
			tree.destroy(existing)
		end
		return
	end

	local node = tree.findChild(parent, leafName)
	if node == nil then
		node = tree.createInstance(patch.class, leafName)
		tree.setParent(node, parent)
	end
	if patch.content ~= nil then
		tree.setSource(node, patch.content)
	end
end

-- Apply a list of patches in order.
function Applier.applyBatch(self, patches: { Patch })
	for _, patch in patches do
		self:apply(patch)
	end
end

-- The Studio binding: a `Tree` backed by real `Instance` calls, rooted at `root`.
function Apply.studioTree(root: Node): Tree
	return {
		root = root,
		createInstance = function(className: string, name: string): Node
			local instance = Instance.new(className :: any)
			instance.Name = name
			return instance
		end,
		findChild = function(parent: Node, name: string): Node?
			return parent:FindFirstChild(name)
		end,
		setParent = function(node: Node, parent: Node)
			node.Parent = parent
		end,
		setSource = function(node: Node, content: string)
			node.Source = content
		end,
		destroy = function(node: Node)
			node:Destroy()
		end,
	}
end

return Apply
