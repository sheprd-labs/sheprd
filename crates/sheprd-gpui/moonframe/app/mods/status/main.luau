-- The status bar: the connection, then the machine's load and quick commands.
local meter = use "meter"

local function run(command: string)
	return { command = command }
end

-- Each usage provider's logo among Moonframe's bundled ones, which cannot
-- draw Sheprd's own (`assets/icons/providers`); a robot for the rest.
local LOGOS = {
	amp = "agent-amp",
	claude = "agent-claude",
	codex = "agent-codex",
	copilot = "agent-copilot",
	cursor = "agent-cursor",
	factory = "agent-droid",
	gemini = "agent-gemini",
	grok = "agent-grok",
	kiro = "agent-kiro",
	mistral = "brand-mistral",
	ollama = "brand-ollama",
	opencode = "agent-opencode",
	qwencloud = "agent-qwen",
	xai = "agent-grok",
}

-- One provider's usage, as Sheprd's own bar draws it: its logo, a meter of
-- the window closest to its limit, and the tightest windows' shares, each
-- in its severity's color (`app.sheprd.usage`, src/usage/hosted.rs). A
-- click opens its panel.
local function usage_segment(segment: any, theme: any)
	local row: any = {
		class = "usage-segment",
		align = "center",
		shrink = false,
		tooltip = segment.name,
		msg = { usage = segment.id },
		to = "provider:sheprd",
		style = { gap = 6, padding = { x = 6 }, radius = 4 },
		ui.icon { name = LOGOS[segment.id] or "bot", style = { color = theme.foreground } },
	}
	if segment.locked then
		table.insert(row, ui.icon { name = "lock", size = "small", style = { color = theme.muted } })
	end
	if segment.meter then
		table.insert(row, ui.row {
			width = 40,
			height = 5,
			shrink = false,
			style = { background = segment.meter.track, radius = 3 },
			ui.row { width = 40 * segment.meter.used, height = 5, style = { background = segment.meter.color, radius = 3 } },
		})
	end
	for index, label in segment.labels or {} do
		if index > 1 then
			table.insert(row, ui.text { "·", style = { color = theme.muted } })
		end
		table.insert(row, ui.text { label.text, style = { color = label.color } })
	end
	if segment.balance then
		table.insert(row, ui.text { segment.balance })
	end
	-- The numbers are the last good ones; the panel says why.
	if segment.stale then
		table.insert(row, ui.text { "!", style = { color = segment.stale_color } })
	end
	return ui.row(row)
end

-- The usage segments, then the button that reads them again; a host that
-- could not be read says so while there is nothing older.
local function usage(state: any, theme: any)
	local row: any = { class = "sheprd-usage", align = "center", shrink = false, style = { gap = 4 } }
	for _, segment in state.segments do
		table.insert(row, usage_segment(segment, theme))
	end
	if state.error then
		table.insert(row, ui.text { state.unavailable, tooltip = state.error, style = { color = theme.muted } })
	end
	table.insert(row, ui.row {
		class = "usage-refresh",
		align = "center",
		justify = "center",
		width = 18,
		height = 18,
		shrink = false,
		tooltip = state.refresh_label,
		msg = { usage_refresh = true },
		to = "provider:sheprd",
		style = { radius = 4 },
		ui.icon { name = "refresh-cw", size = "small", spin = state.busy == true, style = { color = theme.muted } },
	})
	return ui.row(row)
end

local function item(icon: string, text: string, color: string, msg: any, to: string?)
	return ui.row {
		class = "status-item",
		align = "center",
		shrink = false,
		msg = msg,
		to = to,
		style = { gap = 5 },
		-- As Sheprd's own bar: text at the UI size, icons a size up.
		ui.icon { name = icon, size = "large", style = { color = color } },
		ui.text { text, style = { color = color } },
	}
end

return mod {
	-- The meters' text at the UI size too, over the component's small one.
	styles = {
		[".meter-label"] = { size = "normal" },
		[".meter-percent"] = { size = "normal" },
		[".usage-segment:hover"] = { background = "secondary" },
		[".usage-refresh:hover"] = { background = "secondary" },
	},

	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function(_, app)
		local sheprd = app.sheprd or { agents = {} }
		local load = app.sheprd_load or {}
		local working = 0
		for _, agent in sheprd.agents do
			if agent.status == "working" then
				working += 1
			end
		end
		local theme = app.sheprd_theme or {}
		return ui.row {
			class = "sheprd-status",
			grow = true,
			align = "center",
			style = { background = theme.surface, gap = 14, padding = { x = 12 } },
			if sheprd.connected == false then item("circle-alert", sheprd.status, theme.danger) else false,
			if working > 0 then item("bot", `{working} working`, theme.accent) else false,
			if sheprd.usage then usage(sheprd.usage, theme) else false,
			ui.spacer(),
			if load.cpu then meter { label = "CPU", value = load.cpu / 100, width = 40, warn = 0.8 } else false,
			if load.memory then meter { label = "MEM", value = load.memory / 100, width = 40, warn = 0.8 } else false,
			item("panel-right", "Details", theme.subtext, "toggle-details", "shell"),
			item("palette", "Theme", theme.subtext, run "themes", "provider:sheprd"),
			item("command", "Commands", theme.subtext, run "command_palette", "provider:sheprd"),
		}
	end,
}
