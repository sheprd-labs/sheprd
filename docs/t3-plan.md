# Plan: T3 Code threads in Sheprd

T3 Code (`pingdotgg/t3code`, MIT) is a server that runs coding agents and
keeps their conversations as threads; its desktop, web, and phone apps are
clients of it. People who use it keep their threads there. This plan lets
Sheprd show and drive those threads beside Herdr's terminals and our own
chats, the way Sheprd is a client of Herdr: T3 keeps running the agents and
owning the threads, and Sheprd never changes T3. Read `AGENTS.md` first;
its rules apply throughout.

The direction is one way: Sheprd is a T3 client. Serving T3's own apps from
Sheprd is not part of this plan.

Facts below were read from T3 Code `0.1.9`, commit `ac5dd1ff` (2026-11-15).
T3 publishes no wire spec; its protocol is defined by Effect `Schema`s in
`packages/contracts/src`, so every type we port names the file and line it
came from, and `sheprd-t3` pins the versions it was tested against.

## What T3 offers a client

| Need | T3 |
| --- | --- |
| Find a local server | `<T3 home>/…/server-runtime.json` (`apps/server/src/config.ts:174`); default port 3773, the next free one when taken |
| Describe it | `GET /.well-known/t3/environment`: environment id, protocol version, capabilities |
| Sign in | A pairing token (`t3 auth pairing create`, or a pairing link), exchanged at `POST /oauth/token` (RFC 8693, `subject_token_type=urn:t3:params:oauth:token-type:environment-bootstrap`) for a Bearer access token with scopes `orchestration:read` and `orchestration:operate` (`contracts/src/auth.ts:169-240`). DPoP is optional |
| Open the socket | `POST /api/auth/websocket-ticket/rpc` with the token, then `GET /ws?orchestrationProtocol=2&wsTicket=…`. Any other protocol version gets HTTP 426 |
| Frames | Effect RPC as JSON: `Request {id, tag, payload}`, then `Chunk {requestId, values}` and `Exit`; the client acks each chunk |
| Thread list | `orchestration.subscribeShell {afterSequence?}`: a snapshot of projects and threads, then `thread.updated` / `project.updated` with sequence numbers |
| One thread | `orchestration.subscribeThread {threadId, afterSequence?}`: a snapshot at `snapshotSequence`, then events with greater sequences; a fresh snapshot when the cursor fell below the pruned log. Unknown event types are skipped but still advance the cursor |
| Act | `orchestration.dispatchCommand`: `message.dispatch`, `runtime-request.respond` (`accept`, `acceptForSession`, `acceptAlways`, `decline`, `cancel`), `run.interrupt`, `provider.switch`; each command carries a client-chosen `commandId`, so a retry is never applied twice |
| Start a thread | `orchestration.launchThread {projectId, title, modelSelection, workspaceStrategy, initialMessage, …}` |
| Fallback | `GET /api/orchestration/threads/:id/transcript`, and the MCP server at `/mcp` (OAuth, polling) |

## Decisions

| Question | Decision |
| --- | --- |
| Transport | The WebSocket RPC, which is what T3's own apps use and the only route with live events. MCP is too slow for a chat view and is kept only as a fallback idea. |
| Runtime | Blocking: `tungstenite` over `std::net` (`rustls` for `https`/`wss`) on a worker thread, as `sheprd-core-client` does. No Tokio. |
| Where it lives | `crates/sheprd-t3`: the wire types, sign-in, the connection worker, and the projection into `sheprd-chat-protocol`; no GPUI. Its `sheprd-t3` binary is the long-lived part (below). |
| What a thread looks like in Sheprd | A chat. `sheprd-t3` serves each opened thread on the existing attach wire (`sheprd-chat-protocol`), so the desktop chat views, the in-pane view, and the phone's chat routes show it without a T3-specific renderer. |
| Projection | T3 messages, tool calls, plans, and runtime requests map to `Entry`, `Activity`, `PlanEntry`, and `Permission`; prompts and answers map back to `message.dispatch` and `runtime-request.respond`; model choices map to `ConfigOption` and `provider.switch`. Anything with no mapping becomes a `Notice` entry, never a dropped event. |
| Session model | A new `Keeper` in `sheprd-session`: T3 projects become workspaces, threads become chats in them, under the host the T3 server runs on. |
| Credentials | Access tokens in `sheprd-credentials`, one per T3 environment id. A pairing token is used once and never stored. |
| Local sign-in | Sheprd offers to run `t3 auth pairing create` itself only after the user confirms, and only for the server it found on this computer. Remote servers pair from a pasted link or code. |
| Remote servers | Over the routes Sheprd already has: a saved SSH, cloud, or WSL host forwards the port; T3's own LAN, Tailscale, and T3 Connect addresses are accepted as typed. |
| Versions | Refuse a server whose `orchestrationProtocol` is not 2 or whose snapshot `schemaVersion` is unknown, with a message naming both versions, rather than guessing. |
| Role beside Sheprd's own chats (2026-11-21) | T3 is an optional backend next to `sheprd-acp` chats, never a replacement; `sheprd-acp` chats stay the default. Whenever a T3 server is running, Sheprd can show its threads, steer them, and create new ones in it. Sheprd never starts, installs, or patches T3: it uses a server that already answers. |
| Command ids | A dispatched command's `commandId` is `sheprd:<chat command id>`, and a prompt's `messageId` adds `:message`. T3 keeps a receipt per `commandId` and answers a repeat with the first answer (`Orchestrator.ts:11742`, recorded in `steer.json`), so a client sending the same chat command again never applies it twice; `sheprd-acp t3` also keeps T3's last 256 answers and gives a repeat the first answer rather than judging it again. Commands go to T3 one at a time, each judged once the thread shows the one before (a Stop right after a prompt stops the run the prompt started), waiting at most two seconds. |
| Steering a thread | Prompts are `message.dispatch`, run now or queued behind the running turn as chat prompts are; Cancel is `run.interrupt` with `holdQueue`; an approval is answered with T3's decisions (`accept`, `acceptForSession`, `decline`, `cancel`, or the ones the request names); one question at a time is answered by question id with the option's value. A chat's files go into the prompt as paths, since T3 takes uploads only and the server is on this computer. Several questions at once, modes, and queued-prompt edits stay in T3 for now. |
| Starting a thread | The project for a checkout is the one whose root is its repository's main folder, found in the thread list by path, else created with `projects.mutate {type: "project.create"}` (`rpc.ts:1251`; T3's own client creates projects there, not through `dispatchCommand`, and refuses a second project for one root). The repository is read from the `.git` folder or file alone, with no `git` process. A linked worktree, such as one Sheprd made, becomes `existing_worktree`; `--base REF [--branch NAME]` asks T3 for a new worktree, which it puts under its own home; anything else is `root`. The model is the one asked for, else the project's default, else the first ready provider's default. Without a title T3 names the thread from its first message. A repeated launch `commandId` answers the same thread (`resumed: true`). |
| Model choices | The thread's model is a `model` setting listing its provider's models from `server.getConfig`, and, with more than one usable provider, a `provider` setting (shown as Agent) picks another; a model of the same provider is `thread.model-selection.set`, another provider is `provider.switch` with its default model, as T3's own client decides. Providers that are disabled, in error, or have no model are not offered. |

