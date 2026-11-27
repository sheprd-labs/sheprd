# Sheprd on Moonframe

Sheprd opens the classic window by default on every platform. Releases include
the [Moonframe](https://moonframe.dev) beta. To try it, turn on **Settings >
General > Moonframe window (beta)** and restart Sheprd, or set
`moonframe_window = true` under `[features]` in your configuration.

## Choosing a window

A plain launch follows your setting. For a single launch, use
`SHEPRD_MOONFRAME=1 sheprd` for Moonframe or `sheprd --classic` for the classic
window. On macOS, holding ⌥ Option during a normal launch opens the other
window once. Commands and launches with `--socket` or `--session` keep their
usual behavior.

On Windows, Moonframe needs `HOME` or the XDG variables to persist its files
and window placement across launches.

The two windows keep their state apart. The classic window's settings, log,
and saved tabs stay under `herdr` as before; Moonframe keeps its own under
the name `sheprd`: `~/.config/sheprd` (your copies of the layout's files),
`~/.cache/sheprd` (the layout written out to run), and `~/.local/state/sheprd`
(where the window was, and each mod's state). Both read the same settings
file and talk to the same daemon.

## Builds

| Build | Layout | The sparkles agent edits |
| --- | --- | --- |
| Default (`just run`, releases) | Compiled in, written to `~/.cache/sheprd/app/<hash>` to run | Your copies in `~/.config/sheprd` |
| `moonframe-studio` (`just run-moonframe`) | Read from `crates/sheprd-gpui/moonframe/app`, reloaded on save | The checkout's files |

Both windows show the same stock controls. **Settings > General > Moonframe tools**
adds the layout editor button, worktree details toggle, and chat turn timeline
to the Moonframe window. You can also set `moonframe_tools = true` under
`[features]`. The Developer menu and Inspect Element are available in Moonframe
with either setting.

`just run-moonframe` uses your settings and daemon, so quit Sheprd first.

The window layout is defined in `crates/sheprd-gpui/moonframe/app`:

| File | Draws |
| --- | --- |
| `main.luau` | The layout: where each part goes, the rule between New and Menu and the agents, that shares the left pane's height between them (kept across launches; a double click shares it evenly again), and the device picker under them |
| `mods/header/main.luau` | Hosts the draggable classic header (`region = "header"`), with Back and Forward while they fit the column |
| `mods/activity/main.luau` | Hosts the shared activity bar, including mod apps, counts, keyboard stops and Settings. Moonframe tools adds the editor above Settings |
| `mods/worktrees/main.luau` | Places the shared `sidebar_title` and `worktrees` regions, including the search field, tags, pinned hosts, context menus, and row reordering |
| `mods/actions/main.luau` | Hosts the classic New and Menu controls and their click handlers |
| `mods/agents/main.luau` | Hosts the shared agents list, including favorites, sort, groups and the empty state |
| `mods/devices/main.luau` | The device picker at the left pane's foot, and the sheet it opens: the agents' activity, every device and the unfolded one's sessions, and Add Device |
| `mods/details/main.luau` | The optional worktree details pane, opened from the title bar with Moonframe tools enabled |
| `mods/turns/main.luau` | The optional chat turn timeline in the left gutter, with Moonframe tools enabled: a tight stack of ticks in the gutter's middle, one per prompt, the prompt and the start of its reply on the card under the pointer, the turn being read marked, and a click back to a prompt. The shell floats it where Sheprd laid the gutter out |
| `mods/build/main.luau` | Hosts the shared worktree build strip with its branch and pull request link |
| `mods/status/main.luau` | Hosts the classic status bar with `region = "status"`, including the version and update button, configured controls, and mod items |
| `components/sheprd-*.luau` | Reusable Luau rows, Orca status marks (working spinner, blocked question bubble, completed check and idle dot), and section headings for custom mods |
| `style.luau` | The hover looks, for every mod |

The stock shell places the terminals, tabs and pages with `region = "body"`.
When activity pages or the tab position require a standalone title bar, it
places `full_header` across both columns. With tabs in the title bar, the
`titlebar` region fills the room at the end of the tab strip. Existing layouts
using `region = "main"` keep their built-in standalone header.

Folded rails, the Devices layout, nested agents, sidebar mods and the hidden
device-footer layout use the whole `sidebar` region, including its own
controls and resize edges. The `devices` region uses the same picker as
Classic. Custom Luau pickers can still open their own sheet using
`device_picker` and the existing provider actions. Copied mods can continue drawing their own rows from the published workspace and agent data.

The terminals keep the keyboard: they take it when the window opens
or comes back to the front, and after a click anywhere in the layout, so only
a field (the search, a note) holds it instead.

On macOS native browser pages sit below GPUI. The toolkit root background and
Luau shell must stay transparent; the native window supplies the base and
Sheprd's main region paints around page rectangles. Sidebar, toolbar and
popover backgrounds remain solid. Adding a full-window background to the shell
hides loaded pages, even when WebKit text and page captures look correct.

## Edge lighting

The window can carry a passive light around its edge, following the focused
workspace's `agent_status`: a clockwise comet while working, two pulses while
blocked, split waves when done, and a quiet breath while idle. No focused
workspace or an unknown status clears it. This is native
(`crates/sheprd-gpui/src/window/edge_light.rs`), not a Luau mod: it follows
both sides of each macOS window corner and uses square corners in fullscreen.

**Settings > Indicators > Edge Lighting** chooses **Auto (status)**, a
preset, or a custom `#RRGGBB` color; the stroke width (1–12 logical pixels)
and trail length (5–50% of the perimeter). The same values are available as
`[edge_light] color`, `width` and `trail` in `config-gpui.toml`.

## Changing it

In `just run-moonframe`, saving a file reloads the window. To keep a change of
your own in any build, copy the file to the same path under `~/.config/sheprd`
and edit the copy; deleting it brings the original back. With Moonframe tools enabled, the sparkles above Settings open a window where you ask an agent to make a change, and it asks before each edit:
in a release it writes those copies, in `just run-moonframe` the checkout's
files. View > Inspect Element (⌥⌘I, or ⇧⌘X
to go straight to picking) shows what built any part, and its file and line.

Run `just run-moonframe --check` after a change (`sheprd --check` checks the
copy a build carries). It loads what is
drawn at that moment, so a mistake in a row that only appears with an agent
shows in the log when one appears: `unknown key` names it.

## `app.sheprd`

| Field | Holds |
| --- | --- |
| `devices` | Each device the sidebar lists, in its order: `id`, `label`, `selected`, `connected`, `enabled`, `removing`, `collapsed`, `wash` (host tint as #rrggbbaa), `opacity` (row opacity for group-dim), `load` (its machine's load in the shape of `app.sheprd_load` below: `cpu` and `memory` while the load is shown, and `gpu`, with `busy` and `memory`, while the sidebar shows the GPU, with their `history` and `meter` once `sidebar_load = "detailed"`), `load_lines` (whether the load takes lines of its own under the name, as `sidebar_load` asks, rather than shares beside it), `search` (every workspace, folded or not, with `label`, `branch` and `note`, for the sidebar's search), and `workspaces`, its visible rows (fields as below, plus `endpoint`, `group`, `fold_glyph` and `collapsed` on a checkout that folds its worktrees, `note`, `muted`, `teleported`) |
| `multi` | Whether several devices are listed, so each shows a header |
| `device_picker` | The device picker: `footer` (`label`, `initials` beside the activity bar, `hosts`, `connected`, `adding`, `settings` (its tooltip, without the bar), `aria`, `shown`, `width`), `online` and `wash` (the connected dot's and the current line's colors), `open`, and while open `sheet`: `activity` (`heading`, `overview`, `working`, `blocked`, `bars` of `working` and `blocked`, or `false` before a step was seen, and their `peak`), `heading`, `hint`, `selected` (the line the keys are on), `lines` (each `kind`: `all`, `device`, `session`, `add`, `note` or `error`; the lines the keys reach have `ordinal`, `key`, `label`, `detail`, `checked`, `enabled`, and `online`, a device its session `chip` (`word`, `session`, `open`, `hint`), a session `delete` (whether it can be deleted now); a note or error its `text`), and `adding` (cloud machines still being added) |
| `moonframe_tools` | Whether the stock layout shows the editor, worktree details control and chat turn timeline |
| `build` | The worktree a build came from: `worktree`, `branch`, `pr` |
| `chat_turns` | The turns of the chat on screen (Herdr's focused pane when several are), or `null` while none shows, its gutter (the room its reading column, `[chat] max_width`, leaves on the left) is under 120 pixels, or it has fewer than two prompts: `pane`; `left`, `top`, `width`, `height`, where the ticks go in the `main` region's coordinates: the gutter's width, and a stack only as tall as the ticks need at `spacing` points apart, in the gutter's middle (the whole gutter once they fill it), since the timeline spreads its ticks over the height it gets; `entries`, each prompt's `id` (its entry), `label` and the start of its reply as `summary`, once there is one; and `selected`, the turn being read |
| `workspaces` | The selected device's, folded or not, in the sidebar's order: `id`, `number`, `label`, `branch`, `repo`, `repo_key`, `repo_tile` (initials and colors when repository icons are enabled), `linked`, `child` (shown under its checkout), `focused`, `agent_status`, `ahead`, `behind`, `added`, `removed`, `dirty`, `pr`, `pr_color` (the number's color for its state) |
| `agents` | In Sheprd's sort: `name`, `kind` (`claude`, `codex`, ...), `status`, `state` (the status's word, when the agent's integration shows one), `title`, `pane`, `pane_name`, `workspace`, `workspace_label`, `tab`, `focused`, `unread` (a reply no one read), `drafted` (its chat holds a draft not sent) |
| `agent_sort` | `grouped` or `priority` |
| `layout` | The configured sidebar layout's `name` and spacing: `padding`, `gap`, `row_padding`, `style_padding`, `child_indent`, `card_indent`, `header_padding`, `footer_padding`, `nest` (how far a device's rows step in under its header), `inset`, `spacing`, `radius`, `outline` (rounded highlights), `tree_lines`, `title_case`, `workspace_details`, `child_details`, `pr_counts` |
| `panels`, `panel` | The activity panels (`id`, `label`, `icon`, `count`, `unseen`) and the current one's `id`. `icon` is Sheprd's own, the file its classic bar draws, as an `app-` name (`app-sessions`) that Moonframe loads from the assets Sheprd gives it: each `icons/NAME.svg` of Sheprd's is also `icons/app-NAME.svg` there |
| `hosts_folded` | Whether every listed host is folded, or `null` when the list has fewer than two host headings |
| `sidebar_line_height`, `sidebar_title_height`, `primary_label` | The sidebar line and title heights in pixels and the localized primary-checkout badge |
| `connected`, `status` | Whether Herdr 1's daemon answers, and the status bar's connection message, the classic window's own, or `null` while it shows none: connected without an error, or a host with no Herdr 1 whose herdr-core runs its terminals |
| `status_bar` | How the status bar's Theme and Commands buttons show, as `[status_bar] theme` and `shortcuts` set them: `theme` and `commands`, each `label`, `icon` (the icon alone, named by its tooltip) or `hidden` |
| `section`, `sidebar_visible`, `status_bar_visible`, `details_visible`, `navigation_in_header` | What the window shows (`navigation_in_header`: whether Back and Forward fit the left column's header, as Sheprd's own window decides); `details_visible` follows the toggle at the end of the title bar, and the shell shows the details pane by it |
| `usage` | The status bar's usage as Sheprd's own bar shows it, or `nil` while hidden or empty: `segments` (each provider's `id`, `name`, `locked`, `stale`, `meter` with `used` as a fraction and its `color`, `labels` with `text` and `color`, and `balance`), `busy`, `error`, and the words `unavailable` and `refresh_label` |

Two parts change on their own, so they have their own providers and a change
runs only the views that read them:

| Provider | Holds |
| --- | --- |
| `app.sheprd_load` | The selected host's load as Sheprd's own status bar shows it, or `nil` while hidden: `cpu` and `memory` as percentages with `cpu_color` and `memory_color`, the labels `cpu_label` and `memory_label` in `label_color`, and while the bar is detailed `history` (the CPU sparkline, oldest first: `value` as a fraction and `color`, or `false` before it is sampled), `meter` (memory: `used`, `color`) and `track`; `unavailable` when the host never answered. While the GPU is shown and the host has a card, `gpu` holds its line, alone when CPU and memory are hidden: `busy` and `memory` (video memory) as percentages with `busy_color` and `memory_color`, the labels `busy_label` and `memory_label` in `label_color`, and while its detail is detailed `history` (of `busy`), `meter` (of `memory`) and `track` |
| `app.sheprd_theme` | `accent`, `active`, `added` and `removed` (change counts), `background`, `danger`, `dirty` and `dirty_ink` (the uncommitted-work chip), `foreground`, `muted`, `on_accent` and `on_muted` (text over those), `sidebar`, `sidebar_size` (the named size nearest the sidebar font's size over the UI size), `status` (each agent status's color), `subtext`, `success`, `surface`, `warning` |

Moonframe's own widgets follow Sheprd's theme and UI font without reading these.
`font = "mono"` is Sheprd's sidebar font, though only its family: set
`size = theme.sidebar_size` beside it for the sidebar's size when drawing custom rows.

## Menus

The menu bar is Sheprd's own, as its window has it (Sheprd, File, Edit, View,
Terminal, Window), with Moonframe's Developer menu before
Window: Developer Mode and, while it is on, Components, Reload Modules, Undo
and Redo, Inspect Element, Developer Tools and Slow Animations. A menu item reaches Sheprd
wherever the focus is, in the Luau layout or a terminal.

## Asking Sheprd for something

Send a message with `to = "provider:sheprd"`:

| Message | Does |
| --- | --- |
| `{ command = "name" }` | Runs any command by its name in the catalog, such as `new_workspace`, `toggle_sidebar`, or `themes` |
| `{ focus_workspace = id }` | Focuses a workspace |
| `{ focus_pane = id }` | Focuses a pane |
| `{ panel = id }` | Shows an activity panel |
| `{ menu = "workspace", workspace = id }` | Opens Sheprd's own popover for a workspace at the pointer, without selecting it: its details, actions (close, delete the worktree, rename, mute, note, teleport, ...) and pull request. Send it as a row's `context`, so a right click opens it |
| `{ menu = "agent", pane = id }` | Opens an agent's pane menu at the pointer |
| `{ menu = "device", endpoint = id }` | Opens a device's menu at the pointer |
| `{ note = id }` | Edits the note on a workspace's checkout |
| `{ toggle_repository = key }` | Folds or unfolds a repository's linked worktrees |
| `{ fold_all = true }` | Folds every repository's worktrees on every device; `false` unfolds them |
| `{ toggle_device = id }`, `{ select_device = id }` | Folds a device's header, or selects the device |
| `{ toggle_agent_sort = true }` | Switches the agents between grouped and by priority |
| `{ move_workspace = id, target = id, after = bool }` | Moves a workspace of the selected device before (or after) another, as dropping its row there does; the worktrees mod sends it when a row with `carry` is dropped on one with `accept` |
| `{ sidebar_width = true }` as a `pointer` | Resizes the sidebar from a drag of its edge (`phase`, `dx`); a double click puts back the default width. The shell draws the edge as a hairline beside the left column, its grip overhanging the main area so it never covers the lists' scrollbars. The shell takes the drag (`hover`), draws the handle as a Moonframe split does (a short pill on hover, a longer one while dragging, as on the rule below New and Menu), and sends the drag on |
| `{ open_build_pr = true }` | Opens the pull request a worktree build came from |
| `{ device_picker = true }`, `{ device_picker_dismiss = true }` | Opens the device picker or closes it, or only closes it (a click outside, Escape) |
| `{ device_line = ordinal, key = key, how = "choose" }` | Picks a picker line: a device (with Cmd, Ctrl elsewhere, listed too or no longer, the sheet left open), a session, or Add Device. `how = "sessions"` unfolds a device's sessions or folds them, `how = "delete"` asks to delete a session. A line whose `key` no longer matches its `ordinal` is left alone |
| `{ chat_turn = pane, revision = id }` | Scrolls the chat in `pane` back to the prompt `id`, as the revision timeline sends it with `select = { chat_turn = pane }`. A prompt the chat no longer has is left alone |
| `{ device_key = key }` | A key the open picker took (`up`, `down`, `left`, `right`, `enter`, `backspace`, `delete`, `escape`), as Sheprd's own picker reads it |
| `{ devices_overview = true }` | Opens the Devices overview, as the picker's activity card does |
| `{ usage = id }`, `{ usage_refresh = true }` | Opens a usage provider's panel above the status bar, or reads usage again |

A row on another device adds `endpoint = id` to its message; without it the
selected device is meant.

## What is not there yet

The window starts as Sheprd's own does (`app::install_services`): it answers
the `sheprd` command line, sends system notifications, follows links, checks
for updates, badges the Dock, and writes Sheprd's log and crash reports. What
looks for a main window finds it through `window::main_window`, whatever the
window's root view.

The window reopens where it was and as large, as Moonframe keeps it
(`window = { remember = false }` in `app/main.luau` turns that off).

New Window opens a Classic window. Moonframe currently hosts one main window.

## Measuring

Compare release builds: a debug window shows a yellow `dev build` strip, and
runs several times slower. GPUI times every frame in either window, so the same
line reads both:

```sh
SHEPRD_MOONFRAME=1 ZED_MEASUREMENTS=1 RUST_LOG=moonframe::perf=debug target/release/sheprd
ZED_MEASUREMENTS=1 target/release/sheprd --classic
```

`frame duration` is one frame's draw and present. `moonframe::perf` adds what
each publish and message ran. Machine load moves these a lot, so alternate the
two windows and compare medians.

`just bench-moonframe /path/to/herdr` does that for you: it builds the app,
runs each window in turn on a private daemon with 14 linked worktrees, through idle,
terminal output, agents changing state, and focusing each workspace, and
prints the medians of three rounds. It opens windows, so it needs a desktop.
`scripts/bench_frames.py` takes other commands (`NAME=BINARY ARGS...`), round
counts, and `--json`.

For terminal stalls, use `scripts/bench_terminal.py` in a Sheprd internal
virtual Mac (`sheprd machine exec NAME -- ...`), with the guest logged in,
awake, and its agent allowed Accessibility and screen capture. Copy the
release binary, Herdr, both benchmark Python files, and this native wheel
helper into the guest; Python 3.10+ and Git must be on its `PATH`:

```sh
swiftc scripts/bench_terminal_scroll.swift -o /tmp/bench-terminal-scroll
python3 scripts/bench_terminal.py --app /path/to/sheprd --herdr /path/to/herdr \
  --scroll /tmp/bench-terminal-scroll --results /tmp/terminal-benchmark
```

The helper can be compiled on the host. It requires macOS, Accessibility,
and screen point `(700, 450)` inside the terminal. Each fresh private daemon
has 14 linked worktrees; no personal sessions are used. The harness focuses
and sizes the app to 1000×650, checks it is active, and alternates classic
and Moonframe over three rounds. It measures idle, steady output, heavy
output, and native scrolling through 3,000 lines, with two seconds of warmup
and 12 seconds measured per phase. `--scroll-only` or `--steady-only` narrows
the workload; `--output-only` measures steady and heavy output together.
`--modes moonframe` isolates one mode when comparing binaries.

Keep the VM tab visible and the guest unobscured by permission dialogs;
a hidden virtual display can stop delivering frame callbacks even when the
guest window reports itself active. Failed commands, inactive
windows, failed wheel injection, and unchanged measured scroll positions reject a
run. The harness samples the daemon's scroll offset every half second during
measurement and saves `scroll-samples.json`; warmup movement alone is not
enough. This readback adds a small amount of measurement overhead. Wheel
input and optional pixel capture use `--until-eof`: the harness closes their
stdin only after measurement ends, so setup delays cannot shorten their run.
An early helper exit rejects the run, even if its exit code is zero.
Each run saves screenshots, status, synthetic terminal readback, logs,
frame timestamps, and `result.json`. Compare redraw rates **and** maximum
silence, including the measurement boundaries: a frozen UI can report cheap
frames precisely because it did not draw. These are frame-processing times
and redraw counts, not input-to-photon latency. Repeat before/after pairs
on the same VM without concurrent builds or other workloads before judging
small timing differences.

Window redraws alone can also replay stale terminal pixels. To check the
content itself, compile `scripts/bench_terminal_pixels.swift` with
`swiftc -O -parse-as-library` and pass its executable as `--pixels PATH`
alongside `--scroll`. This separate ScreenCaptureKit probe observes the
numbered text at `(560, 220, 100, 300)` in display points, at up to 60 Hz.
It records a change when more than 0.5% of pixels differ by over 8 channel
levels, rejecting capture noise. `pixel_change_hz` and
`pixel_max_silence_ms` describe that region, not input-to-photon latency.
The first 100×300 BGRA frame is saved so the region can be checked. Compare
pixel-probed runs separately from uninstrumented timings: capture adds work.
