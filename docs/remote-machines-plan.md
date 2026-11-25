# Virtual Macs on other computers

**Phase 1 is built** (forwarding, lending, the borrow dialog, and watching);
placement and copying images are not. Where the build differs from the
first plan, the sections below say what was built. macOS runs at most two virtual Macs per
computer, so two agents testing in VMs fill this Mac. This plan lets an
agent use a free virtual Mac on another Mac the window already reaches,
with the same `sheprd machine` commands and the same claims.

## Where things stand

- A virtual Mac runs inside the Sheprd app that started it
  (`crates/sheprd-gpui/src/machine/vm.rs`, Virtualization.framework). Its
  picture (`VZVirtualMachineView`) and its vsock device, which Sheprd Agent
  answers on (`machine/agent.rs`), exist only in that process.
- `MAX_ACTIVE_MACHINES = 2` (`sheprd-control-protocol`, `protocol/tabs.rs`)
  and `vm::capacity` count only this app's machines and a live installer.
  VMs of other programs, such as the Tart CI runners on the Mac minis
  (`docs/self-hosted-runners.md`), take the same slots unseen; starting
  then fails in the framework.
- Claims are kept by the running app, per Herdr pane or core terminal
  (`control/server/tabs/machine_claims.rs`). A caller without either
  cannot claim.
- The app already runs another machine's Sheprd command line over its
  host connection and reads its `--json` answer: `remote_update/fetch.rs`
  (`script`, the exit-code table, bounded output, strict parsing).

## Decisions

| Question | Decision |
| --- | --- |
| Who runs a remote VM | That Mac's own Sheprd. Nothing here can run it: the framework, the view, and vsock are per process. |
| How this app reaches it | `sheprd machine lend` over the existing host connection (SSH, through relays when needed, sharing one login per host), as `remote_update` runs `sheprd update`: one request as JSON on its stdin, one answer as JSON on its stdout (`sheprd-control-protocol` `protocol/lend.rs`). No new daemon, transport, or crate. |
| Why not the remote control API | Fleet members do not get the remote control API on each other's daemons (`docs/relay-plan.md`, What joining grants). SSH is the access the user already granted. |
| Who may borrow | Every Mac lends by default: `[machine] lend`, on unless set off (in the config file; Settings does not show it yet). A Mac that runs VMs of other programs, such as Tart CI runners, never lends, whatever the setting: their VMs share its two slots (Hosts that never lend). |
| Who decides to borrow | The user, once per host. The first time placement would use another Mac, the app asks: "Virtual Macs here are full. Use a free slot on mini-2?" with **Use once**, **Always use mini-2**, and **Wait for a slot here**. An agent cannot answer it. |
| What is copied | Nothing by default: each Mac makes machines from its own images. When the only way is copying an image, the app measures the link first and asks, with the size and the time it would take (Copying an image). |
| Remote app not running | A named host answers exit 8, "Sheprd is not running on mini-2"; placement will skip it. Never launched over SSH: it needs the user's GUI login. |

## Commands

`open`, `start`, `claim`, `release`, `list`, `suspend`, `agent`, `exec`,
`screenshot`, `click`, `type`, and `key` take `--host HOST`, a host ID or
label as `host update` takes (built: `TabOp::MachineOn`). Making, deleting,
and changing a machine stay with its own Sheprd. Without `--host`, nothing
changes: this computer.

| Command | State |
| --- | --- |
| `machine start NAME` | Built. Boots without a tab, which is what a forwarded `open` does: nobody is at the other Mac. Locally, `open` keeps opening a tab. |
| `machine lend` | Built, hidden. What the borrower runs there. Screenshots and frames are composed by this command from a screenshot the app writes into a private folder, since the control socket's messages stay under 256 KB; the borrower bounds the answer (96 MiB), checks the PNG is the size it says, and writes PATH here. |
| `machine list --all` | Phase 2. Each lending host's capacity and machines, this computer first. A host that does not answer is listed with its reason, not left out. |
| `machine claim --any [--image IMAGE] [--wait S]` | Phase 2. Places the claim: an unreserved running machine here, then a stopped one with a free slot here, then the same on lending hosts, most free slots first. `--wait` polls every host every five seconds from this app, not one long SSH command. |

