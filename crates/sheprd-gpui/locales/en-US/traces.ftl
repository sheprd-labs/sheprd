# Traces: every agent session on this computer, with messages per day by
# agent, a filter and search, and one session's events. Agent, host, model,
# and folder names are shown as they are.

traces-title = Traces
traces-subtitle = Agent sessions across your connected computers, kept here for offline access
traces-search = Search sessions, folders, branches, models
traces-starting = Reading agent sessions…
traces-failed = Traces could not read agent sessions: { $error }
traces-none = No sessions match. Widen the range or clear the search.
traces-more = { $count ->
        [one] { $count } more session; narrow the filter to see it
       *[other] { $count } more sessions; narrow the filter to see them
    }
traces-untitled = Untitled session
traces-close = Close

# The range and agent filters.
traces-range-week = 7 days
traces-range-month = 30 days
traces-range-quarter = 90 days
traces-range-year = 1 year
traces-agent-all = All agents
traces-agent-claude = Claude Code
traces-agent-codex = Codex
traces-agent-pi = Pi
traces-agent-omp = omp
traces-all-hosts = All hosts
traces-labels = Labels
traces-hosts = Hosts
traces-host-disambiguated = { $host } · { $id }

# The totals over the sessions shown.
traces-tile-sessions = Sessions
traces-tile-hosts = { $count ->
        [one] on { $count } computer
       *[other] on { $count } computers
    }
traces-tile-messages = Messages
traces-tile-tools = Tool calls
traces-tile-errors = { $count ->
        [one] { $count } error
       *[other] { $count } errors
    }
traces-tile-tokens = Tokens

# The chart.
traces-chart-title = Messages per day
traces-activity-loading = Reading this selection’s activity…
traces-activity-failed = Could not read this selection’s activity: { $error }
traces-chart-summary = { $messages } messages in { $days } days

# The table's columns.
traces-column-updated = Updated
traces-column-session = Session
traces-column-host = Computer
traces-column-model = Model
traces-column-messages = Msgs
traces-column-tools = Tools
traces-column-tokens = Tokens

# One session's facts and events.
traces-fact-agent = Agent
traces-fact-host = Computer
traces-fact-model = Model
traces-fact-folder = Folder
traces-fact-branch = Branch
traces-fact-started = Started
traces-fact-tokens = Tokens
traces-fact-tokens-value = { $input } in · { $output } out
traces-events-loading = Reading events…
traces-events-failed = Could not read this session's events: { $error }
traces-events-none = No events yet
traces-event-user = You
traces-event-agent = Agent
traces-event-thinking = Thinking
traces-event-tool = Tool
traces-event-result = Result
traces-event-error = Error

traces-replication-summary = { $count ->
        [0] No connected hosts to synchronize. Previously replicated sessions remain available.
        [one] Sharing traces both ways with { $count } connected host, including traces learned from other hosts.
       *[other] Sharing traces both ways with { $count } connected hosts, including traces learned from other hosts.
    }
traces-replication-waiting = Waiting to synchronize
traces-replication-syncing = Synchronizing…
traces-replication-ready = Up to date at the last exchange
traces-replication-failed = Could not synchronize: { $error }. Check the connection and the host’s sheprd-acp version.
traces-replication-peer = { $host }: { $status }

traces-overlap-title = Agents running at once
traces-overlap-recent = { $count } active in the last 15 min
traces-overlap-peak = Peak { $count } · { $time }
traces-overlap-rule = Daily peak. Each session counts for 15 minutes after a reply, reasoning event, or tool call. Select a peak to see its sessions.
traces-overlap-selection = Sessions active at { $time }
traces-overlap-clear = Show all sessions
traces-overlap-agent = { $count } { $agent }
