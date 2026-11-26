# Shared-session gateway, draft v0

This is an implemented native reference path through the existing QUIC relay and
hybrid tunnel, tested on loopback. It is not yet enabled by daemon startup or the
Share dialog. A host supplies observed source snapshots through a bounded watch;
no test connects to the user's running agent. The native peer can use credentials decoded from a draft HTTPS link. The
browser bundle, browser-to-relay proofs and native application UI remain pending.

## Ownership and lifecycle

`sharing::worker::Owner::open` starts one serial blocking worker owning the
persistent `Store`. Call it off the UI/async thread. Its command queue has 32
slots and rejects excess submissions; it does not create an unbounded queue of
waiting senders. Owner operations create shares, invite, revoke and stop them.

Each share receives a `watch::Receiver<Option<Snapshot>>`. It holds the latest
host-observed endpoint, cursor and `Arc<Transcript>`, not queued copies. `None`
or a closed watch means unavailable and ends admitted streams. The daemon's
chat adapter still needs to supply these snapshots atomically from its existing
attach worker. Guests cannot publish snapshots or select a source endpoint.

`Owner::access(share)` exposes only that share's internal admission, read and
live-grant queries. The gateway gets this handle, never `Owner` or the paired
phone router. The worker revalidates authorization on reads and checks idle
leases every 100 ms, subject to scheduling and filesystem latency. Revocation
signals leases before acknowledging the owner. A dropped/cancelled transport
releases its lease; failed, timed-out or cancelled reference-peer reads cannot
be retried on the partially consumed stream.

A `Route` has independent ephemeral P-256 listener keys, a random bootstrap seed,
and X25519/ML-KEM tunnel keys. It must not reuse a fleet or phone identity. The
host explicitly supplies a trusted relay connection and, for private relays, its
listen token. No invitation-controlled URL is fetched by the host. The token is
never put in a guest offer. Startup, key updates and admission have deadlines.
Stopping the route cancels its task set and tunnel pumps, including while a relay
control operation waits for an answer.

Route setup:

1. Create the host share and at least one application invitation through `Owner`.
2. Create an `Arc<Route>` and call `listen(connection, listen_token, access)`.
3. Run the returned `Serving` with a stop watch.
4. Await `route.offer(grant)` before constructing a user-facing invitation. It
   waits for that grant's bootstrap key to be acknowledged by the relay.
5. Stop the route when the owning share/service ends. The integration must enforce
   the plan's one route per active share and host/relay quotas; a `Route` alone is
   not a global listener orchestrator. Multi-relay reconnection/status is pending.

The gateway follows the worker's live grants. Each grant gets a distinct
bootstrap P-256 scalar derived by SHA-256 rejection sampling from the route's
random seed, the `sheprd-share-bootstrap/v0\0` domain, its 16 UUID bytes and a
big-endian u64 counter starting at zero. This uses the existing relay's scalar
construction pattern with a separate domain. It is a host implementation detail;
peers receive the resulting scalar and do not reproduce this derivation. Public
keys are compressed SEC1.
Revocation removes both its bootstrap key and its per-viewer keys. App-side
checks stop reads even when a relay delays key removal.

## Anonymous invitation links

