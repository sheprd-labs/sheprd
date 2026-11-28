# sheprd-relay

A relay that joins two peers with no direct path to each other, such as a
phone and the desktop app it controls, and carries only what they encrypt
end to end. It never sees content: a **listener** proves a P-256 key and
waits on the route named by that key's hash; a **dialer** that the listener
named reaches it; the relay splices the two byte streams.

- Two transports, mixable in one session: **QUIC** (one connection, many
  sessions, proofs bound to the TLS session) and **WebSockets** over HTTPS
  for networks that block UDP.
- Who may dial is the listener's choice: dial keys it names, and one-use
  pairing keys for new devices.
- Bounded everywhere: connections, listeners, sessions, frames, bandwidth,
  rates, with bans for addresses that misbehave. No disk, no accounts, no
  addresses or routes in logs.

[`PROTOCOL.md`](PROTOCOL.md) specifies the wire format for anyone writing a
client, with test vectors in [`vectors/`](vectors).

> The protocol's identifiers (`sheprd-relay/listen/v1`, the ALPN
> `sheprd-relay/1`, and the rest) are its permanent names. They stay as
> they are whatever this project is called.

## Running a relay

```sh
cargo run --release -- --public-host relay.example.org
```

Every setting is a flag or a `RELAY_*` variable (`--help` lists them):
listen addresses, `--public-host` (which turns the WebSocket transport on),
limits, listen tokens for a relay kept to its owner's listeners, the
operator's dashboard token, and bans. Put it behind a TLS-terminating proxy
for the HTTPS side; QUIC listens on its own UDP port. A container image is
built from [`Dockerfile`](Dockerfile).

`/metrics` (its own port, `127.0.0.1:9090` by default) serves Prometheus
text. Every family is named `relay_*`; the same families under
`sheprd_relay_*` are deprecated aliases kept for existing dashboards. A ban
answers HTTP 403 with `relay-banned: 1` and, for older clients, the
deprecated `sheprd-relay-banned: 1`.

For tests and private relays, `--auto-ban off` turns automatic bans off and
`--listener-silence DURATION` drops silent listeners sooner.

## Using it from an app

### Rust, over WebSockets (`client-ws`)

```toml
sheprd-relay = { default-features = false, features = ["client-ws"], … }
```

```rust,ignore
use sheprd_relay::{Endpoint, client_ws::{self, Listener, ListenerConfig}};

// The desktop: listen, and name the phone's key.
let endpoint = Endpoint::parse("relay.example.org")?;
let mut listener = Listener::spawn(ListenerConfig::new(endpoint.clone(), desktop_key));
listener.set_dial_keys(vec![client_ws::public(&phone_key)]).await?;
while let Some(incoming) = listener.accept().await {
    let session = incoming?.session; // AsyncRead + AsyncWrite
    // run your end-to-end protocol over `session`
}

// The phone: dial the desktop's route.
let session = client_ws::dial(&endpoint, &route, &phone_key).await?;
let mut messages = session.framed(65_535); // optional u16-length messages
messages.send(b"hello from acme").await?;
```

The listener reconnects with capped backoff and names its keys again,
reports where it stands (`connecting`, `listening`, `unreachable`,
`replaced`), and stops for good when another listener takes its route.
Sessions ping the relay, which counts pings as activity, so quiet sessions
need no keepalive of their own.

### Rust, over QUIC (`client`)

`client::connect`, `client::Listener`, and `client::dial` speak the QUIC
transport; `client::reach` reads `/v1/info` and connects with the pinned
certificate.

### Swift

[`swift/`](swift) is the `RelayClient` package (Foundation and CryptoKit
only): proofs, pairing dial keys, addresses, statuses, framing, and a dialer.

### Other languages

Follow [`PROTOCOL.md`](PROTOCOL.md) and check your implementation against
[`vectors/`](vectors).

## Testing against a real relay (`testing`)

```toml
[dev-dependencies]
sheprd-relay = { default-features = false, features = ["testing", "client-ws"], … }
```

```rust,ignore
let relay = sheprd_relay::testing::TestRelay::start()?;
let endpoint = relay.endpoint(); // http://127.0.0.1:PORT, WebSockets only
// … listen and dial …
relay.stop(); // closes every socket, as a restart does
```

The test relay runs the real WebSocket relay on its own runtime, with small
limits and no bans, dashboard, QUIC, metrics port, or log subscriber.
`testing::Config` sets its limits, listen tokens, and address (start it
again on a stopped relay's address to test reconnects).

## Features

| Feature | What it builds |
| --- | --- |
| *(none)* | The protocol: endpoints, routes, proofs, messages, framing |
| `client` | QUIC listener and dialer |
| `client-ws` | WebSocket listener and dialer, TLS with the platform's verifier |
| `testing` | `TestRelay`, a WebSocket relay for tests |
| `websocket-server` | The relay over WebSockets |
| `quic-server` | The QUIC transport |
| `ops` | Rate limits, secrets, Prometheus text, and a dashboard kit |
| `dashboard` | The operator's read-only page |
| `server` (default) | Everything above as deployed, and the binary |

## Checks

`./ci.sh` runs formatting, lints for each feature set, the tests, and the
Swift package's tests. `RELAY_UPDATE_VECTORS=1 cargo test` rewrites
`vectors/` after a deliberate protocol change; otherwise any difference
fails.

## License

Apache-2.0; see [`LICENSE`](LICENSE).
