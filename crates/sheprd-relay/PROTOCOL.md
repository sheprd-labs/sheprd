# Relay protocol, version 1

This relay joins two peers that have no direct path to each other: a
**listener** (say, a desktop app) that waits on a route, and a **dialer**
(say, a phone) that reaches it. The relay is a blind pipe. It checks who may
listen and who may dial, then splices two byte streams and carries whatever
the peers send, which should be their own end-to-end encrypted protocol.

A relay offers two transports, and one session may mix them:

- **WebSocket** over HTTPS: works on any network, including ones that block
  UDP. Described in full here.
- **QUIC** over UDP: one connection carries many sessions, and proofs bind
  the TLS session itself.

Everything in this document is wire format, stable across relay versions.
Names in the format, such as `sheprd-relay/listen/v1`, are the protocol's
permanent identifiers, whatever the software or repository is called. The
test vectors in [`vectors/`](vectors) give exact bytes for every proof and
message; check an implementation against them.

Examples use a relay at `relay.example.org` and an app called acme.

## Conventions

| Thing | Format |
| --- | --- |
| Key | A P-256 (secp256r1) public key, SEC1 **compressed**: 33 bytes |
| Signature | ECDSA P-256 over SHA-256 of the message, `r ‖ s`: 64 bytes |
| Route | SHA-256 of a listener's key (its 33 bytes): 32 bytes |
| Session id | 16 random bytes the relay picks for one dial |
| Text encoding of bytes | base64url without padding (RFC 4648 §5): a route is 43 characters, a session id 22, a key 44, a signature 86, a nonce 43 |
| JSON | UTF-8 text frames, one object each; the relay ignores unknown fields |

ECDSA signatures may be deterministic (RFC 6979) or randomized; the relay
only verifies them.

A listener's **route** is the only thing that names it. Use a key for the
relay that is separate from your end-to-end keys: the relay learns nothing
from it but a stable pseudonym, so making one per relay is better still.

## Finding out what a relay offers

`GET /v1/info` over HTTPS answers:

```json
{"protocol":1,"version":"0.1","listen_token":false,"quic":{"port":8443,"pin":"…"},"websocket":true}
```

| Field | Meaning |
| --- | --- |
| `protocol` | The protocol version, `1` |
| `version` | The relay's major and minor version, for display only |
| `listen_token` | Listeners must present the operator's token (dialers never do) |
| `quic` | `null` or absent when QUIC is off; else the UDP `port` on the same host, and `pin`, the base64url SHA-256 of the relay's QUIC certificate (DER) |
| `websocket` | Whether the WebSocket transport answers (it needs the relay to know its public name) |

`GET /healthz` answers `ok`, for uptime checks.

**Bans.** A relay may ban an address that misbehaves (forged proofs,
malformed messages, probing). Every route but `/healthz` then answers HTTP
403 with the header `relay-banned: 1`. Relays before that name send only
`sheprd-relay-banned: 1`, which current relays still send beside it: treat
either as a ban. A 403 without either comes from something else, such as a
proxy. Capacity and rate limits answer 429 or 503 and never ban.

## The proof name

WebSocket proofs sign the relay's **name** as the client addressed it, so a
malicious relay cannot pass another relay's challenge through. From the
relay's URL:

1. Take the host, lowercase. An IPv6 address keeps its brackets.
2. Add `:port` only when the port is not the scheme's default (443 for
   `https`/`wss`, 80 for `http`/`ws`).
3. Add the path prefix, if any, without a trailing slash.

| Relay URL | Proof name |
| --- | --- |
| `relay.example.org` (https implied) | `relay.example.org` |
| `https://Relay.Example.org:443/` | `relay.example.org` |
| `wss://relay.example.org/acme/` | `relay.example.org/acme` |
| `https://relay.example.org:8443/acme` | `relay.example.org:8443/acme` |
| `http://127.0.0.1:8080` | `127.0.0.1:8080` |

`wss` is the same as `https`, and `ws` as `http`. Plain `http`/`ws` is
meant only for loopback, private addresses, and `.local` names. The relay
checks proofs against the name its operator configured, never against the
request's `Host` header. Socket URLs are the relay URL with `v1/…`
appended to its path, in `wss` (or `ws`) form.

A proof signs:

```text
context ‖ name ‖ 0x00 ‖ nonce ‖ extra
```

