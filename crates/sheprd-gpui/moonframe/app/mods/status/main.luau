-- Use the same status items, settings and actions as the classic window.
return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function()
		return ui.view { name = "sheprd", id = "status", props = { region = "status" } }
	end,
}
