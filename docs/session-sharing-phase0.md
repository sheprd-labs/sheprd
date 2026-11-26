# Shared sessions: Phase 0 findings

Status: feasibility investigation and technical proposal ready for review.
The user approved the direction and authorized Phase 0 only. No production
sharing feature, identity service, account, or public endpoint was created.
The user subsequently authorized phased implementation; see the current plan.
This historical experiment is not a production security sign-off.

Source inspected: `4ab49ed3ee744dc48bae6a18a565d220c4526b48`.
See [the approved direction](session-sharing-plan.md) and
[the browser probe evidence](session-sharing-phase0-evidence.json).

## Verdict

The existing tunnel can run in a browser and exchange encrypted records with
native Rust, with small portability changes. The existing relay already has
useful origin checks and revocable reusable dial keys. A new fleet enrollment
mechanism or a second agent runtime is unnecessary.

Recommendation: proceed to the narrowly scoped host-policy work in Phase 1
once these findings are reviewed. Retain the browser/relay integration and
identity gates below. Do not interpret a successful crypto probe as evidence
that guest authorization, team recovery, or public hosting is implemented.

## What was actually tested

| Experiment | Observed result | Limit of the evidence |
| --- | --- | --- |
| Unmodified tunnel, browser WASM target | Build fails on a native RNG dependency | The current crate cannot be shipped to browsers unchanged |
| Browser RNG features enabled in a disposable copy | A subsequent link fails in `ring` | Snow's `std` feature pulls in an unused native crypto dependency |
| Remove Snow's `std` feature; use elapsed `Duration` for record timing | WASM build succeeds with the existing ChaCha/X25519/SHA/ML-KEM algorithms | This is an experimental copy, not a production change |
| Fresh random hybrid handshake, browser to native Rust | Pass | Loopback WebSocket fixture, not a deployed relay |
| UTF-8 round trip | Pass | Synthetic text, no real session data |
| Full-size 16,384-byte records and timed rekey in both directions | Pass | Synthetic elapsed times cross the ten-minute threshold; no ten-minute wall-clock endurance test |
| Oversize record rejection | Pass, within the full-size/rekey test | Does not yet test whole application-frame or artifact limits |
| Replay and tampering | Rejected | Tested on the browser receive path using native encrypted responses |
| Wrong route and wrong host identity | Handshake rejected | The fixture supplies host public keys; invitation authenticity is not tested |
| Production relay origin suite | 4 tests pass | Existing HTTP/WebSocket policy, not a new guest viewer |
| Production relay revocation suite | 7 tests pass | Existing transport key/token revocation, not team membership revocation |

The browser reported seven passing cases in macOS WebKit through Sheprd. The
native peer used the same tunnel code with only the timing API changed. The
browser copy additionally disabled Snow's unnecessary `std` feature and enabled
browser entropy. No production cryptography was replaced with JavaScript crypto.

The fixture accepts a synthetic guest without authorization to isolate crypto
interoperability. It is not an implementation or test of session authorization.
It binds only to loopback, has no access to real agent sessions, and was stopped
after the run. HTTP/WS on loopback provides a browser secure context but does not
validate production HTTPS/WSS, certificate handling, or deployment headers.

## Browser implementation profile

Use the existing Rust tunnel compiled to `wasm32-unknown-unknown`, behind a small
`wasm-bindgen` interface. The experiment used version 0.2.128, already resolved
transitively in the repository lockfile; it added no workspace dependency.
Confirm the explicit production dependency and its packaging before adding it.
Run encryption in a Web Worker so large transcripts do not block rendering.

Required production changes, not made in Phase 0:

1. Enable `getrandom`'s browser backend for the tunnel's 0.4 dependency and
   Snow's 0.3 dependency, under target-specific configuration.
2. Avoid Snow's `std` feature on the browser target. Inspection and linking
   showed it brings in `ring` even when using the default ChaCha resolver.
   Once excluded, the incidental third RNG dependency (0.2 via `ring`) is not
   needed. Preserve native feature behavior unless separately verified.
3. Replace the core record API's `Instant` argument with explicit elapsed time
   or an equivalent platform-independent clock value. Native adapters obtain
   monotonic elapsed time; browser adapters use `performance.now()`. Clock
   regression, sleep/resume, rekey count, and overflow need production tests.
