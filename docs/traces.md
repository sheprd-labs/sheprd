# Traces

Open **Traces** in the activity bar, or run `sheprd ui panel traces`, to
browse Claude Code, Codex, Pi, and omp session history from this computer and
its connected hosts, including histories forwarded by those hosts.
The page shows session totals, messages per UTC day, agent, host, label, and date filters,
and a search over session titles, folders, branches, models, and computer names.
Select a session to read its prompts, replies, thinking, tool calls, and results.

Choose **All hosts**, one computer, or an existing device label such as **work**
or **personal**. The same source selection applies to the session table, totals,
and daily chart, alongside the agent and date filters. Label matching is
case-insensitive. Computers with the same name are distinguished by their stable
IDs, and hosts remain selectable while offline.

The labels are the device tags assigned in Sheprd's device menu; there is no
separate set of Traces labels. The index keeps their latest known values and
replicates updates and clears, so an indirectly learned host retains its labels
too. A route is bound to a host ID by its authenticated replication handshake,
never by comparing host names. Filtered charts are read on a worker, one query
per window at a time, with only the latest selection pending.

The app indexes native JSONL session files on a background thread and follows
changes as agents write. It reads Claude's `~/.claude/projects`, Codex's
`~/.codex/sessions` and `~/.codex/archived_sessions`, Pi's
`~/.pi/agent/sessions`, and omp's `~/.omp/agent/sessions`.
`CLAUDE_CONFIG_DIR` and `CODEX_HOME` replace their default base directories;
Codex's active and archived sessions are both included. `PI_CODING_AGENT_DIR`
overrides Pi/omp's agent directory, and omp also honors `PI_CONFIG_DIR` beneath
the home directory when no direct agent-directory override is set. Relative
direct overrides resolve from the app/helper's working directory; empty values
use defaults. The desktop reads its resolved login environment on the worker.
When Pi and omp share an override directory, it is indexed once using their
shared Pi-compatible format, rather than duplicating its sessions.
The index is private to the host's user account. Connected, authenticated hosts
exchange its redacted session records automatically; previously replicated
sessions remain readable when their source is offline. Nothing is published
to a public server.

## Agents running at once

The first chart shows the highest number of sessions active at the same instant
on each UTC day. A timestamped reply, reasoning event, or tool call counts its
session for the next 15 minutes. Another such event extends the window. This
estimate includes activity across midnight and expires while a session is idle.
Counts reflect indexed traces, including the last history received from offline hosts.
A subagent contributes separately only when Traces indexes it as its own session.

The recent count is broken down by agent type and refreshes about every 30 seconds.
Select the recent count, the peak badge, or a day in the chart to see the
participating sessions and open their transcripts. A selected daily peak keeps
its timestamp if newer trace history moves the peak. Tied peaks use the earliest
instant. The chart follows the host, device label, agent, and date filters.
Search narrows the session list. **Show all sessions** closes the drill-down.
The drill-down reads up to 1,000 participants directly from the index, including
older sessions outside the regular recent-session listing.

## Replication

Every running Sheprd exchanges traces in both directions with the enabled hosts
it is connected to, across all its windows. This uses the same authenticated
host-command transport as other host operations: SSH (including paired desktops
reached through relays), supported cloud machines, and WSL distributions.
Each host needs a compatible `sheprd-acp` (or legacy `herdr-acp`) on its command
path. A missing or older helper is reported on the Traces page; it is not
automatically installed or upgraded. Remote host commands currently require a
POSIX shell; a Windows OpenSSH host without one cannot participate through this
transport.

Replication includes records learned from other hosts. With A connected to B
and C, B's traces reach C through A even if B and C have no direct connection.
Only an existing connection is used: Sheprd never discovers or dials a new host
just because its identity appears in a trace. Four host jobs run at most, one
per machine route, with small batches and durable acknowledgements. A caught-up
host is checked every 30 seconds; catch-up continues at one-second intervals,
and failures retry after 60 seconds. Disconnecting a host cancels its job and
retains data already committed there.

