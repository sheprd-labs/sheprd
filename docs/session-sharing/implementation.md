# Implementation progress

The phased build was authorized on 2026-11-22. Host policy, read projection,
a bounded worker, an encrypted relay gateway, daemon control wiring (`sharing::service`
over live `Hub` chats and trusted `Relays`), the native **Share this session**
dialog with live guest preview, chat header Share/Sharing control, and
`sheprd remote share` / `sheprd remote shares` CLI commands are implemented.
No share route starts on daemon boot; shares are created only after explicit
owner confirmation in the dialog. A relay with the browser viewer enabled serves the browser viewer from its own origin. The native
"Shared with me" recipient view remains follow-up work. The
[plan](../session-sharing-plan.md) remains the roadmap;
[gateway v0](gateway.md) records the implemented wire flow.

## Host boundary

`sheprd_remote_control::sharing::Store` is a blocking, single-writer component
for the daemon worker, behind its existing `server` feature. It adds no dependency,
service, runtime, or fleet access. Its owner operations create shares, mint view
invitations, preview, revoke individual grants, and stop one or all shares.

A grant has a random 256-bit capability returned once. Only a domain-separated
SHA-256 verifier reaches the private state file. Admission is scoped to the
share and grant, and does not confer any owner operation. An admitted `Guest`
is an opaque, non-serializable handle. Every read checks live authorization and
the endpoint, source epoch, and stable agent session ID. Source replacement or a
regressed source sequence closes the share. A supplied resume cursor is never
an access credential.

The guest read schema is separate from the attach command schema and rejects
unknown fields and unsupported versions. There is no route to a prompt,
approval, terminal, shell, settings, fleet, file, or artifact command. This follows
[OWASP's authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
on denying by default and checking access at every request.

State writes use a private temporary file, fsync, atomic replacement, and directory
sync on Unix. A separate file lock prevents two writers. Input is size-bounded,
versioned and semantically validated; unknown or corrupt records are refused.
A write failure disables the store and every admitted reader. Revocation is never
reported as successful before persistence succeeds.

Conservative restart behavior: opening the store durably stops every previous
share, even if it was active. The owner must reshare. This prevents clock changes,
failed prior writes, or reused attach endpoints from resurrecting old links.
Transparent restart/resumption is not implemented. A backward wall or monotonic
clock closes live access; expiry checks both clocks, so a stalled wall clock does
not extend a grant. The gateway must supply actual host clocks.

## Guest content

The allowlist includes user text (without attachment suffixes or hidden context)
and agent text. Tool text is an explicit opt-in; its source call IDs, parent IDs,
file locations, terminal handles, diffs, images and timestamps are excluded.
Thoughts, notices, permissions, queue contents, activities, session paths/titles,
usage, options, agent switching and automatic approval state are excluded.
Failure and exit states carry no internal diagnostic message. This is a content
boundary, not an automated secret redactor: selected text can contain secrets.

“From now” records the source's last entry ID. All earlier entries stay excluded,
including tools replaced after sharing begins. An empty, truncated source has no
reliable boundary and is refused for “From now”; snapshots older than creation
are refused as well. Preview, live reads and reconnect
snapshots use the same filter. Public entry IDs and cursor epochs are independent
of the source. Deltas are generated from two filtered snapshots and use the existing
transcript reducer. A gap, incompatible cursor or structural change gets a complete
filtered snapshot. Hidden changes do not advance the public cursor.

Initial limits are intentionally below the Phase 0 ceilings: 8 shares, 16 grants
and 16 viewers per share, at most one hour, 1 KiB read requests and 256 KiB encoded
responses. Projections retain at most 256 entries and 128 KiB of encoded entry
content; individual text blocks are capped at 16 KiB on UTF-8 boundaries. Older
content is marked truncated. Revoked grants count toward the grant limit until
that share ends. No replay archive or transcript is written to disk.

## Opening a browser link

The dialog and CLI call the same host service. It selects a trusted HTTPS relay,
checks the advertised viewer capability and configured QUIC pin, subscribes to
the chat feed and starts a dedicated listener with the host's listen token.
It returns the link after the relay acknowledges the grant's dial key.

The relay embeds the landing page and worker bundle. The fragment stays in
memory until Join. A dedicated WASM entry point, `sheprd-share-viewer`, keeps
the daemon and native networking out of the browser build. It shares the
invitation envelope, admission proof encoding, relay challenge, hybrid tunnel,
read schema and transcript reducer with the native implementation. The browser
renders user, agent and opt-in tool text with an inert Markdown subset.

For local acceptance, install Python Playwright and its Chromium browser, then
run `python scripts/test-share-viewer.py` from the repository root. Set
`CHROMIUM` to use an existing Chromium executable. The fixture uses temporary
loopback TCP/UDP ports, a temporary TLS certificate, synthetic source data and
a private temporary store. Add `--screenshots DIR` to capture the viewer states.

## Remaining work

The host has one route and one selected relay per share. Replacing a failed
host route across multiple relays, explicit artifact inclusion, the native
recipient UI, independent security review and the wider browser/platform/load
matrix remain open. The host and relay keep the existing quotas and expiry
checks; there is no offline archive. See [gateway v0](gateway.md) for browser
failure semantics, packaging and operator instructions.

## Historical host-only verification

- `cargo test --locked -p sheprd-remote-control --lib sharing::tests`: 24 passed.
  These exercise the real store, private files, capability checks and projection
  reducer with explicit clock values, without mock authorization or network peers.
- `just format` and scoped `cargo fmt ... -- --check`: passed.
- Scoped Clippy for both changed crates, all targets and features: passed.
- `cargo check --locked -p sheprd-remote-control --lib --no-default-features`: passed.
- `just check-file-size`, `just check-sorted`, and `git diff --check`: passed.
  New untracked Rust files were also checked directly because repository scanners
  only enumerate tracked files.
- Retained HTML relative links and the source ZIP's integrity: checked.

At that stage the native application and browser adapters had not been tested. Subsequent native gateway tests exercise real relay
admission as recorded below; Phase 0 browser evidence remains separate.

## Relay gateway verification

The next implementation adds `sharing::worker`, `sharing::gateway`, and the
native `gateway::peer` reference client. See [the draft profile](gateway.md).

- `cargo test --locked -p sheprd-remote-control --lib sharing:: --no-fail-fast`:
  38 tests pass, including 14 new worker/transport cases. Real loopback QUIC relays
  exercise public and private admission, live filtered updates, wrong capability,
  wrong pinned host, bootstrap/resume proof replay, tunnel identity substitution,
  grant-specific bootstrap removal, malformed owner requests, oversize frames,
  idle expiry, source replacement, partial-read revocation, cancellation cleanup
  and route shutdown. Source transcripts are synthetic fixtures, not real agents.
- `cargo clippy --locked -p sheprd-remote-control -p sheprd-chat-protocol
  --all-targets --all-features -- -D warnings`: passed.
- `cargo check --locked -p sheprd-remote-control --lib --no-default-features`: passed.
- Workspace formatting, size/sorted checks, direct added-file checks and
  `git diff --check`: passed.

Those checks covered the native gateway.

## Invitation verification

The v0 single-relay invitation codec and gateway tests pass together with
`cargo test --locked -p sheprd-remote-control --features server --lib sharing::`
(40 tests). The native relay test reads through a decoded invitation and observes
revocation. Codec tests cover the exact landing path, private relay prefixes,
relay substitution, expiry equality, the URL size limit, malformed keys and
unknown fields and versions. The loopback test supplies an existing trusted
QUIC connection. It does not load a browser page or discover a production relay.