4. Keep fixed test randomness inaccessible through the production JS interface.
   Dispose failed transports; an invalid record must not be retried against the
   same state. Bound WASM inputs before allocation and avoid exposing secret
   export methods to JavaScript unnecessarily.
5. Test release builds, worker execution, bundle size, and Chrome/Firefox/mobile
   Safari as well as macOS WebKit. The experiment did not measure those.

This avoids implementing Noise or ML-KEM a second time. Native clients use the
same core and vectors. Do not use a downgraded browser-only cipher suite.

## Relay refinement: reuse the transport, separate the authority

The first plan proposed new share transport endpoints. Inspection suggests a
smaller first implementation: dedicated share routes over the current transport,
plus a same-origin static viewer. This refines D6 without relaxing its origin
policy. Validate this profile end to end before freezing the protocol.

Each live share has its own relay listener identity, route, and tunnel identity.
They are separate from the host's paired-device route and credentials. Incoming
streams on these routes reach only the sharing gateway. Existing `/v1/listen`,
`/v1/dial`, and `/v1/accept` carry opaque bytes and can remain transport-only.
The same-origin viewer uses the relay's configured public host/prefix exactly
when signing proofs. No wildcard cross-origin exception is needed.

Use ordinary reusable dial keys, never `PairingKey`. Registering a share route
must not modify the device-pairing slots. The current relay supports 64 dial
keys per listener, 32 sessions per route, and only 4 concurrent sessions per
key by default. Therefore a single link key cannot directly support the planned
16 active viewers. Do not raise that global limit to hide the mismatch.

Proposed admission flow:

1. The invitation carries a bootstrap relay proof credential and a separate
   high-entropy session capability. Both are scoped to a grant on a share route.
2. Explicit Join opens a short bootstrap connection, completes the existing
   hybrid session handshake against the pinned share identity, and submits
   application authentication inside the encrypted stream. No transcript is
   sent yet. A successful Noise handshake alone never authorizes the guest.
3. The guest supplies a new per-client P-256 relay public key and proves its
   possession, bound to the encrypted channel. After grant checks, the host
   registers that key with the relay and returns a short-lived admission bound
   to the guest tunnel identity. Close the bootstrap connection.
4. The guest reconnects with its own relay key. Only after the same host-side
   grant checks does it receive the shared transcript. At most four bootstrap
   joins per link are concurrent; additional joins retry with bounded backoff.
5. The host maps each admitted client to its grant. Revocation invalidates the
   application admission and removes derived relay keys, including live streams.
   Expiry is enforced by the host even if relay key updates are delayed.

The extra connection avoids a shared key consuming the per-key live-session
budget. Cap grants and admitted keys so the 64-key list remains bounded. Relay
listener quotas are shared with other sessions and people behind the same NAT:
8 active shares is a host ceiling, not a promise that every relay will admit 8.
Report capacity failures without silently switching to broader host routes.

Browser P-256 signing should use WebCrypto. Its raw ECDSA signature format fits
the relay's `r || s` verification; compressed SEC1 public-key encoding and exact
proof bytes still need a browser-to-production-relay integration test. This was
reviewed in source and documentation, not exercised by the crypto fixture.

## Invitation and application session contract

Draft profile, not frozen wire encoding:

- HTTPS landing URL with a versioned fragment envelope, at most 8 KiB. Envelope
  includes share/grant IDs, relay origin and path prefix, route, full share tunnel
  public identity, expiry, bootstrap credential, and a 256-bit capability.
- No session name, identity proof, capability, or transcript in HTTP paths,
  query strings, preview metadata, telemetry, or analytics.
- The fragment pins the tunnel identity. A link author can still send a malicious
  host's link; do not show an unverified display name as an authenticated person.
- The first encrypted message identifies the share and grant, binds the agreed
  protocol and capabilities to the handshake, and proves possession of the
  application capability or an issuer/device-bound membership admission.
- Use constant-time capability verification and per-share independent secrets.
  The host persists a verifier rather than a recoverable invitation capability.
  Existing invitations need not remain copyable forever: mint a replacement if
  the capability was deliberately discarded. Do not expose a secret via list APIs.
- A named/team invitation link is only a locator/bootstrap invitation, never a
  substitute for the named person's proof. Leaking it cannot grant team access.
- Session admission is distinct from participant presence. Multiple devices may
  belong to the same person; anonymous viewers have a guest identifier rather
  than a verified human name. Attribute commands from host-authenticated context.
