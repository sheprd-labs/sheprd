//! Usage text for every control command.
use crate::cli::control::FAMILIES;

const COMMON: &str = "\
Every command accepts --json, printing one JSON document, and --window ID,
acting in that window (IDs come from `window list`). By default a command
acts in the window showing the caller's workspace (HERDR_WORKSPACE_ID), or,
in a herdr-core terminal (HC_TERMINAL_ID), the tab holding it, and exits 5
when no window shows it; outside both, in the frontmost window.
Exit status: 0 done, 1 refused, 2 invalid arguments, 3 Sheprd not running,
4 nothing yet (no notes, no event), 5 not found, 6 busy (a dialog is open),
7 timed out, 8 unsupported on this platform or by this app version.";

const BROWSER: &str = "\
sheprd browser open URL|FILE [--workspace ID] [--focus|--no-focus] [--beside|--tab]
    Show a web address, or a local HTML file, in a browser tab of the running
    Sheprd. The tab joins the caller's own workspace (HERDR_WORKSPACE_ID),
    or the frontmost one outside Herdr. Showing the same page again from the
    same pane reuses and reloads its tab. Bare hosts such as localhost:3000
    are accepted. A file is served with the other files in its folder.
    Files and web addresses open in a background tab. --focus selects the
    page, and --beside places it in a split beside the caller's terminal.
sheprd browser reload
    Reload the pages this pane opened, for example after editing the file.
sheprd browser feedback [--wait SECONDS]
    Print the notes the user sent about this pane's pages and have not been
    delivered yet. With --wait, wait up to SECONDS (at most 600) for them.
    Notes are otherwise typed into this pane as a prompt once it is idle.
sheprd browser list
    List page tabs with their IDs.
sheprd browser close|back|forward|text TAB
    Close a page tab, go back or forward in it, or print its visible text.
sheprd browser navigate TAB URL
    Load another web address in a page tab.
sheprd browser zoom TAB +|-|reset|PERCENT
    Zoom a page tab in or out a step, back to 100%, or to PERCENT (50-300),
    as the font size keys do while it is focused. The tab keeps its zoom.
sheprd browser screenshot TAB PATH
    Save a PNG of the page without selecting it (macOS). An unloaded page
    in an inactive Mac window reports busy for screenshot and text.
sheprd browser skill
    Print the agent skill that explains these commands.
sheprd browser example
    Print an example explainer page built on the kit Sheprd serves to local
    pages at /.herdr/kit.css and /.herdr/kit.js.
Exit status: 0 done, 1 refused, 2 invalid arguments, 3 Sheprd not running,
4 no notes (feedback), 5 no such tab or workspace, 6 busy, 7 timed out,
8 unsupported on this platform.";

