--!strict

local ServerStorage = game:GetService("ServerStorage")

local Client = require(script.Parent.Client)
local Apply = require(script.Parent.Apply)
local Connection = require(script.Parent.Connection)
local MessagePack = require(script.Parent.MessagePack)
local Protocol = require(script.Parent.Protocol).new(MessagePack)
local Terrain = require(script.Parent.Terrain)

local BASE_URL = "http://localhost:34872"
local ROOT_FOLDER = "Naht"
local TOKEN_SETTING_PREFIX = "naht.token."

local STATE_COLOR = {
	Connecting = Color3.fromRGB(220, 180, 60),
	Connected = Color3.fromRGB(80, 200, 120),
	Reconnecting = Color3.fromRGB(220, 140, 60),
	Conflict = Color3.fromRGB(220, 80, 80),
	Warning = Color3.fromRGB(230, 120, 40),
	TokenRequired = Color3.fromRGB(200, 120, 230),
	Disconnected = Color3.fromRGB(150, 150, 150),
}

local tokenStore = {
	load = function(projectId: string): string?
		local stored = plugin:GetSetting(TOKEN_SETTING_PREFIX .. projectId)
		if type(stored) == "string" and stored ~= "" then
			return stored
		end
		return nil
	end,
	save = function(projectId: string, token: string)
		plugin:SetSetting(TOKEN_SETTING_PREFIX .. projectId, token)
	end,
}

local function ensureRoot(): Folder
	local existing = ServerStorage:FindFirstChild(ROOT_FOLDER)
	if existing ~= nil and existing:IsA("Folder") then
		return existing
	end
	local folder = Instance.new("Folder")
	folder.Name = ROOT_FOLDER
	folder.Parent = ServerStorage
	return folder
end

local toolbar = plugin:CreateToolbar("Naht")
local toggleButton = toolbar:CreateButton("Naht Sync", "Connect to the Naht daemon", "")
toggleButton.ClickableWhenViewportHidden = true

local widget = plugin:CreateDockWidgetPluginGui(
	"NahtStatus",
	DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Right, false, false, 220, 90, 180, 60)
)
widget.Title = "Naht"

local label = Instance.new("TextLabel")
label.Size = UDim2.fromScale(1, 1)
label.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
label.TextColor3 = Color3.fromRGB(230, 230, 230)
label.Font = Enum.Font.Code
label.TextSize = 14
label.TextWrapped = true
label.Text = "Disconnected"
label.Parent = widget

local tokenBox = Instance.new("TextBox")
tokenBox.Size = UDim2.new(1, -8, 0, 24)
tokenBox.Position = UDim2.new(0, 4, 1, -28)
tokenBox.BackgroundColor3 = Color3.fromRGB(45, 45, 45)
tokenBox.TextColor3 = Color3.fromRGB(230, 230, 230)
tokenBox.PlaceholderText = "project token, then Enter"
tokenBox.PlaceholderColor3 = Color3.fromRGB(130, 130, 130)
tokenBox.ClearTextOnFocus = false
tokenBox.Font = Enum.Font.Code
tokenBox.TextSize = 14
tokenBox.Text = ""
tokenBox.Visible = false
tokenBox.Parent = widget

local connection: any = nil

local function setState(state: string, detail: string?)
	local color = STATE_COLOR[state] or Color3.fromRGB(200, 200, 200)
	local asksToken = state == "TokenRequired"
	label.TextColor3 = color
	label.Size = if asksToken then UDim2.new(1, 0, 1, -32) else UDim2.fromScale(1, 1)
	tokenBox.Visible = asksToken
	label.Text = if detail ~= nil then string.format("%s\n%s", state, detail) else state
end

tokenBox.FocusLost:Connect(function(enterPressed: boolean)
	local token = string.match(tokenBox.Text, "^%s*(.-)%s*$") or ""
	if not enterPressed or token == "" or connection == nil then
		return
	end
	tokenBox.Text = ""
	connection:submitToken(token)
end)

local function connect()
	local root = ensureRoot()
	local client = Client.new(BASE_URL, function(): number
		return game.PlaceId
	end, nil, tokenStore)
	connection = Connection.new(client, root, setState, {
		terrain = workspace.Terrain,
		tree = Apply.studioTree(root),
		apply = Apply,
		protocol = Protocol,
		terrainBlob = Terrain,
		history = game:GetService("ChangeHistoryService"),
	})
	connection:start()
	widget.Enabled = true
end

local function disconnect()
	if connection ~= nil then
		connection:stop()
		connection = nil
	end
end

toggleButton.Click:Connect(function()
	if connection == nil then
		connect()
		toggleButton:SetActive(true)
	else
		disconnect()
		toggleButton:SetActive(false)
	end
end)

plugin.Unloading:Connect(disconnect)
