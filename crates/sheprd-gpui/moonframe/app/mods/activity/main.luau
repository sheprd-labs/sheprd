return mod {
	init = function(previous)
		return previous or {}
	end,
	update = function() end,
	view = function(_, app)
		local editor = app.acp and (app.sheprd or {}).moonframe_tools
		local name = if editor then "activity_editor" else "activity"
		return ui.column {
			grow = true,
			gap = "none",
			ui.view { name = "sheprd", id = name, props = { region = name } },
			-- The shared bar reserves one 32-point button above Settings for the editor.
			if editor then ui.column {
				float = { bottom = 42, left = 5 },
				width = 32, height = 32, align = "center", justify = "center",
				ui.mount "feedback",
			} else false,
		}
	end,
}