Each database keeps a persistent replica identity and ordered change journal.
Origin/sequence deduplication stops forwarding loops from multiplying events or
totals. File replacements get a new logical generation, so an older peer cannot
resurrect the replaced events. Lost replies replay safely. A rebuilt peer starts
from zero; rebuilding a source index preserves its current native files over
older replicated contents. Schema-1 indexes are migrated and their existing
history is made available for replication.

The new schema is version 3 (replication protocol 2); older builds refuse it rather than guessing its
contents. The UI reports waiting, synchronizing, caught-up, and failed states
for connected routes; `sheprd ui traces --json` reads the same prepared status.
An incoming exchange from another process becomes visible
on the receiving app's next local watcher refresh (at most 30 seconds while idle).
The replication journal currently has no compaction or retention policy.

Event text and titles pass through the diagnostics redactor before storage.
Structured tool data and JSON-encoded arguments are scrubbed recursively before
being shortened; source-file opens reject symlinks and linked directories within
the session roots. Periodic catch-up scans still run while notifications are busy.
Events are limited to 4 KB in the index and shortened to 600 bytes in the view;
the original files remain with the agent. The page reads up to 1,000 recent
sessions, draws up to 200 matching rows, and shows the first 400 events of a
selected session. Search filters session metadata, not the full transcript.
Totals cover the matching sessions' lifetime counts; the chart covers messages
in the selected date range and agent, independently of the text search.

## Host command line

The companion can maintain and query the same index without the GUI:

```sh
sheprd-acp traces index --json
sheprd-acp traces watch --json
sheprd-acp traces list --agent claude --limit 50 --json
sheprd-acp traces activity --days 30 --json
sheprd-acp traces overlap --days 30 --json
sheprd-acp traces overlap --at UNIX_MILLISECONDS --agent codex --host HOST_ID --json
sheprd-acp traces peers --json
sheprd ui traces --json
sheprd ui traces --tag work
sheprd ui traces --host HOST_ID
sheprd ui traces --all
```

`index` reads one bounded batch per file; repeat it or use `watch` to catch up
with very large files. `list`, `activity`, and `overlap` read the existing index.
`overlap` covers 1–365 UTC days. Its JSON contains `recent` and `daily` points
with Unix millisecond `at` and session `count`, plus `recent_by_agent`. Pass a point’s `at` to list its sessions.
Without `--json`, the commands print human-readable summaries.
`watch --json` emits a host record, changed session records, and quiet-rescan
heartbeats as JSON lines. It runs until stopped or its output reader closes.

`peers` reports durable cursor progress and the last acknowledged exchange for
routes this index initiated. `sheprd-acp traces exchange` is the host-command
protocol endpoint: it reads one bounded JSON request from stdin and writes one
reply to stdout. It has no listening socket and relies on its caller's existing
authenticated transport. Requests and replies are capped at 2 MiB; journal
batches are capped at 64 records and 512 KiB of serialized record data.

## Still missing

- Full-text transcript search: the FTS index exists, but the UI searches only
  session metadata.
- Pagination beyond the displayed session/event limits.
- Retention, journal compaction, and a delete-everywhere operation with durable
  deletion records. Disconnecting or deleting a source file does not remove
  copies already replicated to another host.
- Per-session replication exclusions, public/link sharing, and additional agent
  formats such as OpenCode and T3.
- Standalone background synchronization when no Sheprd app is running to
  coordinate connected hosts. `watch` indexes files; it does not discover peers.

## Native screenshots

[Wide concurrency chart](screenshots/traces/overlap-wide.png) · [Narrow concurrency chart](screenshots/traces/overlap-narrow.png)

The opt-in fixture uses synthetic data and the real native window, with the
shared test sandbox. With a desktop available, capture dark, light, narrow,
and session-detail views:

```sh
HERDR_TEST_TRACES_CAPTURE=/absolute/existing/directory/traces \
  cargo test --locked -p sheprd-gpui --features integration-test,mockup \
  --test live_gui native_traces -- --ignored --nocapture --test-threads=1
```

This exercises native rendering; it does not verify compositor-delivered
keyboard input or IME.

On Linux, captures use X11 and require Python's Pillow and python-xlib packages.
Unset `WAYLAND_DISPLAY` for the command and supply the desktop's `DISPLAY` and
`XAUTHORITY` if running over SSH. Only the fixture process's window is captured.
