--!strict
-- Apply patches received from the daemon to the DataModel.
--
-- This is transport-and-apply, not sync logic: the daemon already decided what each patch does and
-- in which direction (only `ToStudio` patches reach here). The one mapping the plugin owns is
-- *where* a path lives in the tree: folders mirror the path's directories, and the leaf is a script
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
	children: (parent: Node) -> { Node },
	className: (node: Node) -> string,
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

local SCRIPT_KINDS = {
	{ suffix = ".server.luau", class = "Script" },
	{ suffix = ".client.luau", class = "LocalScript" },
	{ suffix = ".luau", class = "ModuleScript" },
}

local function scriptKind(fileName: string): { suffix: string, class: string }?
	for _, kind in SCRIPT_KINDS do
		if #fileName > #kind.suffix and string.sub(fileName, -#kind.suffix) == kind.suffix then
			return kind
		end
	end
	return nil
end

local function instanceName(fileName: string): string
	local kind = scriptKind(fileName)
	if kind == nil then
		return fileName
	end
	return string.sub(fileName, 1, #fileName - #kind.suffix)
end

local function expectedClass(fileName: string, patchClass: string): string
	local kind = scriptKind(fileName)
	if kind == nil then
		return patchClass
	end
	return kind.class
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

-- Walk the folder chain for every segment but the last without creating anything; nil when a folder is
-- missing.
function Applier._findParent(self, segments: { string }): Node?
	local tree = self.tree
	local parent = tree.root
	for index = 1, #segments - 1 do
		local existing = tree.findChild(parent, segments[index])
		if existing == nil then
			return nil
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
	local fileName = segments[#segments]
	local leafName = instanceName(fileName)

	if patch.kind == "Delete" then
		local parent = self:_findParent(segments)
		if parent == nil then
			return
		end
		local class = expectedClass(fileName, patch.class)
		for _, child in tree.children(parent) do
			if child.Name == leafName and tree.className(child) == class then
				tree.destroy(child)
			end
		end
		return
	end

	local parent = self:_ensureParent(segments)
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
		children = function(parent: Node): { Node }
			return parent:GetChildren()
		end,
		className = function(node: Node): string
			return node.ClassName
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
