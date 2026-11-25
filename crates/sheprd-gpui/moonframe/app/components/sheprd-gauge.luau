-- One gauge of a host's load, as Sheprd's status bar and host rows draw
-- it: its label, then a sparkline of recent samples (`history`) or a meter
-- (`meter`) when it has one, then its share in its severity's color. The
-- values and colors are Sheprd's own (`app.sheprd_load`, a device's `load`).

type Props = {
	class: string?,
	name: string,
	value: number?,
	color: string?,
	label_color: string?,
	history: any?,
	meter: any?,
	track: string?,
	label_width: number?,
	share_width: number?,
}

-- The graph's size, as Sheprd's own (src/system_load/render.rs).
local GRAPH, BAR, METER = 11, 2, 32

return component {
	title = "Sheprd Gauge",
	category = "Sheprd",
	icon = "activity",
	description = "A load gauge: a label, a sparkline of recent samples or a meter, and the share in its severity's color.",
	when = "For CPU, memory, GPU or video memory in the status bar or under a device's name.",
	props = {
		["class"] = "the row's class",
		["name"] = "the label, such as \"CPU\"",
		["value"] = "the share, as a percentage; none draws no number",
		["color"] = "the share's color",
		["label_color"] = "the label's color",
		["history"] = "the sparkline's bars, oldest first: `{ value, color }` with `value` as a fraction, or `false` before it is sampled",
		["meter"] = "the meter: `{ used, color }` with `used` as a fraction",
		["track"] = "the color of an empty bar and of the meter's track",
		["label_width"] = "a width for the label, so gauges on lines one under the other line up",
		["share_width"] = "a width for the share, set to the right, so it does not move as the number changes",
	},
	example = { name = "CPU", value = 42, meter = { used = 0.42, color = "#888888" }, track = "#333333" },

	build = function(props: Props)
		local label = ui.text { props.name, style = { color = props.label_color } }
		local row: any = {
			class = props.class,
			align = "center",
			shrink = false,
			style = { gap = 5 },
			if props.label_width then ui.row { width = props.label_width, shrink = false, label } else label,
		}
		if props.history then
			local bars: any = { class = "sheprd-sparkline", align = "end", height = GRAPH, shrink = false, style = { gap = 1 } }
			for _, bar in props.history do
				table.insert(bars, if bar
					then ui.row { width = BAR, height = math.max(GRAPH * bar.value, 1), shrink = false, style = { background = bar.color, radius = 0.5 } }
					else ui.row { width = BAR, height = 1, shrink = false, style = { background = props.track } })
			end
			table.insert(row, ui.row(bars))
		end
		if props.meter then
			table.insert(row, ui.row {
				width = METER,
				height = 5,
				shrink = false,
				style = { background = props.track, radius = 3 },
				ui.row { width = METER * props.meter.used, height = 5, style = { background = props.meter.color, radius = 3 } },
			})
		end
		if props.value then
			local share = ui.text { `{math.floor(props.value + 0.5)}%`, style = { color = props.color } }
			table.insert(row, if props.share_width
				then ui.row { width = props.share_width, shrink = false, justify = "end", share }
				else share)
		end
		return ui.row(row)
	end,
}
