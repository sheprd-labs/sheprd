# Session model

Sheprd supports backends that disagree about what a session is. Herdr 1 owns
workspaces, tabs, panes, and agents. herdr-core and the tmux adapter each own terminals. The goal
is one model the app reads whichever backend runs, so a feature stops depending
on Herdr 1's snapshot and a new backend costs an adapter rather than a rewrite.

## tmux

Sheprd keeps one session per terminal on the host's `tmux -L sheprd` server.
The stored address consists of the workspace host and a `tmux-<UUID>` terminal ID.
Tabs, splits and mirrors reuse the app's saved pane tree. Each attaching view
owns a control client and a local emulator. Detaching drops the client, while
closing a driving pane ends its session. Tmux reports the live working directory
and pane process ID. A Sheprd chat publishes its status, title and endpoint
through the session environment, including while its view is detached. The
tmux server owns the programs and their scrollback. Stopping that server or
rebooting the host ends those processes. When an owning view reopens after a new
host boot, the adapter recreates its managed shell or ACP chat with the same
pane ID. A shell reopens in the folder it was last in: the desktop's inventory
saves a running shell's new folder to its recovery record when tmux reports
that it moved. Tabs the desktop saved in a Herdr 1 workspace keep that backend,
so the desktop starts Herdr 1 for them at launch even with tmux selected. The ACP helper records the endpoint, built-in agent, session ID and
provider history directories and workspace labels on the machine that runs the chat. A tmux-enabled remote service also reopens recorded chats on startup after a host reboot, even with the desktop closed. It requires
the provider to load that exact session. Shell screen contents and arbitrary
programs have no comparable continuation protocol.

The desktop's terminal worker depends on `Backend` and `Terminal`.
`WatchBackend` adds an ordered event stream for herdr-core. The tmux desktop
inventory uses polling. The tmux adapter takes a scoped `CommandHost` for remote
execution, so SSH, WSL and cloud authentication stay in `sheprd-remote`. Paired phone and web clients can open, continue and start managed tmux chats through a tmux-enabled remote service. Raw terminal control and shared workspace documents, Teleport
and daemon-launched automations still have their own Herdr-specific callers.
The existing `sheprd-t3` client owns the T3 conversation integration. A T3
thread keeps its server and thread identity regardless of the terminal backend.
Further T3 work belongs at the conversation layer, where transcripts, sending a
turn and approvals can be implemented against the T3 server. The existing
[T3 integration plan](t3-plan.md) describes what the client already supports.

### Remaining work for Herdr-free feature parity

| Feature | Current dependency | Interface needed |
| --- | --- | --- |
| Phone and web terminal control | `sheprd-remote-control/src/hub/core` attaches to a core and reads its shared spaces document | Use the terminal traits for the screen, driving lease, input and scrollback. Give shared layout storage its own revisioned read/write interface. |
| Teleport | `sheprd-remote/src/teleport` prepares and resumes terminals on the destination core | Select the destination backend and track a verified source, destination and transfer receipt before closing a source pane. |
| Fan-out and orchestrator | `sheprd-remote/src/work/owned_agent` checks core ownership labels and the live foreground process before messaging or ending an agent | Provide an owned-agent interface with atomic labelled spawn, process identity checks, readiness and recovery. |
| Scheduled automations | `sheprd-automate/src/drive.rs` selects Herdr or herdr-core independently of the desktop preference | Extend the automation drive with the same owned-agent operations and persist the selected backend in each run. |
| Converting an existing CLI agent into a chat | `sheprd-core-client/src/convert.rs` verifies the current process and its recorded conversation | Supply tmux process-bound session evidence before interrupting an agent or sending its resume command. |
| Agent notes and remote browser requests | The desktop delivers notes and acknowledges remote requests through herdr-core | Route replies through the owning backend and verify that the same agent still owns the receiving pane. |
| Recovery after stopping tmux within the same host boot | Automatic chat recovery is scoped to host reboots and saved owning panes | A same-boot restore needs proof that the old agent has exited. |
| Personal tmux servers | The desktop lists Sheprd's dedicated server | Moonmux demonstrates read-only discovery on other sockets. External pane identities need the host, server lifetime and pane ID, and explicit ownership before a destructive operation. |

## Where it stands

| Layer | Herdr 1 | herdr-core | tmux |
| --- | --- | --- | --- |
| Terminals (`sheprd-backend`) | Not implemented; `sheprd-client` talks to the daemon | `CoreBackend` | `sheprd_tmux::Tmux` implements the same lifecycle and attached-view traits as `CoreBackend` |
| Tab and pane layout | The daemon's snapshot | `sheprd_session::panes`, saved with the tab | The same saved pane tree, scoped to the desktop |
| Workspaces | The daemon's snapshot | Kept by the app beside its tabs (`browser-tabs.json`'s `workspaces`, read by `SavedTabs::workspaces`), under the scope of the host whose core runs them (this computer, or a saved SSH, cloud, or WSL host), listed after Herdr 1's; with no Herdr 1 installed, the first is `core_terminals::WORKSPACE`. A terminal started outside the app for a workspace, as by a phone, carries `sheprd_session::WORKSPACE_LABEL` and gets a tab there (`core_terminals::adopt`) | Kept by the desktop, including a local Home workspace when Herdr is absent |
| Agents | The daemon's snapshot | `TerminalInfo::agent`, per terminal; which agent by `sheprd_core_client::agent_kind` | Sheprd chats publish agent status, title and endpoint in their tmux session. Other programs expose their foreground command |
| Commands | 43 `sheprd_client::Method` variants | Handled per feature in `core_terminals` | The terminal worker handles input, history and resizing. The adapter handles listing, spawning and closing |
| `Session` | `session_model::of_snapshot`, cached per snapshot in `LiveState::session` | `Session::core`, rebuilt on the window tick from the app's tabs and the core's terminals | The app builds the same kept-workspace session from saved tabs and its tmux inventory |

