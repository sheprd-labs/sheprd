return mod {
	init = function(previous)
		return previous or {}
	end,
	update = function() end,
	view = function(_, app)
		if not ((app.sheprd or {}).build or {}).worktree then
			return ui.row { height = 0 }
		end
		return ui.row { height = 22, shrink = false, ui.view { name = "sheprd", id = "build", props = { region = "build" } } }
	end,
}
