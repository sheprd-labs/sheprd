# Plan: Sheprd as an MCP server

Any MCP client (Claude Desktop, Claude Code, Codex, Cursor, Zed, and so on)
can use what only Sheprd shows or keeps: its windows and tabs, browser pages
and the user's notes on them, worktrees, pull request and CI status,
repository memory, checkpoints, ports, usage, and events. Today agents reach
this through the `sheprd` CLI (`docs/cli.md`) and the managed skill, and only
agents that can run shell commands and have loaded the skill use it. Read
`AGENTS.md` first; its rules apply throughout.

## Decisions

| Question | Decision |
| --- | --- |
| Gate | Both. A Cargo feature `mcp` on `sheprd-gpui`, in its `default` set so release builds include it, and a runtime switch `[features] mcp = false`, shown in Settings > General. A build without the Cargo feature has no MCP code, subcommand, or Settings row. |
| Default | Off. With the switch off, `sheprd mcp` still starts and answers `initialize`, but lists no tools and refuses every call. Its `instructions` say how to turn it on. |
| Transport | stdio only. The client starts `sheprd mcp`, which runs without GPUI, as every other `sheprd` command does, and turns each tool call into the control-socket request the matching CLI command sends. There is no new listener, port, token, or runtime. Streamable HTTP is out of scope (see Later). |
| Tools | A curated set with typed input schemas (below), not every CLI command. Each acts as its CLI command does: checkpoint restore, worktree create, fan-out, and teleport open the app's own dialog for the user to confirm; memory, notes, checkpoints taken, pages, and port forwards change at once. |
| Runtime | Blocking threads, no Tokio (`AGENTS.md`: no runtime around the blocking socket worker). MCP over stdio is newline-delimited JSON-RPC, which takes very little code to write by hand, so we add no `rmcp` (it requires Tokio). |

## Shape

```text
MCP client ──stdio──► sheprd mcp ──control socket──► running Sheprd app
                       │  (no GPUI)                    (existing handlers,
                       │                                dialogs, bounds)
                       └─ answers "no app needed" tools itself
                          (memory get, checkpoint list, ...)
```

### `crates/sheprd-mcp` (new, no GPUI, no runtime)

A sans-transport MCP server core, separate from Sheprd's own tools. It
justifies a crate because `sheprd-acp` will need the same core for the
servers `docs/memory-plan.md` (`sheprd-acp memory-mcp`) and
`docs/threads-plan.md` (`sheprd-acp thread-mcp`) plan, and `sheprd-acp`
cannot depend on `sheprd-gpui`.

- `jsonrpc`: request, notification, response, and error types; strict
  decoding. A batch (a JSON array) is refused, as MCP 2025-06-18 and later
  no longer allow batches.
- `protocol`: `initialize` with version negotiation (accept `2025-11-25`,
  `2025-06-18`, and `2025-03-26`, whose clients ignore the fields later
  revisions added; echo the client's version when supported, else the
  latest),
  `ping`, `tools/list`, `tools/call`, `notifications/initialized`, and
  `notifications/cancelled`. `serverInfo` gives `sheprd` and
  `sheprd_updater::APP_VERSION`.
- `tool`: `Tool { name, title, description, input_schema, annotations }` and
  `ToolResult { content, structured_content, is_error }`. Annotations carry
  `readOnlyHint`, `destructiveHint`, `idempotentHint`, and `openWorldHint`.