Exit codes stay those of `docs/cli.md`; a host that cannot be reached,
has no Sheprd running, or has one too old to lend is exit 8.

## Asking before borrowing

Borrowing uses another computer's memory, CPU, and one of its two slots,
so it is confirmed in the app's own dialog, as other outward actions are
(`AGENTS.md`, Command Line). The dialog names the host, the machine, its
CPUs and memory, and how the host is reached (direct, LAN, relay).

- **Use once** places this claim there.
- **Always use mini-2** is kept per host (`[machine] borrow_from`), so
  later claims go there without asking. Settings > Virtual Macs lists them
  and removes them.
- **Wait for a slot here** turns the claim into `--wait` on this Mac.

Built: opening, starting, or claiming on a host not in `borrow_from` asks
(`machine/borrow.rs`, the menu's `Page::BorrowMachine`); the request waits
up to two minutes for the answer, and **Wait for a slot here** or closing
the dialog exits 6. The skill tells the agent to say it is asking, as the
user may be in the chat rather than at the window. The dialog does not yet
show the machine's CPUs and memory or how the host is reached, and
Settings does not yet list `borrow_from`.

## Hosts that never lend

A Mac whose slots other programs use cannot keep its promise: placement
would see a free slot that a CI job takes a minute later. So a host does
not lend, and `list --all` says why, when any of these holds:

- Another program's VM is running (Counting every VM on a host).
- Tart has VMs (`$TART_HOME/vms`, else `~/.tart/vms`, is not empty), even
  when none runs: a CI runner starts them on demand.
- `[machine] lend = false`.

The check runs on the lending Mac when a borrower claims or starts a
machine there (`machine/lend.rs`, `machine_claims::not_lending`), and
refuses with the reason (exit 8). With lending off, a borrower may still
list and release, so it learns why and can let go; `machine list --json`
reports `lending`.

## Owners across computers

A forwarded request arrives over SSH, with no pane. `machine lend` sends
it to the app as relayed by the borrower (`Via::Lend { borrower, owner }`,
where `borrower` is this computer's host name and `owner` the pane or
terminal that asked), with no pane, terminal, or window of the lending
computer. The lending app:

- Serves only virtual Mac work from it (`TabOp::lendable`,
  `control/server/relay.rs`), never a tab, dialog, or deletion.
- Keeps its claims under their own `Owner` field, which never equals a
  local pane's.
- Applies the same rules: 15 idle minutes, renewed by guest commands, only
  the owner releases.

SSH as that account already controls the app, so this grants nothing new.
A borrower that quits leaves its claims to expire; the VM keeps running.

## Counting every VM on a host

Placement trusts `capacity`, so it must count all VMs, not only Sheprd's.
Each running Virtualization.framework VM is one
`com.apple.Virtualization.VirtualMachine` XPC process (seen on macOS 26).
Built: `machine/lend.rs` counts them, and `capacity` reports `in_use` as
the larger of those and Sheprd's own machines and installer, with
`others` naming how many belong to other programs. Claims and starts check
against it, so a slot a Tart VM holds is no longer offered. Not yet: a
start the framework refuses for the limit becoming `Error::Capacity`.

## Watching a remote machine

A tab for a machine on another Mac, from the start. The real view cannot
cross processes, so the tab draws the guest's screen from Sheprd Agent and
sends input back through it, as `machine click`, `type`, and `key` do.

- Built differently: each frame is one `machine lend` run, on the SSH
  connection the host already shares, rather than one long-lived stream,
  so a stalled frame never wedges a channel and nothing new is kept open.
- Built differently: the lending Sheprd makes the frame from Sheprd
  Agent's PNG (`machine/remote/frame.rs`): the screen scaled to the width
  asked for, as JPEG, and nothing when its pixels have not changed. Sheprd
  Agent is unchanged, so machines saved as images keep working.
- The watcher asks for frames one at a time, so a slow link lowers the
  frame rate instead of queuing: at most one frame in flight, the latest
  one drawn. It starts at the tab's width and halves it while frames take
  longer than half a second.
- Watching does not need the claim. Clicks and keys go as "a person
  watching" and follow the claim's rules: while an agent holds the
  machine, they are refused and the tab says who holds it.
- The tab says which host it is on, and says the link is slow when a
  frame takes over a second. Frames come only while the tab is drawn.

## Copying an image

Only when the user asks for a machine from an image the other Mac lacks.
A used image is 20–40 GB even sparse, so it is never copied without the
user's answer.

1. **Measure.** Before asking, send 16 MB through the same SSH channel and
   time it, after a 2 MB warm-up. Read how the host is reached: direct on
   the LAN, Tailscale (direct or DERP), or a Sheprd relay. Keep the result
   for ten minutes per host.
2. **Ask.** The dialog says the image, the bytes to send, the measured
   speed, and the time it would take: "Copy macos-26-agent (24 GB) to
   mini-2? About 3 minutes over your network." Over a relay, Tailscale's
   DERP, or an estimate beyond 30 minutes it warns instead, "About 4 hours
   over a relay", and makes **Wait for a slot here** the default button.
   Below 1 MB/s it refuses and says why.
3. **Copy.** Only the blocks the disk uses (it is sparse), compressed,
   with progress in the app and a checksum checked on arrival before the
   image is listed there. Cancel leaves nothing behind on either side.

The measuring and asking come before any byte of the image is sent;
nothing here retries on a slower path by itself.

## Code

| Piece | Where |
| --- | --- |
| `TabOp::MachineOn`, `MachineStart`, `Via::Lend`, `Lending`, `capacity.others`, the lend wire | `sheprd-control-protocol` `protocol/tabs.rs`, `protocol.rs`, `protocol/lend.rs` |
| `--host`, `start`, and the hidden `lend` | `cli/control/tabs.rs`, `control/client/tabs.rs`, `control/client/tabs/lend.rs` |
| Forwarding: resolve the host, ask, run, map exit codes, write screenshots | `control/server/tabs/machine_remote.rs`, running `machine/remote/link.rs` with `remote_update::script` |
| Lent requests: what is served, and while lending is off | `control/server/relay.rs` (`lent`) |
| Remote owners and the lending check at a claim or start | `control/server/tabs/machine_claims.rs` |
| All-VM count, Tart, and whether a Mac lends | `machine/lend.rs` |
| `[machine] lend` and `borrow_from` | `config/machine.rs` (Settings does not show them yet) |
| The borrow dialog | `machine/borrow.rs` |
| Frames, and the watcher's pacing and input | `machine/remote/frame.rs`, `machine/remote/watch.rs` |
| The remote machine tab | `browser/remote_machine_tab.rs`, `Location::RemoteMachine` |
| Phase 2 and 3: lending hosts' last answers, placement, link measurement, image copy | not built |

Tests built: lent requests limited to virtual Mac work and to listing and
releasing while lending is off; borrower names bounded; forwarded exit
codes; answers parsed strictly; pictures written only when they are the
PNG they say; capacity and lending with other programs' VMs and Tart's
folder; the borrow answer and dismissal; frame scaling, change detection,
and pacing; click mapping through the letterbox; key mapping; `--host`
parsing, including words after `--`. Still to test: placement order, and
the copy refused below its speed floor and cleaned up after cancel.

## Skill and docs

Built: `skills/herdr-gpui-browser/SKILL.md`, "Sharing the two macOS VMs",
looks for a free slot on another Mac with `--host` before waiting, tells
the user it is asking, and never borrows without asking. `docs/cli.md`
has the commands and "Virtual Macs on other computers". Phase 2 replaces
the skill's manual search with `machine claim --any`.

## Phases

1. **Forwarding and watching.** `--host` on every subcommand, remote
   owners, `start`, screenshot over stdout, `lend` with the hosts that
   never lend, the borrow dialog, and the remote watching tab. An agent can
   name a host by hand, and the user can see what it does there.
2. **Placement.** All-VM counting, `list --all`, `claim --any [--wait]`,
   and the skill's new flow.
3. **Copying images.** Measurement, the copy dialog, and the copy.