where `name` is the proof name's UTF-8 bytes, cut to its first 255 bytes,
`nonce` is the 32 bytes of the relay's challenge, and `extra` depends on the
proof:

| Proof | Context (ASCII, ending in a 0x00 byte) | Extra |
| --- | --- | --- |
| Listen | `sheprd-relay/listen/v1\0` | nothing |
| Dial | `sheprd-relay/dial/v1\0` | the 32-byte route dialed |

## WebSocket transport

Open each socket without an `Origin` header (or with the relay's own
origin) and without `Sec-Fetch-Site`: the relay refuses requests a browser
makes for a page on another site, with 403 before anything else.

Control messages are JSON in text frames. Session data is binary frames
only. Every socket starts the same way:

1. Relay: `{"t":"challenge","nonce":"<32 bytes>","version":1}`.
2. Peer, within 10 seconds: `{"key":"<key>","signature":"<signature>"}`,
   plus `"token":"…"` for a listener on a relay that asks for one.

### Listening: `GET /v1/listen`

1. Challenge, as above. The listener signs the listen proof.
2. Relay: `{"t":"ready","route":"<route>"}`. Check that it is your key's
   route. A refusal closes the socket instead (see close codes).
3. The listener names who may dial it, at any time, as often as needed:
   - `{"t":"dial_keys","keys":["<key>", …]}`: the keys allowed to dial,
     replacing the whole previous set; at most 64. Open sessions of a key
     left out end at once (4403). Until the first `dial_keys`, no one can
     dial.
   - `{"t":"pairing_key","key":"<key>","seconds":N}`: one key allowed **one**
     dial within N seconds (1 to 600), for pairing a new device. At most
     4 unused ones at a time. Its session lasts 10 minutes at most and
     survives a later `dial_keys` that leaves the key out.

   The relay answers each with `{"t":"status","status":"<status>"}`, in
   order. A message may carry an `"id"`, a non-negative integer or a string
   of at most 64 bytes, which the status echoes:
   `{"t":"dial_keys","id":7,"keys":[…]}` gets `{"t":"status","id":7,"status":"ok"}`.
   Relays older than ids ignore it and echo nothing, so match answers in
   order when the id is missing. An id over 64 bytes is a malformed message.
4. For each dial, the relay sends
   `{"t":"incoming","key":"<dialer's key>","session":"<session id>"}`.
   Open `GET /v1/accept/{session}` within the accept timeout: that socket
   is the session. No challenge on it; the session id, which only this
   listener learned, is the proof, and it works once.

The relay pings an idle listener at least every 25 seconds. A listener that
sends nothing for the listener silence limit (60 s by default), not even a
pong, is dropped with its keys; ordinary WebSocket stacks answer pings
automatically, which is enough. A second listener proving the same key
replaces the first, which is closed with 4409: do not reconnect in a loop
on 4409, or two listeners will take turns forever. Listener messages are
limited to 60 a minute, 16 at once (`limited` beyond that).

Keys the relay holds live only as long as the control socket: after a
reconnect, send `dial_keys` again, and any unused pairing keys with the
seconds they have left.

### Dialing: `GET /v1/dial/{route}`

1. Challenge. The dialer signs the dial proof, over the route in the URL.
2. Relay: `{"t":"status","status":"<status>"}`. It may wait up to the
   accept timeout for the listener before answering. A key the listener did
   not name gets `offline`, exactly as if no one listened, so a stranger
   learns nothing about the route.
3. After `ok`, binary frames are the session. Otherwise the relay closes
   the socket (4404 for `offline`, 4000 for the rest).

### Sessions

- Binary frames only, each at most the relay's `max-frame` (64 KiB by
  default, never under 1 KiB): write in frames of 16 KiB or less to be safe.
  A text frame or an oversized frame ends the session (4400).
- The relay forwards bytes, not frames: it may split or join frames. Do not
  rely on frame boundaries; use the message framing below if you need
  messages.
- A WebSocket ping or pong from either peer counts as activity: the
  session does not idle out while either side pings, so no keepalive is
  needed inside the session. The relay answers pings itself, never forwards
  them, and does not count them against bandwidth. Relays before this rule
  count only data; against those, keep a keepalive of your own inside the
  stream if the session may be quiet longer than the idle timeout.
- There is no half-close: when either side closes, the session ends.

### Statuses