- `serve(info, input: impl BufRead + Send, output: impl Write + Send, handler: impl Handler)`,
  where `Handler` gives the tools, answers calls, and may give
  `instructions`. It is a trait because each server (`sheprd mcp`, the
  planned `sheprd-acp` ones, the tests) supplies all three together:
  - A reader thread reads stdin lines, refusing a line over
    `MAX_MESSAGE` (match the control protocol's 256 KiB) without buffering
    more of it. It answers `ping` itself, so a long call does not stall
    pings. It forwards `tools/call` to one worker through a bounded channel.
  - One call runs at a time, in arrival order, which keeps the control
    socket's single request in flight. A `notifications/cancelled` for the
    running call drops its reply, as the spec requires. The call is not
    aborted: control-socket waits are already capped by
    `MAX_WAIT_SECONDS`.
  - Output is written one line per message, and stdout is never used for
    anything else. Diagnostics go to stderr.
- `Error` and `Result` (`thiserror`), with variants for I/O, oversize,
  malformed JSON, and protocol misuse. A tool's own failure is not a
  protocol error: it is a `ToolResult` with `is_error: true`.

### `sheprd mcp` in `sheprd-gpui` (`#[cfg(feature = "mcp")]`)

- `cli/control/mcp.rs` parses `sheprd mcp [serve]` and
  `sheprd mcp config [--client claude|claude-code|codex|cursor]`. The second
  prints the snippet to paste into the client's configuration, with the
  absolute executable path (`agent_skill::executable()`), since a macOS
  bundle puts nothing on `PATH`.
- `control/client.rs`: split `run` into `dispatch(command, &Context) ->
  Result<Output>` and `finish`, so MCP reuses every family's existing code
  path without printing. `Output`'s JSON data becomes `structuredContent`
  and its text becomes the `text` content.
- `mcp/tools.rs`: one `macro_rules!` table between `keep-sorted` markers.
  Each row declares the tool name, a Fluent-free English description (agents
  read these, and they are protocol text, not UI), annotations, the input
  schema as `serde_json::json!`, and a parser from arguments to a typed
  `ControlCommand`. Unknown or ill-typed arguments return `is_error`. A
  string is never dispatched on directly.
- Every tool takes an optional `window` argument, matching the CLI's
  `--window` contract. The caller comes from the environment, as in the CLI
  (`HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`, `HC_TERMINAL_ID`): an agent in a
  Herdr pane acts in its own window, and one outside Herdr acts in the
  frontmost one. Targeting refuses rather than acting in another session's
  window, just as it does today.
- An error maps to `is_error` content that holds the CLI's exit-code name
  (`control::client::code`), so clients can tell "app not running" from
  "refused" from "not found".

### The gate

- `config/features.rs`: add `mcp: Mcp = false, "features-mcp",
  "features-mcp-description"`. Extend the `features!` macro so an entry
  carries `#[cfg(feature = "mcp")]` onto its field, variant, and match arms.
  Without the Cargo feature, the row, key, and messages disappear, and an
  `mcp = true` left in a config file is ignored like any unknown key.
- Messages go in a new `crates/sheprd-gpui/locales/en-US/mcp.ftl`, so other
  branches don't edit the same file.
- Protocol: add `Caller.via: Option<Via>` with
  `Via::Mcp { client: String }`, where `client` is the `clientInfo.name`
  from `initialize`, printable and at most 64 bytes. It is skipped on the
  wire when absent, and `PROTOCOL_VERSION` goes up to 16. An older app
  refuses the field (`deny_unknown_fields`), so MCP fails closed against it.
- The app's control server (`control/server.rs`) refuses any request with
  `via: Some(Mcp)` while `[features] mcp` is off. The app is the authority.
  A dialog an MCP call opens names the client ("Requested by Cursor via
  MCP").
- `sheprd mcp` reads `[features] mcp` from the config file at each
  `tools/list` and `tools/call`. This is a bounded read in a separate
  process, never the UI thread. Tools that need no app (memory get,
  checkpoint list, cloud list) cannot rely on the app's refusal, so this
  read is what gates them.

## Tools (v1)

Read-only (`readOnlyHint: true`):

| Tool | CLI equivalent |
| --- | --- |
| `status` | `status` |
| `window_list` | `window list` |
| `window_alerts` | `window alerts` |
| `tab_list` | `tab list` |
| `browser_list`, `browser_text` | `browser list`, `browser text ID` |
| `browser_feedback` | `feedback --wait S` (bounded wait) |
| `worktree_list` | `worktree list` |
| `chat_status` | `chat status` |
| `notes_get`, `notes_list` | `notes get`, `notes list` |
| `memory_index`, `memory_list`, `memory_search`, `memory_get` | `memory index\|list\|search\|get` |
| `checkpoint_list`, `checkpoint_diff` | `checkpoint list\|diff` |
| `pr_status`, `pr_checks`, `pr_comments` | `pr status\|checks\|comments` |
| `ports_list` | `ports list` |
| `usage`, `system` | `usage`, `system` |
| `security_list` | `security` |
| `events_wait` | `wait …` (bounded) |

Acting (no change without the user's say where the CLI asks today):

| Tool | CLI equivalent | Notes |
| --- | --- | --- |
| `browser_open` | `browser open` | URL or absolute file; `placement` tab or beside |
| `browser_navigate`, `browser_reload` | `browser navigate\|reload` | |
| `code_open` | `code open FILE[:LINE]` | read-only tab |
| `notes_set` | `notes set` | the caller's checkout |
| `memory_add`, `memory_pin`, `memory_forget` | `memory add\|pin\|forget` | refuses secrets as the CLI does; `memory_forget` has `destructiveHint` |
| `checkpoint_create` | `checkpoint create` | |
| `checkpoint_restore` | `checkpoint restore` | opens the dialog |
| `ports_open`, `ports_forward` | `ports open\|forward` | |
| `worktree_create`, `fan_out`, `teleport` | same | open the app's dialog prefilled; the user confirms |

Left out of v1: `app quit`, `run COMMAND` (any palette command), `ports
stop|pause`, `cloud exec`, `config`/`theme`/`font`/`keybinds` writes,
`remote` pairing, `update`, `mod`, and `automation approve`. They are
either too broad for an unattended client or already exist in Herdr's own
CLI.

## Settings

The `[features] mcp` switch in Settings > General is the entry, with a
description saying what it allows and pointing to `sheprd mcp config`. When
the switch is on, a "Copy client configuration" button under the row puts
the `sheprd mcp config` snippet on the pasteboard. A dedicated page waits
until there is more to configure, such as per-tool allow lists or HTTP.

## Bounds and safety

- No listener: a client can reach the app only by starting the user's own
  `sheprd` binary, the same reach the CLI already gives any local process.
  The switch keeps this opt-in.
- Every request and response stays within `MAX_MESSAGE`. Larger structured
  output is cut down the way the CLI already bounds it (for example
  `printable`), never streamed without limit.
- Destructive and outward actions keep the app's dialogs, and the dialog
  names the MCP client.
- Terminal content and page text a tool returns are untrusted data for the
  client, as they are for the CLI. We do not process them further.

## Tests

- `sheprd-mcp`: `initialize` negotiation (supported, unsupported, missing),
  `ping` during a running call, unknown method, malformed JSON, a refused
  batch, an oversize line that is refused without being buffered, a
  notification that must get no reply, cancellation dropping the reply, and
  ordered calls. Each topic goes in its own file under `tests/`.
- Tool table: names unique and valid (`^[a-zA-Z0-9_-]{1,64}$`), every schema
  an `object`, every row's parser accepting its documented arguments and
  refusing unknown ones, and every acting tool not marked `readOnlyHint`.
  Compare against the table, not a hard-coded count.
- Gate: a headless control-server test showing that `via: Mcp` is refused
  with the switch off and served with it on; `tools/list` is empty and
  `tools/call` refused when off; `Caller` round-trips with and without
  `via`, and an older `Caller` still decodes.
- Build: `cargo check -p sheprd-gpui --no-default-features` without `mcp`
  (no MCP symbols, no Settings row) and with `--all-features`.

## Docs

- `docs/cli.md`: a "MCP" section with `sheprd mcp` and `sheprd mcp
  config`, the client snippets, the tool table, and the switch.
- `README.md`: one line under features.
- The managed skill does not change: agents in Herdr panes keep the CLI.

## Phases (one PR each)

1. `sheprd-mcp` crate: JSON-RPC, `initialize`/`ping`/`tools`, the stdio
   loop, and tests.
2. The gate: the Cargo feature, the `[features] mcp` flag with
   `#[cfg]` support in `features!`, Fluent messages, `Caller.via` with
   protocol 16, and the server's refusal.
3. `sheprd mcp` with the read-only tools, the `dispatch`/`finish` split,
   and `sheprd mcp config`.
4. Acting tools, with dialogs naming the MCP client.
5. Docs, the Settings copy button, and a manual check with Claude Desktop
   and Codex.

## Where it stands

All five phases are in. Differences from the plan above:

- Tools build typed `ControlCommand`s and run the CLI's own `dispatch`,
  rather than control-socket requests directly, so commands that need no app
  (memory, checkpoints) work the same. `sheprd mcp` stamps every request its
  thread sends with `Via::Mcp`, so no command can reach the app unmarked.
- The client is named by a flash over the window's terminal when an MCP
  request opens a dialog, not inside each dialog. A dialog drawn over that
  spot may hide it.
- Settings > General shows an **MCP client setup** row under the flags,
  whose Copy button works while the switch is on.
- Memory and checkpoint tools use the directory the client starts
  `sheprd mcp` in. Clients that start servers from `/` (Claude Desktop) see
  no repository there.

## Later

- Streamable HTTP on `127.0.0.1` with a token, for clients that cannot start
  processes.
- `notifications/tools/list_changed` when the switch flips, so clients need
  not reconnect.
- Resources: the memory index and the current PR as readable resources.
- `sheprd-acp memory-mcp` and `thread-mcp` on the same crate.
- Per-tool allow lists in Settings.
- A `directory` argument for memory and checkpoint tools, for clients that do
  not start servers in the project.
- The client's name inside each dialog rather than in a flash beside it.