- Application state machine: connected, encrypted-but-unauthorized, admitted,
  streaming, closing. Only the admitted states can read content or propose actions.
  Bind every operation to one share and one source-session identity.

Use the present Noise session pattern with a distinct per-share identity and
route. Do not reuse the pairing PSK mode or its acceptance semantics. Any new
application authentication transcript/proof format needs test vectors and a
security review before production; Phase 0 does not invent a new cryptographic
primitive or claim to have reviewed the complete handshake composition.

## Identity, team membership, and recovery

D2 remains an optional authority, independent of relays. Initial authority
contract: stable issuer/person/team IDs, explicit enrolled devices, passkey sign-in,
short-lived signed assertions, key rotation, and a recovery epoch. Use standard
signed-token formats with a fixed allowed algorithm and trusted issuer keys;
never fetch arbitrary token-provided key URLs or treat display names as IDs.
Select the maintained implementation/library in a separate dependency review.

Assertions bind issuer, subject, audience (the share host), share ID, device key,
team ID when applicable, membership version, recovery epoch, issued/expiry times,
and the host's fresh challenge. Device possession must be verified. Reject reused
challenges, wrong audiences, future issue times outside the agreed skew, unknown
keys, and unsupported authorization semantics. The host owns grant permissions;
a team assertion proves membership, not a right to operate an agent.

Proposed recovery rules:

- Encourage two enrolled authenticators; support a one-use high-entropy offline
  recovery code. Store only its verifier at the authority and rotate it on use.
- A second authenticator or recovery code can enroll a replacement device.
  Recovery advances the person's recovery epoch and revokes prior sessions/device
  bindings. Hosts receive the change; stale proofs still expire within the bound.
- Email ownership alone and a team admin cannot silently recover another person's
  identity. An admin can remove and invite a new identity, but old direct grants
  do not transfer. If all recovery methods are lost, create a new identity and
  obtain new invitations rather than impersonating the old account.
- Require recent authentication for membership changes and recovery actions.
  Record and surface recovery events. Do not remove the last team owner without
  an explicit ownership transfer or team deletion.
- Issuer signing-key rollover has a bounded overlap. Unknown/revoked issuers fail
  closed. Issuer compromise is a trust failure: it can forge membership. This is
  not fixed by transcript encryption and must be included in operational response.

Membership is current membership, as approved. Bound proof lifetime to 300 seconds.
Use an issuer-relative time estimate plus local monotonic expiry; subtract the
allowed clock uncertainty rather than adding grace beyond 300 seconds. If clock
uncertainty cannot be bounded, refresh online or refuse access. Never extend an
existing lease using a stale cached assertion. Check before every dispatch and
close streams when their membership lease expires. A removal event accelerates
revocation; the five-minute bound covers missed events and authority outages.
Direct, other-team, and anonymous-link access remain independent grants.

This defines behavior, not a chosen hosted vendor or deployed identity service.
Passkey UX, origin transitions to native clients, authority key rotation and
recovery require real implementation tests before Phase 3 ships.

## Proposed beta limits

These are explicit starting limits, not measured capacity claims. Clients may
negotiate lower values; never accept a peer's request to raise a hard limit.

| Resource | Initial limit |
| --- | --- |
| Live shares per host | 8, subject to relay quota |
| Active viewers per share | 16 |
| Active grants per share | 16; bootstrap plus admitted keys remain below 64 |
| Concurrent bootstrap sessions per invitation | 4, with a 5-second application-auth deadline |
| Pending owner approvals per host | 32, expire after 2 minutes |
| Decoded invitation | 8 KiB |
| Encrypted record plaintext | Existing 16 KiB |
| Logical JSON message | 256 KiB, explicitly framed across bounded records |
| Snapshot | Chunked, at most 8 MiB projected data, with a truncation marker |
| Buffered outgoing data per guest | 256 KiB; disconnect slow clients and resume |
| Replay window per share | Smaller of 2 MiB or 1,024 events; otherwise reset |
| Aggregate sharing buffers per host | 64 MiB; avoid cloning snapshots per guest |
| Artifact | 8 MiB each, 32 MiB of explicitly shared artifacts per share |
| Comment or proposed prompt | 16 KiB, at most 32 pending proposals per share |
| Application send rate | 1 MiB/s per guest and 8 MiB/s per host, including artifacts |
| Membership lease | At most 300 seconds; no outage extension |
| Default anonymous invitation | 1 hour |
| Audit retention | Earlier of 30 days or 16 MiB per host; bounded rotation |

