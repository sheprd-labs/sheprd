-- Orca marks shared by the worktree and agent lists. Use native drawing:
-- an image in a stateless component would resolve in the caller's mod.

type Props = {
	status: string?,
	theme: any,
	height: number?,
}

-- Orca's question bubble, in a 14px box. The three strokes inherit the
-- theme color together; the tail distinguishes it from the ringed check.
local QUESTION = {
	{ stroke = 1.4, radius = 1, points = { 4.6, 11.7, 1.5, 12.5, 2.3, 9.4, 1.4, 7, 1.8, 4.6, 3.2, 2.6, 5.4, 1.5, 7.8, 1.4, 10.1, 2.4, 11.8, 4.3, 12.4, 6.7, 12, 9, 10.4, 11, 8, 12.1, 6.3, 12.2, 4.6, 11.7 } },
	{ stroke = 1.4, radius = 1, points = { 5.2, 5.4, 5.8, 4.3, 7.1, 4, 8.4, 4.4, 8.8, 5.6, 8.4, 6.6, 7, 7.5, 7, 8 } },
	{ stroke = 1.6, points = { 7, 9.3, 7, 10.5 } },
}

local function mark(status: string, theme: any)
	local tint = theme.status and theme.status[status]
	if status == "working" then
		return ui.icon { "loader-circle", spin = true, style = { color = tint or theme.warning or "warning" } }
	elseif status == "blocked" then
		return ui.paths { width = 14, height = 14, paths = QUESTION, style = { color = tint or theme.danger or "danger" } }
	elseif status == "done" then
		return ui.icon { "circle-check", style = { color = tint or theme.success or "success" } }
	end
	local idle = status == "idle"
	local size = if idle then 6 else 3
	return ui.row {
		width = size, height = size, shrink = false,
		style = { background = tint or theme.muted or "muted-foreground", radius = size / 2, opacity = if idle then 0.4 else 1 },
	}
end

return component {
	title = "Sheprd Status Dot",
	category = "Sheprd",
	icon = "circle",
	description = "Orca agent status: a rotating ring while working, a question bubble when blocked, a ringed check when done, a quiet dot when idle, a speck when unknown.",
	when = "Before a worktree or agent row's label, so every label starts at the same place.",
	props = {
		["status"] = "\"idle\", \"working\", \"blocked\", \"done\" or \"unknown\"",
		["theme"] = "`app.sheprd_theme`, for the colors",
		["height"] = "the line's height; the mark is centered in it (16)",
	},
	example = { status = "working", theme = {} },

	build = function(props: Props)
		-- Sidebar continuation lines reserve 8px plus the row gap. Center
		-- the non-shrinking glyph on that column; do not shift the labels.
		return ui.row {
			class = "status-slot",
			width = 8,
			height = math.max(props.height or 16, 16),
			shrink = false,
			align = "center",
			justify = "center",
			mark(props.status or "unknown", props.theme or {}),
		}
	end,
}