`sheprd-session` holds what is already shared: the pane tree, the reader of the
app's saved tabs, which the remote control daemon used to parse as untyped JSON,
and `Session`: workspaces, their tabs, and agents, each workspace marked with the
`Keeper` that carries out what is asked of it. A test in the app reads what the
app saves through the saved tabs reader, so the two cannot drift apart.

`SheprdWindow::sessions` gives a host's sessions together: Herdr 1's, and the
one over herdr-core, on this computer and on each saved host whose workspaces
the app keeps. Readers walk both, so a reader moved onto
it lists Herdr 2 work beside Herdr 1's.

| Reader | On `Session` |
| --- | --- |
| Devices overview: counts, agent lines, search | Yes |
| Sidebar workspace rows, rail, search, reorder, previous and next workspace | Yes |
| Window title | Yes |
| Tab strip | Herdr 1's tabs only while its workspace is in front (`herdr_workspace_in_front`) |
| Palette Go To and jump list | Kept workspaces added beside Herdr 1's rows |
| Workspace menu | Kept workspaces: rename, close, a note once Git names the checkout, new and deleted worktrees, which the app makes with Git (`sheprd_remote::work`), fan-out, and worktree scripts, which run in herdr-core tabs (`worktree_scripts::kept`) |
| Fan-out, smart dispatch, orchestrator | Kept workspaces on this computer: Git makes the worktree and the agent starts in a herdr-core shell (`sheprd_remote::work`); the app keeps a workspace for each checkout |
| Merged cleanup | Kept linked checkouts whose cached pull request merged (`merged_cleanup::kept`), with host-scoped repository, branch, HEAD, dirty-state and archive-script revalidation; PR discovery is still limited to the origins supported by the GitHub cache |
| Git status, pull request prefetch, worktree rule, notes | Kept workspaces whose folder Git names (`core_terminals::checkouts`) |
| Agent checkpoints | Yes: `sheprd_vcs::checkpoint::Checkpoints::observe` takes a `Session`; Herdr 2's is watched as its own endpoint |
| Mods state: workspaces, agents, focus | Yes |

herdr-core reports no Git facts, so the app asks Git for each kept workspace's
folder on a worker (`sheprd_vcs::git::identity`): its branch, its repository's
common directory as the worktree key, and whether it is a linked worktree.
An unnamed root space follows its first terminal's live folder, using the
checkout's top-level folder inside Git and `~` for the home directory, as
Herdr does. The lookup runs on that terminal's host in the same background
worker, which also caches that host's home directory. The last reported folder
keeps naming a space during a disconnect; it does not make cached agent status
current. Explicit names and linked worktree names stay fixed. The same first
terminal's live folder drives new terminals and chats, Git/PR metadata, and
upstream counts. Its last folder is persisted through last-terminal closure
and restart; persistence never replaces the saved label with a basename.

Remote kept workspaces read their Git identity on their own SSH, cloud or WSL
host and keep separate metadata caches, even when paths coincide. Worktree
creation, opening, deletion and setup/run/archive scripts use that same route.
Archive terminal completion is scoped to its core; switching devices cannot
turn another core's exit into permission to delete a checkout.

The app still reads `ClientShell*` types directly in about 67 files. That is the
work left.

## Moving the app onto it

Move one reader at a time, in separate pull requests, so parallel work conflicts
as little as possible:

1. Add `Session` to `sheprd-session`: workspaces, tabs, and agents as domain
   types, built from a `ClientShellSnapshot` in the app (`session_model`)
   rather than through a new dependency here; a `From` impl would need one,
   as both types are foreign to the app. It is a model, not a mirror of the wire types: only what
   features read, and nothing the daemon's protocol alone needs.
2. Keep it next to `LiveState::snapshot`, built once when a snapshot lands, so
   render paths stay thin and revision fencing stays where it is.
3. Move readers that only list things first: the overview, then the sidebar's
   workspace rows, the tab strip, and the palette.
4. Move commands last. Each becomes an intent the backend carries out or
   refuses, so a backend without the capability says so instead of failing late.
5. Build a `Session` from herdr-core: several workspaces, saved by the app. This
   replaces `core_terminals::WORKSPACE` and is what the default flip in the
   Herdr 2 rollout waits on. Done: a new workspace is the app's with Herdr 2
   chosen for new terminals or with no Herdr 1 installed. A kept workspace in
   front (`SheprdWindow::kept_in_front`) is the window's workspace whatever
   Herdr 1 focuses, and choosing anything of Herdr 1's hands the front back.

## Decisions

- Layout stays in the app's saved tabs rather than a core document. The app
  saves the tabs of every core it reaches, on this computer or over SSH, and
  the remote control daemon reads the same file,
  so a document would have no reader the file does not already serve. Revisit
  when a second client on another machine needs a core's layout.
- Sheprd's saved tabs win on the phone. A `herdr-app` document lists what they
  do not hold, so a terminal in both stays in Sheprd's tab and its split.
- No daemon of our own. Add one only for something that must run while the app
  is closed and that neither `doc.*` nor `sheprd-automate` can hold.
