return mod {
	init = function(previous)
		return previous or {}
	end,
	update = function() end,
	view = function()
		return ui.view { name = "sheprd", id = "devices", props = { region = "devices" } }
	end,
}
