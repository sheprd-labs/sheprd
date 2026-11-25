-- One row of the left pane's lists, its children stacked as its lines and
-- sized by them, highlighted as the configured layout
-- marks the selected row: flat layouts fill it edge to edge, rounded ones
-- (and Orca's cards) inset it, round it and outline it, and Superset adds a
-- stripe down its leading edge. Unselected rows keep a clear border, so
-- selecting one never moves its text.

type Props = {
	class: string?,
	selected: boolean?,
	host: any?,
	layout: any,
	theme: any,
	indent: number?,
	nest: number?,
	card: boolean?,
	stripe: boolean?,
	msg: any,
	context: any,
	carry: any,
	accept: any,
	drop: string?,
	to: string?,
	tooltip: string?,
}

-- `color` with no alpha, so a clear border keeps its place.
local function clear(color: string?): string?
	return if color then `{color}00` else nil
end

return component {
	title = "Sheprd Row",
	category = "Sheprd",
	icon = "rows-2",
	description = "A row of the left pane with its children stacked as its lines, highlighted as the configured sidebar layout marks the selected row.",
	when = "For each worktree or agent row, so every list follows the same layout.",
	props = {
		["class"] = "the row's own class, for hover rules and Inspect",
		["selected"] = "whether the row is the focused one",
		["host"] = "the row's device, with its wash and opacity",
		["layout"] = "`app.sheprd.layout`: spacing, inset, radius and highlight",
		["theme"] = "`app.sheprd_theme`, for the colors",
		["indent"] = "how far it steps in, for a worktree under its checkout",
		["nest"] = "how far its contents step in under a device's header, inside its highlight",
		["card"] = "draw it as a card (Orca) whatever the layout's highlight",
		["stripe"] = "mark the selected row with a stripe down its leading edge (Superset)",
		["msg"] = "what a click sends",
		["context"] = "what a right click sends, to open a menu",
		["carry"] = "what the row carries when dragged onto a row with `accept`",
		["accept"] = "what a row dragged over this one sends, with `phase`, `carry` and `y`",
		["drop"] = "\"before\" or \"after\": where a dragged row would land, drawn as a line",
		["to"] = "where the click goes",
		["tooltip"] = "text on hover",
	},
	example = { layout = { padding = 8, line = 16 }, theme = {}, selected = true },

	build = function(props: Props)
		local layout = props.layout or {}
		local theme = props.theme or {}
		local selected = props.selected == true
		local outline = props.card or layout.outline
		local inset = if props.card then 4 else layout.inset or 0
		local padding = layout.padding or 8
		local style: any = {
			-- A device's rows keep their highlight's edges and step their
			-- contents in, as Sheprd's own sidebar draws them.
			padding = {
				left = padding + (props.nest or 0),
				right = padding,
				y = (layout.row_padding or 0) + (layout.style_padding or 0),
			},
			margin = { left = inset + (props.indent or 0), right = inset, y = (layout.spacing or 0) / 2 },
		}
		if outline then
			style.radius = if props.card then layout.radius or 8 else layout.radius
			style.border = "all"
			style.border_color = if selected and props.card and theme.foreground then `{theme.foreground}2e` elseif selected then theme.muted else clear(theme.sidebar)
			style.background = if selected and theme.foreground then (if props.card then `{theme.foreground}1a` else `{theme.foreground}12`) else nil
		elseif props.stripe then
			style.border = "left"
			style.border_color = if selected then theme.accent else clear(theme.sidebar)
			style.background = if selected then theme.active else nil
		else
			style.background = if selected then theme.active else nil
		end
		-- Keep the host wash behind the text. On a flat row or an Orca card
		-- it overlays the highlight; rounded rows draw their highlight over it.
		local inner: any = nil
		if props.host then
			style.opacity = props.host.opacity
			if props.host.wash then
				local top = props.host.wash
				if outline and not props.card then
					top = style.background
					style.background = props.host.wash
				end
				inner = { gap = "none", style = { background = top, padding = style.padding, radius = style.radius } }
				style.padding = nil
			end
		end
		local row: any = {
			class = `sheprd-row {props.class or ""}`,
			selected = selected,
			gap = "none",
			shrink = false,
			msg = props.msg,
			context = props.context,
			carry = props.carry,
			accept = props.accept,
			to = props.to,
			tooltip = props.tooltip,
			style = style,
		}
		-- Where a dragged row would land, as a line across this one's edge.
		local content = inner or row
		local line = ui.row { class = "sheprd-row-drop", height = 2, shrink = false, style = { background = theme.accent } }
		if props.drop == "before" then
			table.insert(content, line)
		end
		for _, child in ipairs(props :: any) do
			table.insert(content, child)
		end
		if props.drop == "after" then
			table.insert(content, line)
		end
		if inner then
			table.insert(row, ui.column(inner))
		end
		return ui.column(row)
	end,
}
