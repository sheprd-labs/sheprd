-- The draggable native header shows navigation and the sidebar toggle where they fit.
return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function()
		return ui.view { name = "sheprd", id = "header", props = { region = "header" } }
	end,
}
