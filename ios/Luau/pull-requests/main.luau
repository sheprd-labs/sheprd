-- Sheprd's Pull Requests tab, in Luau: the pull requests the github.com
-- account wrote, and those asking for its review, each saying where its
-- branch is checked out. Review requests come first while there are any.
--
-- It reads `app.pulls`, which the app publishes from the same model as the
-- SwiftUI tab (ios/SheprdRemote/Luau/PullRequestsProvider.swift), already
-- searched and sorted. It draws, and asks `provider:pulls` for the rest.
-- Started from moonswift's examples/sheprd.
local row = use "row"

type State = {}

-- A section's header: its title, and how many of all it shows.
local function header(title: string, shown: number, total: number)
	return ui.row {
		ui.text { title },
		ui.spacer {},
		total > shown and ui.text { shown .. " of " .. total, digits = true } or false,
	}
end

-- Its dot on the edge index, in the agents' palette, as PullRequestsList
-- gives it: red when it needs the person (changes asked for, a check
-- failing), yellow while checks run, teal once approved, green otherwise.
local function dot(pull): string
	if not pull then
		return "secondary"
	elseif pull.review == "changesRequested" or pull.checks.failing > 0 then
		return "red"
	elseif pull.checks.pending > 0 then
		return "yellow"
	elseif pull.review == "approved" then
		return "teal"
	end
	return "green"
end

-- A section: its header, its rows drawn as they scroll in, and its letter
-- and dots on the edge index.
local function section(key: string, title: string, keys, total: number, pulls)
	if #keys == 0 then
		return false
	end
	local dots = {}
	for i, id in keys do
		dots[i] = dot(pulls[id])
	end
	return ui.section {
		key = key,
		header = header(title, #keys, total),
		count = #keys,
		row = function(i)
			local pull = pulls[keys[i]]
			if not pull then
				return ui.text { "" }
			end
			return row.view(pull, key == "requested")
		end,
		edge = { letter = string.upper(string.sub(title, 1, 1)), dots = dots },
	}
end

local function loading()
	return ui.row {
		gap = 8,
		ui.spacer {},
		ui.progress {},
		ui.text { "Loading pull requests…", style = { color = "secondary" } },
		ui.spacer {},
	}
end

-- What shows over the list when it has no rows to show.
local function empty(pulls)
	if pulls.any and #pulls.mine + #pulls.requested == 0 then
		return ui.unavailable { variant = "search", query = pulls.query }
	elseif pulls.loading and not pulls.any then
		return loading()
	elseif pulls.status == "noAccount" then
		return ui.unavailable {
			"No GitHub account",
			icon = "arrow.triangle.pull",
			description = "Sign in to GitHub on this "
				.. pulls.device
				.. ", or let a paired computer lend its sign-in.",
			ui.button { "Open Settings › GitHub", variant = "primary", msg = "github" },
		}
	elseif pulls.status == "unread" then
		return loading()
	elseif pulls.status == "read" and not pulls.any then
		return ui.unavailable {
			"No open pull requests",
			icon = "arrow.triangle.pull",
			description = "Pull requests you opened, and those asking for your review, appear here.",
		}
	elseif pulls.status == "failed" and not pulls.any then
		return ui.unavailable {
			"GitHub could not be read",
			icon = "exclamationmark.triangle",
			description = "Pull down to try again.",
		}
	end
	return nil
end

-- From this many pull requests the list has an index down its edge.
local INDEXED_FROM = 12

return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function(state: State, msg)
		if msg == "refresh" then
			return state, { { to = "provider:pulls", msg = { refresh = true } } }
		elseif msg == "github" then
			return state, { { to = "provider:pulls", msg = { github = true } } }
		elseif type(msg) == "table" and msg.msg == "query" then
			return state, { { to = "provider:pulls", msg = { query = msg.value } } }
		elseif type(msg) == "table" and msg.msg == "sort" then
			return state, { { to = "provider:pulls", msg = { sort = msg.value } } }
		elseif type(msg) == "table" and ((msg.device and msg.workspace) or msg.web) then
			return state, { { to = "provider:pulls", msg = msg } }
		end
		return state
	end,

	view = function(state: State, app)
		-- Empty until the app publishes, as when checked headlessly.
		local given = app.pulls or {}
		local pulls = {
			status = given.status or "unread",
			loading = given.loading or false,
			refreshing = given.refreshing or false,
			query = given.query or "",
			sort = given.sort or "updated",
			device = given.device or "iPhone",
			any = given.any or false,
			requested = given.requested or {},
			mine = given.mine or {},
			requestedTotal = given.requestedTotal or 0,
			mineTotal = given.mineTotal or 0,
			rows = given.rows or {},
		}
		local list = {
			variant = "inset",
			edge = #pulls.requested + #pulls.mine >= INDEXED_FROM,
			titleMode = "inline",
			refresh = "refresh",
			refreshing = pulls.refreshing,
			search = {
				value = pulls.query,
				change = "query",
				prompt = "Title, repository, author, branch",
				placement = "always",
			},
			toolbar = {
				ui.menu {
					icon = "arrow.up.arrow.down",
					place = "leading",
					ui.picker {
						"Sort Pull Requests",
						value = pulls.sort,
						change = "sort",
						options = {
							{ value = "updated", label = "Recently Updated", icon = "clock" },
							{ value = "status", label = "Status", icon = "exclamationmark.circle" },
						},
					},
				},
			},
		}
		local overlay = empty(pulls)
		if overlay then
			list.overlay = { place = "center", overlay }
		end
		if pulls.status == "failed" and pulls.any then
			table.insert(
				list,
				ui.section {
					key = "failed",
					ui.label {
						"GitHub could not be read. These may be out of date.",
						icon = "exclamationmark.triangle",
						style = { font = "footnote", color = "secondary" },
					},
				}
			)
		end
		table.insert(list, section("requested", "Review requested", pulls.requested, pulls.requestedTotal, pulls.rows))
		table.insert(list, section("mine", "Mine", pulls.mine, pulls.mineTotal, pulls.rows))
		return ui.list(list)
	end,
}
