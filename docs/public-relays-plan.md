# Public relays

Sheprd runs two public relays that any user can add, so phones reach their
computers without Tailscale or a relay of their own. This is the launch
plan; the relay, its deployment, and the signed list are described in
[the relay plan](relay-plan.md).

## Decisions

| Question | Decision |
| --- | --- |
| Regions | DigitalOcean `fra1` (Frankfurt) and `sfo3` (San Francisco). Phones pick the relay with the shortest whole path, so Europe and the US west coast are covered and the east coast falls between. |
| Names | `fra1.relay.sheprd.org` and `sfo3.relay.sheprd.org`, A records on each droplet's own address with a short TTL. The names are in the signed list and on every phone that used them, so a replaced droplet keeps the name and only the record moves; a rebuilt droplet keeps its address. |
| Opt-in | No relay is used until the user adds it. Settings > Remote control > Relays offers Sheprd's relays from the signed list, each with **Add**; the daemon never contacts sheprd.org. Turning them on for everyone is a later decision, made after watching load. |
| Admission | Open: no listen token, since users have no way to get one. Phones never need one either way. |
| Trust | Settings saves each relay's signed QUIC pin with it, and the daemon refuses a relay whose `/v1/info` reports another. |

## Before deploying

Code (this branch):

- Signed pins reach the daemon (`--relay-pin`) and phones
  (`GET /api/relay`), and a mismatch refuses the relay.
- The relay plan no longer describes a `"default"` setting or a daemon
  fetching the list; both contradicted opt-in.

Owed by the maintainer, outside the repository:

1. Make the `ghcr.io/sheprd-labs/sheprd-relay` package public.
2. Create the `relays` environment on `sheprd-labs/sheprd`, deployments
   from `main` only, with a required reviewer, and store
   `RELAYS_SIGNING_KEY` (the Ed25519 key matching
   `crates/sheprd-relay/keys/actions.ed25519.hex`) in it.
3. Add the `fra1.relay` and `sfo3.relay` A records at sheprd.org's DNS
   host when `just relays-deploy` prints them (below).

## Deploying

```sh
just relays-deploy fra1 sfo3
```

1. It creates open droplets named `sheprd-public-relay-REGION` (apart from
   private `sheprd-relay-REGION` ones) with every SSH key on the account,
   prints the A records to add, and then waits as `just relays-check
   fra1 sfo3` does: for each name to resolve, `/v1/info` to answer from an
   open relay, and a real QUIC connection (`cargo run --example probe`).
   Caddy retries its certificate until the name resolves. `relays-check`
   can be run again at any time.
2. Compare each relay's `pin=` log line (`docker compose logs relay` in
   `/opt/sheprd-relay`) with the pin `/v1/info` reports.
3. Note the running image digest from `/opt/sheprd-relay/compose.env`, to
   roll back to.
4. Add a DigitalOcean uptime check on each relay's `/healthz`, alerting by
   email, and a droplet alert on outbound bandwidth and CPU.

## Load test

Before publishing, on `fra1`: hold listeners, open sessions, and push
throughput at the default limits from a client outside DigitalOcean, and
record listeners held, sessions, CPU, memory, and MB/s. Keep
`s-1vcpu-1gb` if it holds a few hundred idle listeners and the bandwidth
limits with headroom; otherwise resize before publishing. Record the
numbers here.

## Canary

1. Add both relays to the maintainer's own computer by address, with
   their pins.
2. On an iPhone over cellular with Wi-Fi off: open a chat and a terminal
   through each relay, then stop one relay's container and check the
   phone fails over to the other within seconds.
3. Check a desktop reached through `sheprd relay connect` the same way.
4. Watch each dashboard for a day: bans, refusals, and bandwidth.

## Publishing

```sh
just relays-add https://fra1.relay.sheprd.org fra1 Frankfurt
just relays-add https://sfo3.relay.sheprd.org sfo3 "San Francisco"
```

Compare each printed pin with the relay's log line, sign with the
YubiKey when asked, commit `website/site/relays.json` and
`relays.json.asc`, and merge: the site deploy adds `relays.json.sig`.
Settings offers the relays the next time it loads the list; no release is
needed. Then update `website/docs/relays.md`, which says no Sheprd relays
are available, and add what a public relay sees (addresses, timing, and
sizes, never content) and where to report abuse.

## Running them

- **Updates.** A new image is a new digest: run `start.sh` on one relay,
  check it, then the other. Never both at once: phones fail over only if
  one stays up.
- **The list expires** a year after signing: `just relays-sign` before
  then, and before the OpenPGP subkey expires on 2030-12-25.
- **Removing a relay** is `just relays-remove`. Users who added it keep
  it until they remove it, so keep it running for a while after.
- **Abuse.** Ban ranges by hand in `/opt/sheprd-relay/bans/list`.

## Later

- Stage 2 of the [resilience plan](relay-resilience-plan.md): an
  authenticated lane past automatic IP bans. A public relay is where one
  misbehaving client behind a carrier's shared address can ban its
  neighbours; watch `sheprd_relay_bans_total` and build it if bans
  appear.
- The iPhone uses the pin `GET /api/relay` hands it.
- A Prometheus scrape and alerts on refusals (phase 5 of the relay plan).
- Turning Sheprd's relays on by default.