The same values on both transports. On QUIC, the status is one byte.

| Byte | JSON | Meaning |
| --- | --- | --- |
| 0 | `ok` | Done; for a dial, the session starts |
| 1 | `offline` | No listener on the route, or not for this key |
| 2 | `busy` | The route or relay holds as many sessions as it may |
| 3 | `timeout` | The listener did not accept in time |
| 4 | `limited` | Too many requests from this address; retry later |
| 5 | `refused` | The proof did not verify |
| 6 | `forbidden` | The listen token is missing or wrong |
| 7 | `full` | The relay, or this address, holds as many listeners as it may |
| 8 | `malformed` | The request could not be read |
| 9 | `too_many` | More unused pairing keys than allowed |

New values may be added; treat an unknown one as a refusal.

### Close codes

| Code | Meaning |
| --- | --- |
| 4000 | The other side of the session closed or went away; a dial that failed with a status other than `offline` |
| 4400 | Protocol violation: a text frame in a session, an oversized frame, a malformed control message, or no proof in time |
| 4401 | The listen was refused: a bad proof, a wrong or missing token, or too many listens from this address |
| 4403 | The listener stopped allowing the dial key this session used |
| 4404 | Offline: no listener on the route for this key |
| 4408 | Nothing crossed the session (or the listener's socket) for too long |
| 4409 | Another listener proved the same key and took the route |
| 4410 | The session reached its longest allowed duration |
| 4429 | One side's address was banned |
| 4503 | The relay holds as many listeners as it may |

## QUIC transport

QUIC v1 with TLS 1.3 and ALPN `sheprd-relay/1`, on the UDP port
`/v1/info` gives. The relay's certificate is self-signed for the name
`sheprd-relay`; trust it by its **pin** from `/v1/info` (the SHA-256 of the
certificate's DER), not by a certificate authority. A relay may make a new
certificate at each start, so a client whose handshake fails reads
`/v1/info` again. The pin protects the listen token and metadata; content
is protected by the peers' own end-to-end protocol.

Only bidirectional streams are used, at most 64 open per connection. The
relay's idle timeout is 60 seconds; send QUIC keepalives every 15 seconds
or so. Every stream a peer opens starts with one request, and the relay
answers with one status byte.

**Proofs** sign 32 bytes exported from the connection's own TLS session
(RFC 5705 exporter, label `EXPORTER-sheprd-relay-listen`, empty context),
so they cannot be replayed on another connection:

| Proof | Signed bytes |
| --- | --- |
| Listen | `sheprd-relay/listen/quic/v1\0` ‖ exported |
| Dial | `sheprd-relay/dial/quic/v1\0` ‖ exported ‖ route (32) |

**Requests:**

| Request | Bytes |
| --- | --- |
| Listen | `0x01`, key (33), signature (64), token length (1), token (0–255) |
| Dial | `0x02`, route (32), key (33), signature (64) |

**Listening.** After `ok`, the listen stream stays open for as long as the
listener listens; closing it stops listening. On it the listener sends, and
the relay answers each with a status byte:

| Message | Bytes |
| --- | --- |
| Dial keys | `0x20`, count (1, at most 64), keys (33 each) |
| Pairing key | `0x21`, key (33), lifetime in seconds (2, big-endian, 1–600) |

These mean what `dial_keys` and `pairing_key` mean over WebSocket. For each
dial, the relay opens a stream on the listener's connection that starts
with `0x10` and the dialer's key (33 bytes); the rest of it is the session.
A newer listener for the same key resets the old listen stream with code 5.

**Dialing.** After `ok`, the dial stream is the session: raw bytes both
ways. One connection can carry many dials.

**Stream and connection error codes:**

| Code | Meaning |
| --- | --- |
| 0 | Done |
| 1 | The other side of the session went away |
| 2 | Nothing crossed the session for the idle timeout |
| 3 | The session reached its longest allowed duration |
| 4 | Protocol violation |
| 5 | Another listener took the route |
| 6 | The relay is shutting down |
| 7 | The listener stopped allowing this session's dial key |
| 8 | This address is banned |

QUIC has no ping that a stream carries (its PING frames keep the
connection, not a session), so a quiet QUIC session idles out after the
idle timeout unless a peer sends bytes in it.

## Pairing dial keys

A listener and a new device that share a 32-byte pairing secret (sent, for
example, in a QR code) both derive the device's one-time dial key from it:

```text
for counter in 0, 1, 2, …:
    scalar = SHA-256("sheprd-relay/pairing-dial/v1\0" ‖ secret ‖ counter as one byte)
    if 0 < scalar < n (the P-256 group order): the private key is scalar; stop
```

The first counter almost always works. The listener names the public half
with `pairing_key`; the device dials with the private half. See
`vectors/pairing.json`.

## Message framing (optional)

A session carries bytes. Peers that want messages can use this framing,
which the relay never reads:

```text
length (2 bytes, big-endian) ‖ message (length bytes)
```

A message holds 0 to 65,535 bytes. WebSocket frames and QUIC chunks may
split or join messages anywhere, so a reader buffers until a message is
whole. A reader should refuse a length over its own cap as soon as the
prefix arrives, rather than buffer it. See `vectors/framing.json`.

## Limits a client must respect

Defaults; an operator may change them. A test relay may use shorter ones.

| Limit | Default | What happens past it |
| --- | --- | --- |
| Dial keys per listener | 64 | `malformed` |
| Pairing key lifetime | 1–600 s | `malformed` |
| Unused pairing keys per listener | 4 | `too_many` |
| Pairing session duration | 10 min | closed (4410 / code 3) |
| Listener messages | 60 a minute, 16 at once | `limited` |
| Answering the challenge | 10 s | closed (4400) |
| WebSocket frame (`max-frame`) | 64 KiB (1 KiB minimum) | closed (4400) |
| WebSocket control message | 8 KiB | closed |
| `id` text | 64 bytes | `malformed`, closed |
| Session idle timeout | 5 min | closed (4408 / code 2) |
| Session duration | 24 h | closed (4410 / code 3) |
| Listener silence | 60 s | listener dropped (4408) |
| Relay pings a listener | every 25 s, or a third of the silence limit if shorter | — |
| Accept timeout | 10 s | the dial hears `timeout` |
| Sessions per dial key | 4 | `busy` |
| Sessions per route | 32 | `busy` |
| Bandwidth per session | 4 MiB/s each way | throttled, not closed |
| QUIC idle timeout | 60 s | connection closed |
| QUIC streams per connection | 64 open | the next stream waits until one closes |

Per-address limits on connections, dials, and listens also apply; a client
that hits one gets `limited`, 429, or 503, and should back off and retry.

## Compatibility

- Version 1 is additive: new fields, new optional messages, new statuses.
  A client must ignore unknown JSON fields and treat unknown statuses as
  refusals. A relay ignores unknown fields it is sent.
- Optional additions so far, each safe against older relays:
  - `id` on listener messages (older relays ignore it and do not echo);
  - WebSocket pings as session activity (older relays need in-band data);
  - the `relay-banned` header (older relays send only
    `sheprd-relay-banned`).
- A renamed context label would be a new protocol version, accepted beside
  the old one, never a replacement.

## What the relay sees

It sees: the listener's public key and route; the dial keys and pairing keys
a listener names; each dialer's public key and the route it dials; both
peers' network addresses; when sessions start and end; and the size and
timing of every chunk it carries.

It does not see: session content, if the peers encrypt end to end (which
they must: the relay, and anyone who takes it over, can read and change
whatever is not); the peers' end-to-end identities; or anything a peer does
not send it. It stores nothing on disk and keeps nothing across a restart.

Route and session ids travel in URL paths (`/v1/dial/{route}`,
`/v1/accept/{session}`), so a proxy in front of a relay may log them.

## Test vectors

`vectors/` holds JSON files made by the relay's own tests from fixed keys
and nonces (`RELAY_UPDATE_VECTORS=1 cargo test` rewrites them; any
difference otherwise fails):

| File | Contents |
| --- | --- |
| `routes.json` | Private scalars, public keys, and their routes |
| `websocket.json` | Proof names and URLs for several endpoint forms; listen and dial proof bytes; signatures to verify; hello texts |
| `quic.json` | The exporter stand-in, proof bytes, signatures, and the encoded requests and listener messages |
| `pairing.json` | A pairing secret, the derived scalar, its public key and route |
| `framing.json` | Messages and their framed bytes |
| `messages.json` | The exact JSON of each control message, with and without `id`; every status and close code |

Signatures in the vectors are deterministic here, but an implementation
need only check that they verify, and that its own verify too.
