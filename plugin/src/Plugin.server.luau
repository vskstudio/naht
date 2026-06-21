--!strict
-- The Naht Studio plugin entry point: a toolbar toggle and a small status widget over the
-- connection loop. All transport and apply logic lives in the sibling modules; this script is just
-- wiring and a view.

local ServerStorage = game:GetService("ServerStorage")

local Client = require(script.Parent.Client)
local Connection = require(script.Parent.Connection)

-- Where synced source lives in the DataModel, and the daemon to talk to. The full configurable
-- mapping lands later; for v1 the project tree mirrors under one folder.
local BASE_URL = "http://localhost:34872"
local ROOT_FOLDER = "Naht"

local STATE_COLOR = {
	Connecting = Color3.fromRGB(220, 180, 60),
	Connected = Color3.fromRGB(80, 200, 120),
	Reconnecting = Color3.fromRGB(220, 140, 60),
	Conflict = Color3.fromRGB(220, 80, 80),
	Disconnected = Color3.fromRGB(150, 150, 150),
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

local connection: any = nil

local function setState(state: string, detail: string?)
	local color = STATE_COLOR[state] or Color3.fromRGB(200, 200, 200)
	label.TextColor3 = color
	label.Text = if detail ~= nil then string.format("%s\n%s", state, detail) else state
end

local function connect()
	local root = ensureRoot()
	local client = Client.new(BASE_URL)
	connection = Connection.new(client, root, game.PlaceId, setState)
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
