-- A pull request's row: repository and number, title, review or author,
-- checks, and where its branch is checked out. It opens the pull request's
-- screen; its menu opens a workspace carrying it, or its page on GitHub.
local row = {}

-- A lifecycle as icon, word, and color.
local looks = {
	open = { symbol = "arrow.triangle.pull", label = "Open", color = "green" },
	draft = { symbol = "pencil.circle", label = "Draft", color = "secondary" },
	merged = { symbol = "arrow.triangle.merge", label = "Merged", color = "purple" },
	closed = { symbol = "xmark.circle", label = "Closed", color = "red" },
	unknown = { symbol = "arrow.triangle.pull", label = "Pull request", color = "secondary" },
}

-- A review decision, when somebody needs to review.
local reviews = {
	approved = { symbol = "checkmark.seal.fill", label = "Approved", color = "green" },
	changesRequested = { symbol = "exclamationmark.bubble.fill", label = "Changes requested", color = "red" },
	reviewRequired = { symbol = "eye", label = "Review required", color = "orange" },
}

local tints = { "indigo", "teal", "pink", "orange", "blue", "purple", "mint", "red", "cyan", "brown" }

-- A person's initial on a color of their own, the same for every login.
local function avatar(login: string, size: number)
	local sum = 0
	for _, code in utf8.codes(string.lower(login)) do
		sum = bit32.band(sum * 31 + code, 0xFFFF)
	end
	return ui.column {
		width = size,
		height = size,
		align = "center",
		gap = 0,
		style = { background = tints[sum % #tints + 1], radius = size / 2 },
		ui.spacer {},
		ui.text {
			string.upper(string.sub(login, 1, 1)),
			style = { size = size * 0.48, weight = "bold", design = "rounded", color = "white" },
		},
		ui.spacer {},
	}
end

-- An icon and its text side by side, as a caption.
local function iconText(text: string, icon: string, style)
	return ui.row {
		gap = 4,
		ui.icon { icon, style = style },
		ui.text { text, lines = 1, style = style },
	}
end

local function count(value: number, icon: string, color: string)
	return ui.row {
		gap = 3,
		ui.icon { icon, style = { font = "caption", weight = "semibold", color = color } },
		ui.text { tostring(value), digits = true, style = { font = "caption", weight = "semibold", color = "secondary" } },
	}
end

-- Failing, pending, and passed checks as small colored numbers.
local function checks(counts)
	return ui.row {
		gap = 8,
		counts.failing > 0 and count(counts.failing, "xmark.circle.fill", "red") or false,
		counts.pending > 0 and count(counts.pending, "clock.fill", "yellow") or false,
		counts.passed > 0 and count(counts.passed, "checkmark.circle.fill", "green") or false,
	}
end

-- Under the title: who wrote it, when they ask for a review, or where its
-- review stands.
local function byline(pull, showsAuthor: boolean)
	if showsAuthor and pull.author then
		return ui.row {
			gap = 8,
			avatar(pull.author, 16),
			ui.text { pull.author, lines = 1, style = { font = "caption", color = "secondary" } },
		}
	end
	local review = pull.review and reviews[pull.review]
	if review then
		return iconText(review.label, review.symbol, { font = "caption", weight = "medium", color = review.color })
	end
	return ui.text { (looks[pull.state] or looks.unknown).label, style = { font = "caption", color = "secondary" } }
end

local function whereLine(pull)
	local first = pull.places[1]
	if not first then
		return iconText("No workspace on any computer", "icloud", { font = "caption2", color = "tertiary" })
	end
	local others = pull.computers - 1
	local text = others == 0 and "Workspace on " .. first.computer
		or "Workspace on " .. first.computer .. " and " .. others .. " more"
	return iconText(text, "laptopcomputer", { font = "caption2", color = "secondary" })
end

local function menu(pull)
	local items = {}
	for _, place in pull.places do
		table.insert(items, ui.button {
			"Open Workspace on " .. place.computer,
			icon = "laptopcomputer",
			msg = { device = place.device, workspace = place.workspace },
		})
	end
	if pull.url then
		table.insert(items, ui.button { "Open on GitHub", icon = "safari", msg = { web = pull.id } })
		table.insert(items, ui.share { url = pull.url })
	end
	return items
end

function row.view(pull, showsAuthor: boolean)
	local look = looks[pull.state] or looks.unknown
	return ui.link {
		route = { pull = pull.id },
		context = menu(pull),
		ui.column {
			gap = 5,
			padding = { 2, 0 },
			ui.row {
				gap = 6,
				ui.icon { look.symbol, style = { font = "caption", weight = "semibold", color = look.color } },
				ui.text { pull.repository, lines = 1, style = { font = "caption", design = "monospaced", color = "secondary" } },
				ui.spacer {},
				ui.text { pull.age, style = { font = "caption", color = "secondary" } },
			},
			ui.text { pull.title, lines = 2 },
			ui.row {
				gap = 8,
				byline(pull, showsAuthor),
				ui.spacer {},
				checks(pull.checks),
			},
			whereLine(pull),
		},
	}
end

return row
