# Relay resilience

Status: handshake and phone diagnostic fixes implemented in this branch,
not deployed. The remaining stages are proposed. No live relay configuration
has changed. See [relay architecture](relay-plan.md).

## Confirmed defects and immediate fixes

The phone called any non-200 info response “Not a relay,” including bans
(403) and proxy failures (502/503), and discarded the evidence. Logs show a
one-hour malformed-message ban near the screenshot's clock time, but do not
identify the caller. Attribution to this phone remains unconfirmed.

WebSocket challenge failures counted disconnects, timeouts, and failed sends
as malformed messages. Multi-relay racing cancels losing attempts, so even
successful failover could contribute to a ban. Ping/Pong before a hello was
also rejected, despite being valid control frames.

This branch stops scoring interrupted handshakes, tolerates control frames,
and puts the entire exchange under one ten-second deadline. Invalid JSON,
binary hellos, invalid credentials, and bad signatures remain penalized.
Admission caps and rate limits stay in place. Tests use real WebSockets for
both listener and dialer paths, including cancellation, timeout, and malformed
clients. QUIC already distinguishes closed streams from malformed requests.

Phone info checks retain 64 timestamped results across launches and support
offline sharing and clearing. They distinguish HTTP status, network errors,
and invalid relay responses. These are screen-triggered checks, not background
connection telemetry. A healthy info endpoint does not prove a tunnel works.

## Stage 1: connection evidence

Record bounded local history for actual phone and desktop connection attempts:
relay, transport, stage (DNS/TLS, upgrade, proof, admission, listener accept,
tunnel), outcome, duration, winner, cancellation, and network changes. Keep
last failure separately so successful probes cannot erase it. Export offline;
never retain keys, tokens, payloads, headers, or raw server error bodies.

Add server counters for handshake stage and outcome. Private operators should
have an authenticated diagnostic mode with a bounded event ring and short
retention. Correlate attempts using short-lived random IDs or rotating
pseudonyms. Define IP visibility explicitly; personal hosting alone does not
mean a relay has private admission enabled. Provide dashboard export with
redaction and retention controls. Do not give phones the dashboard token;
a diagnostic receipt may expose only that authenticated caller's attempt.

## Stage 2: authenticated recovery through automatic IP bans

A listen token admits a computer; a phone proves a dial key authorized by
that computer. A valid signature alone does not authorize an arbitrary key.

Add a separately bounded authentication lane for automatically banned
addresses. Verify token plus listener proof, or an explicitly authorized dial
key, before allowing work. Charge authenticated identities their own quotas,
while retaining global connection, memory, bandwidth, and handshake caps.
Keep unknown callers indistinguishable from offline routes. Manual operator
bans remain authoritative by default. Revoked tokens or keys must lose access.

Never whitelist an entire IP after one valid credential: cellular NATs share
addresses and IPv6 prefixes change. Limit concurrent verification and its rate;
legitimate saturation must not be treated as hostile protocol traffic.

Coordinate this policy across HTTP preflight, WebSocket upgrade, QUIC admission,
registration, dialing, accept, and session eviction. Current bans run before
authentication at multiple layers; changing just one check is insufficient.
Version new wire fields and preserve compatibility with old clients.

## Stage 3: usable multi-relay failover

Computers already listen on multiple endpoints and phones race with a
one-second stagger. Configure public and private endpoints on computers,
not only phones. Decide whether private-first preference should override
latency ranking, and show registration health on each relay plus the active
path. Do not label a successful info probe as end-to-end connectivity.

Reconnect through alternatives with bounded jittered backoff, but never
replay non-idempotent operations just because a tunnel failed. Test a dead
first relay, 403/429/502/503, valid info with broken WebSockets, missing
listeners, tunnel failure, and successful second relay. Assert prompt failover,
loser cancellation, permit release, and unchanged pinned identities.
Exercise Wi-Fi/cellular changes, app suspension, and hosts sharing a NAT.

## Verification and rollout

Run Rust default/all-feature CI gates and the full iOS app build before release.
For the authentication lane, test legitimate and hostile peers sharing a NAT,
revoked tokens/keys, manual bans, bounded concurrent authentication, saturation,
and QUIC/WebSocket parity. Test diagnostic authorization, redaction, bounds,
retention, and offline export.

Ship the handshake correction first, then connection telemetry, then the
authentication lane. Canary the relay fix on one private instance and verify
cancellations no longer add strikes while malformed clients still do. Exercise
public/private failover before wider rollout. Retain the previous verified
image digest for rollback. Disabling all abuse protection is not the fix.
