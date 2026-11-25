-- Keep the title and list independently placeable in custom layouts.
return mod {
	init = function(previous)
		return previous or {}
	end,
	update = function() end,
	view = function(_, app)
		local sheprd = app.sheprd or {}
		local height = sheprd.sidebar_title_height or 0
		return ui.column {
			grow = true,
			gap = "none",
			if height > 0 then ui.row {
				height = height, shrink = false,
				ui.view { name = "sheprd", id = "sidebar_title", props = { region = "sidebar_title" } },
			} else false,
			ui.view { name = "sheprd", id = "worktrees", props = { region = "worktrees" } },
		}
	end,
}
