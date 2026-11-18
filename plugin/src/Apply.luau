--!strict

local Apply = {}

export type Node = any
export type Tree = {
	root: Node,
	createInstance: (className: string, name: string) -> Node,
	children: (parent: Node) -> { Node },
	className: (node: Node) -> string,
	getSource: (node: Node) -> string,
	setParent: (node: Node, parent: Node) -> (),
	setSource: (node: Node, content: string) -> (),
	destroy: (node: Node) -> (),
}

export type Echo = {
	expect: (path: string, content: string) -> (),
	forget: (path: string) -> (),
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

local SCRIPT_CLASSES = { Script = true, LocalScript = true, ModuleScript = true }

local INIT_CLASSES = {
	["init.server.luau"] = "Script",
	["init.client.luau"] = "LocalScript",
	["init.luau"] = "ModuleScript",
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

function Apply.create(tree: Tree, echo: Echo?)
	return setmetatable({ tree = tree, echo = echo }, Applier)
end

function Applier._container(self, parent: Node, name: string): Node?
	local tree = self.tree
	local script: Node? = nil
	for _, child in tree.children(parent) do
		if child.Name == name then
			if not SCRIPT_CLASSES[tree.className(child)] then
				return child
			end
			script = script or child
		end
	end
	return script
end

function Applier._ensureParent(self, segments: { string }): Node
	local tree = self.tree
	local parent = tree.root
	for index = 1, #segments - 1 do
		local name = segments[index]
		local existing = self:_container(parent, name)
		if existing == nil then
			existing = tree.createInstance("Folder", name)
			tree.setParent(existing, parent)
		end
		parent = existing
	end
	return parent
end

function Applier._findParent(self, segments: { string }): Node?
	local parent = self.tree.root
	for index = 1, #segments - 1 do
		local existing = self:_container(parent, segments[index])
		if existing == nil then
			return nil
		end
		parent = existing
	end
	return parent
end

local function adopt(tree: Tree, node: Node, stale: { Node })
	for _, old in stale do
		for _, grandchild in tree.children(old) do
			tree.setParent(grandchild, node)
		end
		tree.destroy(old)
	end
end

function Applier._initNode(self, parent: Node, dirName: string, class: string): Node
	local tree = self.tree
	local matching: Node? = nil
	local folders: { Node } = {}
	for _, child in tree.children(parent) do
		if child.Name == dirName then
			if tree.className(child) == class then
				matching = matching or child
			elseif tree.className(child) == "Folder" then
				table.insert(folders, child)
			end
		end
	end
	local node = matching
	if node == nil then
		node = tree.createInstance(class, dirName)
		tree.setParent(node, parent)
	end
	adopt(tree, node, folders)
	return node
end

function Applier._demoteInit(self, parent: Node, dirName: string, class: string)
	local tree = self.tree
	for _, child in tree.children(parent) do
		if child.Name == dirName and tree.className(child) == class then
			local folder = tree.createInstance("Folder", dirName)
			tree.setParent(folder, parent)
			adopt(tree, folder, { child })
		end
	end
end

function Applier._scriptNode(self, parent: Node, leafName: string, class: string): Node
	local tree = self.tree
	local matching: Node? = nil
	local stale: { Node } = {}
	for _, child in tree.children(parent) do
		if child.Name == leafName and SCRIPT_CLASSES[tree.className(child)] then
			if tree.className(child) == class then
				matching = matching or child
			else
				table.insert(stale, child)
			end
		end
	end
	local node = matching
	if node == nil then
		node = tree.createInstance(class, leafName)
		tree.setParent(node, parent)
	end
	adopt(tree, node, stale)
	return node
end

function Applier._write(self, path: string, node: Node, content: string)
	local echo = self.echo
	if echo ~= nil then
		echo.expect(path, content)
	end
	local ok, err = pcall(self.tree.setSource, node, content)
	if not ok then
		if echo ~= nil then
			echo.forget(path)
		end
		error(err, 0)
	end
end

function Applier._applyInit(self, segments: { string }, class: string, patch: Patch)
	local tree = self.tree
	table.remove(segments)
	local dirName = segments[#segments]
	if patch.kind == "Delete" then
		local parent = self:_findParent(segments)
		if parent ~= nil then
			self:_demoteInit(parent, dirName, class)
		end
		return
	end
	local parent = self:_ensureParent(segments)
	local node = self:_initNode(parent, dirName, class)
	local content = patch.content
	if content ~= nil and tree.getSource(node) ~= content then
		self:_write(patch.path, node, content)
	end
end

function Applier.apply(self, patch: Patch)
	local tree = self.tree
	local segments = splitPath(patch.path)
	if #segments == 0 then
		return
	end
	local fileName = segments[#segments]
	local initClass = INIT_CLASSES[fileName]
	if initClass ~= nil and #segments >= 2 then
		self:_applyInit(segments, initClass, patch)
		return
	end
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
	local node = self:_scriptNode(parent, leafName, expectedClass(fileName, patch.class))
	local content = patch.content
	if content ~= nil and tree.getSource(node) ~= content then
		self:_write(patch.path, node, content)
	end
end

function Applier.applyBatch(self, patches: { Patch })
	for _, patch in patches do
		self:apply(patch)
	end
end

function Apply.studioTree(root: Node): Tree
	return {
		root = root,
		createInstance = function(className: string, name: string): Node
			local instance = Instance.new(className :: any)
			instance.Name = name
			return instance
		end,
		children = function(parent: Node): { Node }
			return parent:GetChildren()
		end,
		className = function(node: Node): string
			return node.ClassName
		end,
		getSource = function(node: Node): string
			return node.Source
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