Aggregate budget wins over individual limits. The projection must bound decoded
images, parser depth, queued metadata, and in-flight snapshot construction too;
encoded-byte caps alone are insufficient. Time out and cancel partial frames.
Persist revocation separately from the prunable audit. No secret or prompt body
is duplicated into audit entries.

## Threats and required controls

| Threat | Design response |
| --- | --- |
| Leaked link | View-only default, expiry, explicit revocation; named/team proof for restricted access |
| Guest escalates into owner/fleet APIs | Separate per-share routes and application dispatcher; capability checks on every operation |
| Hidden history leaks on reconnect | Host-side projection with separate IDs/cursors; resets preserve the history boundary |
| Removed member retains a connection | Removal events, short monotonic leases, checks before dispatch, stream closure |
| Relay reads or changes content | Existing encrypted records; pinned share host; never terminate encryption at a gateway |
| Malicious browser viewer | Acknowledge code-origin trust; no third-party scripts; self-hosted/native options; CSP is not protection against a malicious origin |
| Malicious issuer | Explicit trust scope; host grants remain local, but issuer compromise can falsify membership |
| Prompt injection through comments | Keep human discussion outside agent input; exact-text owner approval for suggestions |
| Resource exhaustion | Caps above, pre-auth deadlines, per-address/route/key transport limits, bounded decoding and queues |
| Revocation races with queued actions | Reauthorize immediately before dispatch; remove pending proposals; do not claim to undo started work |
| Invite causes arbitrary network access | No host-side URL fetching; trust-check relay destinations, exact endpoint validation, no metadata-service redirects |

## Gates that remain

Before enabling real sharing, connect the browser client through the actual relay,
including P-256 proof serialization, bootstrap-to-client-key handoff, prefixes,
TLS, real expiry/revocation, and host process restart. Test resource use with 16
slow/fast viewers, not only one synthetic peer. Repeat the browser tests in release
mode on the supported browser/platform matrix and inside a worker.

Before teams, select and review the authority implementation and recovery UX,
prove short-lease behavior under clock skew and outages, and exercise multi-device
identity and issuer rotation. No dependency/service is selected or deployed by
this document. Before direct control, obtain a new explicit scope decision.

The initial Phase 0 gate is satisfied only for browser/native crypto feasibility
and existing transport-policy behavior. The complete admission profile is still
a proposal. The next approved work should make the host boundary testable without
publishing sessions, then prove the complete browser/relay profile before Phase 2.

## Reproduction and artifacts

Exact production checks run:

```sh
cargo check --locked -p sheprd-tunnel --target wasm32-unknown-unknown
# Expected failure: browser RNG backend not enabled.
cargo test --locked -p sheprd-relay --features server cross_site -- --nocapture
cargo test --locked -p sheprd-relay --features server revok -- --nocapture
```

The first test build took about 11 seconds; the second was cached. No full
workspace gate was run because production Rust was not edited. The isolated
Rust wrapper was formatted and linted separately. The native and WASM builds
used the repository toolchain, 1.99.0. `wasm32-unknown-unknown` was installed for
this experiment. The wasm-bindgen CLI was downloaded from its official release
and checked against the accompanying SHA-256 checksum, in a temporary directory.

A source/evidence ZIP in Downloads contains the isolated manifests and lockfile,
experimental tunnel copy, wrapper, native host, loopback server, browser probe,
and result. It excludes binaries, downloaded tools, and build products. Its README
identifies the source revision and commands; the fixture must never be deployed.

References:

- [Existing tunnel](../crates/sheprd-tunnel/src/lib.rs),
  [record clock](../crates/sheprd-tunnel/src/record.rs),
  [relay limits](../crates/sheprd-relay/src/limits.rs), and
  [dial-key revocation](../crates/sheprd-relay/src/hub.rs).
- [Browser entropy backend](https://docs.rs/getrandom/latest/getrandom/).
- [wasm-bindgen browser packaging](https://rustwasm.github.io/docs/wasm-bindgen/examples/without-a-bundler.html).
- [Rust browser WASM target limitations](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html).
- [WebCrypto signature formats](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/sign).
- [WebAuthn's origin-scoped credentials](https://www.w3.org/TR/webauthn-3/).