fn family(name: &str) -> &'static str {
    match name {
        "status" => {
            "sheprd status\n    Whether Sheprd runs, its version, protocol, windows, and hosts.\n    Exits 3 when it does not run."
        }
        "skill" => "sheprd skill\n    Print the agent skill describing these commands.",
        "app" => {
            "sheprd app focus|hide|quit\n    Bring the app forward, hide it, or quit it. Quitting leaves Herdr\n    and its terminals running."
        }
        "open" => {
            "sheprd open DIR\n    Open the folder as Open Project in the command palette does: focus its\n    workspace, or start one there, and bring the window forward. DIR must\n    be an existing folder. With no Sheprd running, starts it and waits up\n    to 20 seconds for its window to reach the local Herdr (exit 7 past that)."
        }
        "window" => {
            "sheprd window [list]\n    The app's windows, with IDs for --window.\nsheprd window alerts\n    The alerts the window kept, newest first, including those held back\n    by quiet hours or with delivery off.\nsheprd window new [--session NAME]\n    Open another window: of the caller's own Herdr session, of a named\n    local session, or outside Herdr of the frontmost window's host.\nsheprd window focus ID\nsheprd window fullscreen [on|off|toggle]\nsheprd window size [WIDTHxHEIGHT]\n    Print the window's size, or resize it. Windows cannot be moved.\nsheprd window detach|reconnect\n    Detach from, or reconnect to, the window's selected host."
        }
        "ui" => {
            "sheprd ui settings [PAGE]\n    Open Settings and list pages and row IDs, or open PAGE, such as voice for speech accounts.\nsheprd ui highlight ROW_ID\nsheprd ui guide TEXT [--highlight ROW_ID]\nsheprd ui guide --clear\n    Point out a Settings row or update one persistent three-line banner.\n    The person can dismiss it; --clear removes banner and highlight.\n    These commands do not change settings.\nsheprd ui sidebar [on|off|toggle]\nsheprd ui statusbar [on|off|toggle]\nsheprd ui menubar [on|off|toggle]\nsheprd ui font-size +|-|reset|POINTS\nsheprd ui panel workspaces|overview|pull-requests|issues|tasks|security|automations|ports|usage|traces\nsheprd ui traces [--host ID|--tag TAG|--all]\n    Read prepared replication status, indexed host IDs, and the current filter.\n    With a filter, show Traces for that host or existing device label, or all hosts.\nsheprd ui agent-group [NAME|all]\n    List the agents panel's coordinator groups, or show one group's sessions\n    with its coordinator first. Needs [sidebar] coordinator_suffix.\n    For this window and session; settings are changed with `config`."
        }
        "tab" => {
            "sheprd tab [list]\n    The app's own tabs: pages, code, reviews, Devices.\n    Terminal tabs belong to Herdr: use `herdr tab`.\nsheprd tab focus|close ID\n    Show a tab, switching to its workspace, or close it."
        }
        "code" => {
            "sheprd code open FILE[:LINE]\n    Show a local file read-only in a code tab, at LINE.\nsheprd code edit FILE[:LINE]\n    Open it in your terminal editor ($VISUAL, $EDITOR) in a new pane."
        }
        "devices" => {
            "sheprd devices [open]\n    Open the Devices overview tab in the caller's workspace."
        }
        "machine" => {
            "sheprd machine [open [NAME]]\n    Open the virtual Mac called NAME (default: default) in a background tab of the\n    caller's workspace, starting or resuming it. Closing the tab leaves it\n    running; quitting Sheprd saves it. Apple silicon Macs only.\nsheprd machine setup [NAME]\n    Show it with its setup guide: the steps that make it ready for agents\n    (install Sheprd Agent, allow its permissions, save an image), which\n    Sheprd checks one by one.\nsheprd machine start NAME\n    Start it without a tab, resuming it if it was saved. `machine agent`\n    tells when it is ready.\nsheprd machine claim NAME [--wait S]\n    Reserve for this pane for 15 idle minutes. Repeat to renew. --wait (1-600)\n    reports progress on stderr while busy, including full VM capacity.\n    Claims clear when Sheprd quits. Releasing a claim does not stop the VM.\nsheprd machine release NAME\n    Release your reservation when finished.\nsheprd machine list\n    VM slots in use/total/available, then machines and reservations. Exits 4 when\n    there are none.\nsheprd machine set NAME [--cpus N] [--memory GB] [--display fit|WxH]\n    What it boots with next, after it next shuts down.\nsheprd machine suspend NAME\n    Save it and stop it; it resumes where it was when next shown.\nsheprd machine delete NAME\n    Add a notice to review deletion and free its disk. The user confirms in Settings.\nsheprd machine new NAME --from IMAGE\n    Make a machine from an image, as a clone that takes no real space\n    until it changes, with an identity of its own.\nsheprd machine new [NAME]\n    Add a notice to create one from Apple's newest macOS for this Mac\n    (over 20 GB). The user clicks the notice to review it in Settings.\nsheprd machine image [list]\n    The images machines can start from. Exits 4 when there are none.\nsheprd machine image save MACHINE IMAGE\n    Keep a shut down machine as an image. Exits 6 while it runs or is\n    suspended.\nsheprd machine image delete IMAGE\n    Add a notice to review deletion. The user confirms in Settings.\n\nInside a running machine, through Sheprd Agent there (install it once from\nthe Sheprd Agent disk on the machine's desktop, then keep it in an image):\nsheprd machine agent NAME\n    Whether the agent answers: its screen size, and the permissions macOS\n    gave it. Exits 1 when it does not answer.\nsheprd machine exec NAME [--timeout S] [--cwd DIR] -- COMMAND...\n    Run COMMAND as the logged-in user, with no input, for at most S seconds\n    (default 120, at most 600). Prints as cloud exec does; exits 0 when it\n    did, 7 when it was stopped at S, 1 otherwise.\nsheprd machine screenshot NAME PATH\n    Write a PNG of its screen to PATH.\nsheprd machine click NAME X Y [--right] [--double]\n    Click at X,Y in the screenshot's pixels.\nsheprd machine type NAME TEXT\n    Type TEXT as from a keyboard (at most 4096 characters).\nsheprd machine key NAME KEY\n    Press a key, with modifiers: return, escape, tab, cmd+q, cmd+shift+4.\n\nOn another Mac this window reaches, when both slots here are busy:\nsheprd machine --host HOST open|start|claim|release|list|suspend|agent|exec|screenshot|click|type|key ...\n    Run it by the Sheprd on HOST (a host ID or label), which lends its\n    virtual Macs unless it keeps Tart VMs, runs other programs' VMs, or\n    has lending off. Opening, starting, or claiming one first asks the\n    user here whether to borrow (exit 6 when they decline); `open` shows\n    it in a tab here drawn from its frames. A screenshot's PATH is on this\n    computer. Exits 8 when HOST has no Sheprd running or cannot be reached."
        }
        "orchestrator" => {
            "sheprd orchestrator [open] [--repo PATH]\n    Show the Issues section narrowed to the caller's project, or PATH's.\nsheprd orchestrator restart RUN [--repo PATH]\n    Open the confirmation to restart a loaded Herdr 2 run in this project.\n    The user confirms; its checkout is kept and its saved prompt/model reused.\nsheprd orchestrator adopt RUN [--repo PATH]\n    Inspect a legacy run's current process and prepare an explicit adoption\n    confirmation. No control authority is granted until the user confirms.\nsheprd orchestrator items|issues|prs|tasks [--repo PATH]\n    List what it last synced, from its local database (no network).\n    Runs here; Sheprd need not run. Exits 4 before its first sync."
        }
        "review" => {
            "sheprd review [open]\n    Open the review of the caller's workspace's uncommitted changes."
        }
        "chat" => {
            "sheprd chat [status]\n    The window's sheprd-acp chat panes on this machine or an SSH host: pane, agent, status,\n    and whether the native chat view or the terminal shows. Exits 4 when\n    there are none.\nsheprd chat shortcuts\n    The text shortcuts every chat's composer offers, in order: each label,\n    then its text. The same list on every computer and phone that shares\n    them; Settings > Text Shortcuts edits it. Exits 4 when there are none.\nsheprd chat pr [list]\nsheprd chat pr add URL\n    Run as the agent of a sheprd-acp chat: the pull requests the chat's side\n    rail lists, or add one by its GitHub address\n    (https://github.com/OWNER/REPO/pull/N) that you changed without `gh`;\n    `gh pr` commands are picked up on their own. Reaches the chat named by\n    SHEPRD_CHAT_ENDPOINT, which the chat gives its agent; Sheprd need not\n    run. `list` exits 4 when there are none; exits 2 for an address that is\n    not a pull request or outside a chat, and 3 when the chat cannot be\n    reached.\nsheprd chat favorite [on|off|toggle] [--pane ID]\n    Star or unstar the agent in the pane, else the caller's own, else the\n    focused one (default: toggle). Favorites are listed first in the agents\n    panel and on paired phones, whatever the sort; each host's\n    sheprd-remote-control keeps its own. Exits 5 when the pane runs no agent.\nsheprd chat unread [on|off|toggle] [--pane ID]\n    Mark the agent in the pane, else the caller's own, else the focused one,\n    unread (on) or read (off) (default: toggle). An agent is unread while it\n    has a reply no one read on any device; the agents panel and paired phones\n    mark it, and its next reply marks it again. Each host's\n    sheprd-remote-control keeps what was read. Exits 5 when the pane runs no\n    agent.\nsheprd chat show|hide|toggle [--pane ID]\n    Show the native chat view over the pane, or its terminal, for this\n    window. The pane is the caller's own, else the focused one. Exits 5 when\n    it runs no chat the window can show.\nsheprd chat new [AGENT] [--pane ID] [--prompt TEXT | --prompt-file PATH]\n    Open a background tab running a sheprd-acp chat with AGENT (antigravity, claude, codex, copilot,\n    gemini, kimi, muse, opencode, or pi) in the workspace and folder of the pane, else the\n    caller's own. --prompt or --prompt-file (- reads standard input) gives it a\n    brief, at most 96 KB, sent as its first prompt and shown as the calling\n    agent's rather than the person's. Its first line names the task on the\n    calling turn's shared iOS Live Activity.\n    A brief is not yet sent to a chat on an SSH host or another machine's\n    Herdr 2 (exit 8). Run by a chat's agent, it starts a chat like that one: AGENT\n    defaults to the chat's agent, which starts with the chat's model,\n    thinking, permission mode, and auto-accepting. On an SSH host the tab runs that host's sheprd-acp, from\n    ~/.herdr/bin or PATH. A Herdr 2 caller gets a background tab in its workspace and folder. The chat view\n    attaches once the program starts.\n    Exits 8 when ACP chats are off in Settings, sheprd-acp is missing here,\n    the host cannot be reached, or the agent is not installed there.\nsheprd chat convert [--pane ID]\n    Move the terminal agent in the pane, else the caller's own (its Herdr 2\n    terminal, HC_TERMINAL_ID, first), else the focused one, into a sheprd-acp chat in the same pane: stops its CLI\n    with Ctrl+C and continues its session, with its messages, in the chat.\n    Claude, Codex, Copilot, and OpenCode, once idle; a draft in its input\n    is lost. Exits 6 while the agent works and 8 for an agent or pane that\n    cannot move."
        }
        "px0" => {
            "sheprd px0 [open]\n    Open the caller's px0 editor tab in the background on its local checkout.\n    Discovers a running workbench or starts installed px0 lazily.\n    px0.url configures an existing loopback HTTP workbench.\n    Success means the tab opened; server startup errors appear there.\n    Embedded pages require macOS or Windows; remote workspaces are unsupported."
        }
        "notes" => {
            "sheprd notes [list]\n    Every worktree note, most recently edited first.\nsheprd notes get|clear\nsheprd notes set TEXT...\n    The note on the caller's checkout (its repository and branch), or the\n    focused workspace's outside a Herdr pane. Notes are one line of at most\n    500 characters. `list` and `get` exit 4 when there is none."
        }
        "labels" => "sheprd labels [list|add TAG|rename OLD NEW|remove TAG] [--worktree] [--pane ID | --workspace ID]
    Manage the calling agent’s labels on this computer. --pane selects a pane by its
    remote-control id. --worktree selects the agent’s checkout; --workspace selects
    a checkout by workspace id. Every agent in that checkout shows its labels.
    Requires the local remote-control daemon. --json returns the resulting tags.",
        "memory" => {
            "sheprd memory [list] [--all] [--limit N]\n    What agents learned about the current directory's repository: pinned\n    entries first, then the most recently used, of the repository and this\n    branch (--all: every branch's too).\nsheprd memory search WORDS... [--all] [--limit N]\n    Entries holding every word (as prefixes), else any, best first.\n    Search before starting a task.\nsheprd memory index [--chars N]\n    What an agent should see first, within N characters (default 3000):\n    pinned entries, this branch's, then the most recently used.\nsheprd memory get ID\n    One entry with its details.\nsheprd memory add [--branch] [--kind KIND] [--name NAME] [--pin]\n                  [--details TEXT] TEXT...\n    Keep TEXT (at most 500 bytes; more goes in --details, up to 16 KiB) for\n    the next agent in any worktree, tab, or chat of this repository, or with\n    --branch only for this branch's work. KIND is fact (default), gotcha,\n    command, decision, preference, or handoff. With --name, a later add of\n    that name updates the entry instead of adding another. Secrets, tokens,\n    and prompt markup are refused.\nsheprd memory pin|unpin ID\n    Pinned entries come first and are never dropped; at most 50.\nsheprd memory forget ID\n    Remove an entry that is wrong or no longer true.\n    These run here, on the host the repository is on; Sheprd need not run.\n    `list`, `search`, and `index` exit 4 when they find nothing."
        }
        "mcp" => {
            "sheprd mcp [serve]\n    Speak the Model Context Protocol on standard input and output, for an\n    MCP client (Claude Desktop, Claude Code, Codex, Cursor) that starts it.\n    Its tools read and drive what Sheprd shows or keeps, as these commands\n    do. Lists no tools until MCP server is on in Settings > General.\nsheprd mcp config [--client claude-code|claude-desktop|codex|cursor]\n    What to paste into a client so it starts this executable's server."
        }
        "worktree" => {
            "sheprd worktree list\n    List app-kept Git workspaces on the caller's host.\nsheprd worktree create [BRANCH]\nsheprd worktree open\nsheprd worktree delete\n    Open the create, existing-checkout, or delete dialog for the caller's\n    Herdr 2 workspace; the user confirms in Sheprd.\nsheprd worktree [status]\n    Whether the caller's worktree is muted.\nsheprd worktree mute|unmute\n    Silence its notifications, or let them through again. Mutes stay in\n    this app; nothing reaches the daemon."
        }
        "feedback" => {
            "sheprd feedback [--wait SECONDS]\n    Notes the user sent this pane: page, review, and terminal notes that\n    were not typed into it. Exits 4 when there are none."
        }
        "checkpoint" => {
            "sheprd checkpoint [list]\n    The current directory's checkout's checkpoints, newest first.\nsheprd checkpoint create [LABEL...]\n    Take one now.\nsheprd checkpoint diff CHECKPOINT [CHECKPOINT]\n    What changed between two, or since one in the working tree (untracked\n    files included).\n    These run here, on the current branch; Sheprd need not run.\nsheprd checkpoint restore [CHECKPOINT]\n    Open the caller's checkout's checkpoints dialog in Sheprd with\n    CHECKPOINT chosen; the user confirms the restore there."
        }
        "scripts" => {
            "sheprd scripts [list]\n    The current directory's repository's worktree scripts\n    (.herdr/worktree.toml). Runs here; Sheprd need not run.\nsheprd scripts setup|run\n    Run one in a new tab of the caller's workspace (setup: linked worktrees\n    only). Sheprd asks before running a script it has not seen. The\n    archive script runs only when a worktree is removed in Sheprd."
        }
        "teleport" => {
            "sheprd teleport --to HOST\n    Open the teleport dialog for the caller's linked worktree, reviewing\n    HOST (a host ID or label). The user confirms the move there."
        }
        "fan-out" => {
            "sheprd fan-out PROMPT... [--agent KIND[:COUNT]]...\n    Open the fan-out dialog for the caller's repository with PROMPT and\n    these agents (claude, codex, ...; at most 6 in all) chosen once the\n    host's agents are known. The user starts it there."
        }
        "pr" => {
            "sheprd pr [status]|checks\n    The pull request of the caller's workspace's branch, as Sheprd last\n    looked it up (selected host only; sign in to GitHub first). Exits 4\n    when there is none.\nsheprd pr comments\n    Its review comments (the workspace must be the focused one).\nsheprd pr open\n    Open it in a browser tab.\nsheprd pr create\n    Open Create pull request in the Git popup for the focused local\n    checkout, using Sheprd's own GitHub sign-in. A core caller must itself\n    be the focused pane, not just share its workspace. Confirm with a\n    click; Enter is ignored, and changing checkouts cancels the action.\n    Nothing is pushed or created by this command. After confirmation,\n    read the result with pr status. Exits 6 while a menu, dialog, or Git\n    operation is open, 8 for remote or non-Git checkouts.\nsheprd pr merge\n    Open Sheprd's merge dialog for it (the workspace must be the focused\n    one); on a GitHub stack it lands the open pull requests below too. The\n    user picks the method and confirms there. Exits 1 with the reason when\n    it cannot merge yet, 6 while a menu or dialog is open.\nsheprd pr queue add|remove\n    Open the confirmation that adds it (with the open layers of its stack\n    below) to its base branch's merge queue, or takes it out.\nsheprd pr quota\n    Last reported GraphQL points and rate-limit retry delays for this\n    window's GitHub accounts. Reads cached state without a network request."
        }
        "host" => {
            "sheprd host [list]\n    The window's hosts and their status. Herdr's `machine` command\n    manages saved hosts.\nsheprd host mute|unmute [HOST]\n    Silence a host's notifications as a whole, or let them through again,\n    by ID or label; absent, the caller's host. Works while it is offline.\nsheprd host pause|resume HOST\n    Pause a saved SSH or cloud host by ID or label: it stays listed,\n    greyed, but Sheprd disconnects from it until it is resumed. Its own\n    Herdr and terminals keep running. `host list` shows it as disabled.\nsheprd host select ID\n    Show that host in the window, as choosing it in the sidebar does.\nsheprd host tag ID [TAG...]\n    Replace the tags that group that host in the sidebar and on paired\n    phones; with no tags, clear them.\nsheprd host update [HOST]\n    Ask in Sheprd to update the Sheprd app running on an SSH or cloud\n    host, by ID or label; absent, the caller's. The user confirms there;\n    that machine then downloads, installs, and restarts on its own. Exits\n    4 when it offers no newer release. `host list` shows those that do."
        }
        "cloud" => {
            "sheprd cloud [list]\n    Saved cloud machines (Sheprd need not run).\nsheprd cloud add\n    Open the settings that add a machine.\nsheprd cloud setup ID\n    Run the setup script from [cloud] in config-gpui.toml again on a saved\n    machine, named by its ID or label, in a new tab once it is shown.\nsheprd cloud images [list]\n    Sandbox images built from projects' .herdr/sandbox/Dockerfile or dev\n    container (Sheprd need not run). Exits 4 when there are none.\nsheprd cloud images build [PROVIDER]\n    Build the caller's repository's images ahead of a dispatch, for\n    PROVIDER (container, daytona) or every provider that holds them.\n    Settings > Cloud devices shows progress, logs, and Reset.\nsheprd cloud create PROVIDER [--count N] [--template T] [--preset P]\n                        [--sandbox] [--no-install] [--no-setup]\n    Ask in Sheprd to create N machines (at most 16). The user confirms\n    there, once for all of them; prints the batch to follow. Each is ready\n    once saved, with Herdr installed and the [cloud] setup script run.\n    --template and --preset are Coder's, by name; --sandbox is Azure's.\nsheprd cloud exec ID [--timeout S] [--cwd DIR] -- COMMAND...\n    Run COMMAND on a saved machine through the app's own connection to\n    it, with no input, for at most S seconds in all (default 120, at most\n    600). Its stdout and stderr come back as they are, each cut to its\n    end; exits 0 when it did, 1 otherwise. --cwd ~/x is under its home.\nsheprd cloud destroy ID... | --batch BATCH\n    Ask in Sheprd to delete saved machines at their providers and forget\n    them (a provider that cannot delete one only forgets it). The user\n    confirms there, once for all of them; prints the batch to follow.\nsheprd cloud wait BATCH [--timeout S]\n    Each machine of the batch once all are settled, or after S seconds\n    (default 60, at most 600), with its ID once made. Exits 0 when all went\n    as asked, 1 when one failed or the user said no, 4 while it is still\n    going.\nsheprd cloud batches\n    The batches Sheprd keeps."
        }
        "ports" => {
            "sheprd ports [list]\n    Ports the caller's workspace's processes listen on, and forwards.\n    Listening ports are known only while show_listening_ports is on.\nsheprd ports forward|unforward PORT\n    Forward a port of the workspace's SSH host to this machine, or stop.\nsheprd ports open PORT\n    Show localhost:PORT in a browser tab, forwarding it first if remote.\nsheprd ports all\n    Every TCP port this computer's user listens on, whoever started it:\n    process, pid, directory, uptime, and the workspace that started it.\n    macOS and Linux only. Exits 4 when nothing listens.\nsheprd ports stop|pause PORT\n    Ask in Sheprd to stop (SIGTERM, then SIGKILL) or pause (SIGSTOP) the\n    processes listening on PORT of this computer. Nothing is signalled\n    until the user confirms there. Exits 6 while a dialog is open.\nsheprd ports resume PORT\n    Resume (SIGCONT) a paused process listening on PORT."
        }
        "processes" => {
            "sheprd processes [--pane ID | --terminal ID]\n    Show CPU and memory for processes in a Herdr pane, herdr-core terminal,\n    or tmux terminal. Defaults to the calling pane or terminal.\n    Runs here; Sheprd need not run."
        }
        "usage" => {
            "sheprd usage\n    AI plan usage on the window's selected host.\nsheprd usage pick\n    The agent the Agent button would start a chat with now: the one last\n    started or picked, unless its plan is spent or close to its limit and\n    another installed agent's is not. Lists each installed agent's room\n    (ample, unknown, tight, spent) as the status bar last read it. Exits 4\n    when no agent can start."
        }
        "security" => {
            "sheprd security\n    Private vulnerability reports in triage (titles only) and open\n    Dependabot alerts of the github.com repositories the window's\n    workspaces hold, most severe first, as the Security section last read\n    them (sign in to GitHub first). Exits 4 when there are none.\n    `ui panel security` shows the section.\nsheprd security open REPOSITORY ALERT|GHSA\n    Show alert ALERT (its number) or private report GHSA (its GHSA ID) of\n    REPOSITORY (owner/name) in the Security section, beside its list, as\n    the section last read it."
        }
        "automation" => {
            "sheprd automation approve FILE [--host ID]\n    Open the approval of the automation script FILE in Sheprd: its plan,\n    caps, and content hash, for the user to approve or cancel. Nothing is\n    approved by this command. FILE is the script's path on the host, as\n    `sheprd-automate status` lists it; --host names a host by ID or label\n    (default: the caller's host). Exits 5 when the host does not list the\n    script, 6 while a dialog is open. `ui panel automations` shows the\n    section."
        }
        "system" => "sheprd system\n    The window's selected host's load, when Sheprd samples it.",
        "remote" => {
            "sheprd remote [status]\n    Whether remote control is on and its daemon runs: the addresses it\n    serves, its certificate fingerprint, and paired and connected devices.\nsheprd remote devices\n    The paired devices, with when each was last seen. Exits 4 when there\n    are none.\nsheprd remote push-status\n    How iPhone pushes leave this computer: off, or through the push relay\n    (push.sheprd.org), and whether the last alert and Live Activity push\n    worked, then each linked SSH or cloud host's mode, asked there\n    over SSH. Read-only: the push relay is turned off or on in\n    Settings > Remote control.\nsheprd remote test-push\n    Send each paired iPhone registered for alerts one test alert now,\n    through the push relay, and say what came of each, as the phone's own\n    Send Test Push does. Exits 4 when none was sent (the push relay is\n    off, or no phone registered) and 1 when one failed.\nsheprd remote pair [--address HTTPS_ORIGIN]\n    Open the pairing dialog in Sheprd. Tailscale is preferred by default;\n    choose an address in the dialog or pass one from remote status.\n    The QR and its token show only there; the user approves the device there.\nsheprd remote revoke DEVICE\n    Ask in Sheprd to revoke a paired device, named by its ID or name;\n    the user confirms there.\nsheprd remote restart\n    Restart the daemon from the binary beside Sheprd, as after a\n    rebuild or update; phones reconnect on their own. Exits 6 while the\n    service is already changing.\nsheprd remote relays token ENDPOINT [--clear]\n    Keep this computer's listen token for a private relay, read from\n    standard input (never an argument; a terminal is refused), or with\n    --clear forget it. Works before the relay is in [remote] relays.\n    Exits 2 for a bad address or token; the token is never printed.\nsheprd remote add-link\n    Open Add Device > Nearby Sheprd with another computer's pairing link\n    (its Pair device > Copy link), read from standard input (never an\n    argument; a terminal is refused). The user clicks Pair there. Exits 2\n    for text that is no pairing link; the link is never printed.\nsheprd remote shares\n    Active read-only shared sessions on this computer (no link secrets).\n    Exits 4 when there are none.\nsheprd remote share [--pane ID]\n    Open the Share this session dialog in Sheprd for ID (default: the\n    caller's or open chat pane); the owner previews and confirms there.\n    pair, devices, shares, revoke, and restart exit 1 while remote control is off."
        }
        "phone" => {
            "sheprd phone [list]\n    The phones connected that let agents on this computer open screens:\n    the Sheprd app open, with Let agents open screens on for this\n    computer there. Exits 4 when there are none.\nsheprd phone state\n    The screen the phone shows: a tab, or one of this computer's chats,\n    terminals, or workspaces; nothing for another computer's.\nsheprd phone open [PANE]\n    Show a pane on the phone: its chat, else its terminal. Without PANE,\n    your own pane.\nsheprd phone open home|workspaces|prs|settings|new-workspace\nsheprd phone open chat PANE|term PANE\nsheprd phone open workspace|changes [WORKSPACE]\n    Show a tab, a chat or terminal, or a workspace or its changes\n    (default: your workspace).\nsheprd phone targets\n    List Settings pages and highlight targets.\nsheprd phone open settings [PAGE]\nsheprd phone highlight TARGET\nsheprd phone guide TEXT [--highlight TARGET]\nsheprd phone guide --clear\n    Open a Settings page, point out a control, or update one persistent\n    banner (three lines including source). --clear dismisses it; a tap\n    dismisses it and opens your own chat. These commands do not change\n    settings.\nsheprd phone show TEXT\n    Put a note in front of the person on the phone, from this\n    computer's agent; a tap on it opens your own chat.\n    Every command takes --device ID|NAME; the phone used last is the\n    default. Exits 1 when the phone refuses (locked, or agents turned\n    off), 2 for a blank or overlong note, 5 for a pane or workspace not\n    here, 7 when it does not answer."
        }
        "fleet" => {
            "sheprd fleet [status]\n    The fleet as this computer's daemon keeps it: members, hosts, who\n    admitted or added each, and how many computers may reach each host.\n    Exits 4 when this computer is in no fleet.\nsheprd fleet members|hosts\n    One of those lists.\nsheprd fleet open\n    Show Settings > Fleet, where computers this Sheprd reaches are\n    invited and hosts shared.\nsheprd fleet remove MEMBER\nsheprd fleet remove-host HOST\n    Ask in Settings > Fleet to remove a member (by ID or name; the user\n    picks why there, which decides what goes with it) or a host\n    everywhere. Nothing is removed until the user confirms there.\nsheprd fleet scrub MACHINE\n    Ask in Settings > Fleet to remove every way a fleet reaches a machine\n    this Sheprd connects to (by device ID or name): its authorized_keys\n    lines and the relay agent's entries, over SSH. The last resort for a\n    host no member can reach or a fleet that is gone; the user confirms\n    there.\nsheprd fleet invite MACHINE|--all\n    Ask in Settings > Fleet to invite a computer this Sheprd reaches that\n    is in no fleet (by device ID or name), or every such computer at once.\n    The user confirms there.\nsheprd fleet join [MACHINE]\n    Offer in Settings > Fleet to join the fleet a computer this Sheprd\n    reaches is a member of, while this computer is in none. The user\n    clicks Join there."
        }
        "mod" => {
            "sheprd mod [list]\n    The mods the window runs, where each draws, and any error. Mods are\n    folders in <config>/herdr/gpui-mods; saving a file reloads its mod.\n    Exits 4 when there are none.\nsheprd mod reload NAME\n    Read a mod's files again now.\nsheprd mod open NAME\n    Show a mod's activity bar app in the window's body. Exits 5 when no\n    running mod of that name has one, 8 while the activity bar is off.\nsheprd mod panel [show|hide|toggle]\n    The panel of mod sections on the window's right: whether it shows and\n    which mods have a section, or show, hide, or toggle it.\nsheprd mod logs NAME [--lines N]\n    The last lines it wrote with herdr.log (default 50, at most 200).\nsheprd mod check DIR\n    Load a mod folder, run its init and view once, and report what it\n    draws. Runs here; Sheprd need not run. Exits 1 with the error and\n    its line when the mod is invalid."
        }
        "search" => {
            "sheprd search symbol|file QUERY... [--limit N]\n    Go to Symbol or Go to File over the current directory's checkout,\n    printing paths relative to its root. Runs here; Sheprd need not run."
        }
        "config" => {
            "sheprd config [path]\n    The settings files.\nsheprd config get KEY\n    The setting's value, as TOML: your override, else the default.\nsheprd config set KEY VALUE\nsheprd config unset KEY\n    Change a setting in config-gpui.local.toml. VALUE is read as TOML\n    (14, true, [\"a\"], { show = true }), else as text. A value or key\n    Sheprd would not load is refused and the file left as it was.\n    A running Sheprd applies the change at once. KEY is dotted, such\n    as terminal.size. Unsetting a key that is not set exits 4.\nsheprd config check\n    Report settings that do not load and unknown keys, and, when Sheprd\n    runs, fonts that are not installed; exits 1 when there are any.\nsheprd config reload\n    Reread Herdr's config and this app's.\nOnly reload needs Sheprd running."
        }
        "theme" => {
            "sheprd theme [list]\n    The themes there are; * marks the selected one.\nsheprd theme set NAME...\n    Choose and save a theme, or light:NAME,dark:NAME to follow the\n    system appearance. Saved even when Sheprd is not running."
        }
        "font" => {
            "sheprd font [list]\n    The installed font families.\nsheprd font set NAME...\nsheprd font reset\n    The font of every face; reset goes back to the default.\nThese need Sheprd running, which knows the installed fonts."
        }
        "language" => {
            "sheprd language [status]\n    The language set, and the locale Sheprd shows: the best match for\n    the setting, else en-US.\nsheprd language list\n    The languages available for direct selection, each named in itself,\n    and the en-XA and en-XB pseudo-locales.\nsheprd language set system|TAG\n    Save the language setting: system follows the OS's languages; TAG is\n    a BCP 47 tag such as fr-CA. A running Sheprd switches at once.\nOnly status asks Sheprd; without it, status reports what it would show."
        }
        "keybinds" => {
            "sheprd keybinds [list]\n    The window's shortcuts.\nsheprd keybinds check\n    Herdr's custom commands this client cannot reach; exits 1 if any."
        }
        "caffeine" => {
            "sheprd caffeine [status]\nsheprd caffeine on|off|toggle [--for 90s|30m|2h]\n    Keep the display and the machine awake. With --for (at most 24h),\n    caffeine turns off then, unless it was changed in the meantime."
        }
        "sound" => {
            "sheprd sound [list]\n    The notification sounds.\nsheprd sound play [done|request]\n    Play one now, from the file Herdr's sound settings name or the\n    built-in one."
        }
        "bell" => "sheprd bell\n    Ring the system bell, as a terminal bell does.",
        "events" => {
            "sheprd events [--after CURSOR] [--wait SECONDS] [--kind KIND]... [--follow]\n    What happened since CURSOR, the INSTANCE:SEQ the last call printed:\n    windows and tabs opening and closing, notes, host status, and cloud\n    batches. A cursor\n    from an earlier app run gets everything this run kept. --follow keeps\n    printing until interrupted. Kinds:\n    window_opened, window_closed, window_focused, tab_opened, tab_closed,\n    feedback, note, host, cloud_batch."
        }
        "wait" => {
            "sheprd wait feedback|window|online|tab-closed ID [--timeout SECONDS]\n    Wait (default 60, at most 600 seconds). Exits 4 when time runs out."
        }
        "run" => {
            "sheprd run COMMAND\n    Run an app command by its [keybindings] name, as the command palette\n    does, for example toggle_sidebar. See `commands`."
        }
        "commands" => "sheprd commands\n    The commands `run` accepts, with their shortcuts.",
        "logs" => {
            "sheprd logs [--lines N] [--path]\n    The app's latest log records (default 100, at most 5000), or the log file's path.\n    Runs here; Sheprd need not run. Exits 4 when there is no log yet."
        }
        "update" => {
            "sheprd update [status]\nsheprd update check\n    Ask for the latest release. Never downloads or installs. Local\n    builds report updates as off.\nsheprd update install\n    Show the app's update dialog; the user installs from there.\n    Another Sheprd that reaches this machine over SSH runs it with\n    --confirmed once its user approved the update there, and the app\n    then downloads, installs, and restarts. --confirmed is refused from\n    this app's own panes and terminals."
        }
        "diagnostics" => {
            "sheprd diagnostics bundle PATH\n    Write build information, settings diagnostics, your settings\n    overrides, and recent logs to a new file at PATH for a bug report,\n    with home paths shortened and secret-looking values redacted. Read\n    it before sharing. Never replaces an existing file. Runs here."
        }
        "integrations" => {
            "sheprd integrations [list]\n    The agent integrations Herdr offers the window's host. Install them\n    from Sheprd's integrations settings.\nsheprd integrations install-skill\n    Install or update the agent skill for Claude Code and other agents.\n    Runs here; Sheprd need not run."
        }
        _ => "",
    }
}

/// Usage for one family, or the list of families.
pub(super) fn usage(name: Option<&str>) -> String {
    match name {
        Some("browser") => BROWSER.to_owned(),
        Some(name) => format!("{}\n\n{COMMON}", family(name)),
        None => {
            let mut text = String::from("sheprd COMMAND ... [--json] [--window ID]\n\n");
            for (name, summary) in FAMILIES {
                text.push_str(&format!("  {name:<14}{summary}\n"));
            }
            text.push_str("\nRun `sheprd COMMAND --help` for one command's usage.\n\n");
            text.push_str(COMMON);
            text
        }
    }
}
