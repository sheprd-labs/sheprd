return mod {
	init = function(previous)
		return previous or {}
	end,
	update = function() end,
	view = function(_, app)
		local sheprd = app.sheprd or {}
		local height = (sheprd.sidebar_line_height or 16) + 2 * ((sheprd.layout or {}).footer_padding or 4)
		return ui.row { height = height, shrink = false, ui.view { name = "sheprd", id = "sidebar-actions", props = { region = "sidebar_actions" } } }
	end,
}