## The `sheprd-t3` daemon

One process per T3 environment, started by the app when it needs one and
left running when the app quits, like a chat program:

- holds the one WebSocket and its token, reconnects with backoff, and resumes
  every subscription from its last sequence;
- keeps the shell subscription, so thread state (running, waiting for an
  answer, finished) is current for the sidebar, the activity dots, and the
  remote control daemon's pushes even with no window open;
- serves each opened thread as an attach endpoint, discoverable as chat
  endpoints are today;
- answers a small control socket: list threads, open one, start one.

Bounds: one connection per environment, bounded event queues with the
coalesced latest-state mailbox for the shell, at most 16 attached threads,
transcript windows as `sheprd-acp` keeps them.

### Where an opened thread shows

Two ways, to decide in phase 2:

1. **In a herdr-core pane**: the app starts `sheprd-t3 open THREAD` in a
   pane labelled `acp=<endpoint>`. Restore after a reboot, the activity dot
   (OSC 7501), the phone, and splits work as they do for `sheprd-acp`
   chats. Cost: a pane per thread.
2. **As an app tab with no pane**, like a browser tab. Lighter, but restore,
   the phone, and the activity dot need their own paths.

Proposal: 1, because everything that already follows chats follows it.

## Phases

Each phase is its own pull request.

1. **Read-only client.** `crates/sheprd-t3`: discovery, pairing exchange,
   the RPC framing with acks, `subscribeShell` and `subscribeThread`, and
   the projection, tested against recorded frames from a pinned T3 server.
   A `sheprd-t3 threads --json` command proves it end to end.
2. **Show threads.** The daemon, the `Keeper`, a T3 section in the sidebar
   and the activity bar, opening a thread as a chat (read-only).
3. **Steer.** Prompts, cancel, approvals, model and provider switch.
   Done in `sheprd_t3::steer` and `sheprd-acp t3`, which now takes a chat's
   commands, and `sheprd-t3 send` and `stop` for scripts, tested against
   `fixtures/steer.json` (T3 Code 0.1.9).
4. **Start threads.** Whenever a T3 server is running, a new thread from
   scratch: the project for a checkout found by path or created
   (`project.create`), then `orchestration.launchThread` with T3's
   workspace strategy; then from Sheprd's new-chat flow. Done in
   `sheprd_t3::launch`, `sheprd-t3 new`, and `sheprd-acp t3 --new`, which
   starts a thread and follows it in its pane, tested against
   `fixtures/launch.json` (T3 Code 0.1.9). The new-chat flow is the app's
   part, below.
5. **Remote and phone.** T3 servers on saved hosts; T3 threads in Sheprd
   Remote through the remote control daemon.

Each phase adds its `sheprd t3 …` commands, `docs/cli.md`, and, once
agents should use them, the skill's table.

## Testing

- Recorded frames from a real T3 server at the pinned commit, checked in as
  fixtures with nothing private in them, replayed through the projection.
- Malformed and oversized frames, unknown event types, a cursor below the
  pruned log, reconnect mid-stream, and an expired token.
- An opt-in live test, `just test-t3 /absolute/path/to/t3`, in the shared
  sandbox with its own T3 home; never a user's running server.

## Not in scope

- Serving T3's apps from Sheprd.
- Moving threads between T3 and `sheprd-acp`. A T3 thread could later be
  handed to a Sheprd chat with the handoff from `docs/threads-plan.md`.
- T3's terminals, previews, and pull request flows: Sheprd has its own.
- Patching T3. Anything it lacks is an upstream issue, not a fork.

## Risks

- **The protocol moves.** Some 23 orchestration methods and about 150 more
  change without a spec. We port only what the phases use, tolerate unknown
  fields and events, and pin the versions we test.
- **Sign-in may change.** T3's environment auth is recent and evolving
  (cookies, Bearer, DPoP, T3 Connect). We use Bearer only and refuse rather
  than half-work when it changes.
- **Licensing.** T3 is MIT; we copy no code, only follow the wire.