The v0 writer uses `https://HOST[:PORT][/PREFIX]/share/#FRAGMENT`. The complete
URL fits in 8 KiB. The credential envelope stays in the fragment, which clients
separate before fetching the landing page ([RFC 3986 §3.5](https://www.rfc-editor.org/rfc/rfc3986#section-3.5)).
The fragment is unpadded base64url of a UTF-8 JSON object.

| Field | Encoding |
| --- | --- |
| `version` | Integer 0 |
| `relay` | Canonical HTTPS relay base including any configured port and prefix, without the trailing slash |
| `share`, `grant` | Non-nil UUID strings |
| `expires_ms` | Unix epoch milliseconds, exclusive |
| `route` | Relay route ID, unpadded base64url |
| `identity` | Unpadded base64url of the host tunnel identity (X25519 followed by ML-KEM-768) |
| `bootstrap`, `capability` | Unpadded base64url of each independent 32-byte credential |

`sharing::link::decode` takes a relay from the recipient's trusted configuration
and checks the landing URL and embedded relay against it before returning
credentials. Parsing performs no network access. An anonymous link is a bearer
credential. The host checks the grant and its stored expiry throughout access.
The outer fields carry no signature or independent claim of host ownership.
A browser deployment must still supply the viewer and clear the fragment from
history before showing the session.

## Admission frames

Every application message is UTF-8 JSON preceded by a big-endian u32 byte length,
inside the existing hybrid session tunnel. An HTTP request, ACP command or paired
phone request on this connection is invalid. Unknown fields, invalid lengths,
versions, signatures and grant checks close it without transcript or private
error details. No pairing slot is used and relay-wide origin/limit policy is
unchanged.

A link carries two different credentials: one grant's relay bootstrap scalar and
its 32-byte application capability. A relay credential alone cannot read a chat.
The invite also pins the dedicated share's full tunnel identity and route.

A client creates fresh tunnel keys and a new per-viewer P-256 relay key:

1. Dial the bootstrap key and complete the existing Noise/ML-KEM session handshake
   against the pinned host identity and route. No transcript is available yet.
2. Send `{"version":0,"share":UUID,"grant":UUID,"capability":[32 bytes],
   "key":[33 bytes],"proof":[64 bytes]}`. UUIDs are strings in serde's UUID format.
   The proof is raw P-256 ECDSA `r || s` over this exact concatenation:

   ```text
   "sheprd-shared-session/join/v0\0" || share UUID bytes || grant UUID bytes || Noise handshake hash
   ```

3. The host checks the bootstrap key's grant, proof and application capability.
   It binds admission to the tunnel's authenticated initiator key, installs the
   new viewer relay key and waits for the relay acknowledgement. It replies only
   `{"type":"registered","version":0}`. The client closes the bootstrap tunnel.
4. Dial with the viewer relay key. Use the same client tunnel identity and a fresh
   handshake. Send `{"version":0,"proof":[64 bytes]}`, signing:

   ```text
   "sheprd-shared-session/resume/v0\0" || share UUID bytes || grant UUID bytes || new Noise handshake hash
   ```

5. The host verifies both possession of that relay key and the enrolled tunnel
   identity. The grant comes from the host's registration, not a recipient claim.
   The admission can be claimed once and expires after five seconds if unused.
   A valid claim receives `{"type":"ready","version":0}`.
6. Only then send `sheprd_chat_protocol::sharing::Read`, with version 0 and an
   optional public cursor. The reply is a filtered snapshot, changes, or unchanged.
   No caller-supplied session path, artifact path or owner operation is accepted.

The relay's incoming-key header is a routing hint, not authorization. Replaying
an enrollment/resume proof on another handshake fails. A different client tunnel
identity cannot use an enrolled relay key. On a later reconnect, enroll a new
viewer key; a stale projection cursor obtains a fresh filtered snapshot. There is
no automatic command replay or reusable admission token beyond the original
explicit grant capability and newly proved keys.

## Resource bounds

- Four concurrent admissions per route, including header/Noise/resume work, with
  one five-second deadline. At most 16 admitted viewers and 20 session tasks.
- At most eight incoming dials per second per route. The relay retains its own
  per-key, per-route, per-IP and bandwidth limits. Excess attempts are closed;
  clients must not retry in an unbounded loop.
- At most 16 bootstrap keys plus 16 viewer keys, below the relay's 64-key limit.
- Read and admission frames: 1 KiB. Encoded responses: 256 KiB. Allocate bodies
  only after checking lengths. One response in flight per viewer, no broadcasts
  queued behind slow readers; tunnel pipes additionally have their existing
  fixed 64 KiB per-direction buffers.
- Four responses per second per viewer, and an 8 MiB logical-response budget per
  second for the worker as a whole. Exceeding a budget ends that reader.
- Thirty-second complete-read timeout and five-second write timeout. Revocation
  interrupts either, so a peer cannot retain access by withholding a frame tail.

These are enforcement bounds, not release performance measurements. Already
transmitted bytes cannot be recalled. External rendering, deployment headers,
WSS/browser interoperability and an independent security review remain release
gates. In particular, these native tests do not prove a safe browser origin or
safe Markdown/media rendering.
