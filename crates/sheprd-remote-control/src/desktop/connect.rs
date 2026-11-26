//! `relay-connect`: the `ProxyCommand` `ssh` runs for a machine added over
//! Bluetooth (`sheprd_client::relay_host`). It dials the machine's route
//! on the first of its relays that lets this computer's key through, runs
//! the tunnel against the identity pinned when pairing, and carries bytes
//! between its stdio and the tunnel; SSH's own encryption and host key
//! check run inside, unchanged.

use crate::desktop::{Error, Keys, Result, record::Record};
use sheprd_relay::{Endpoint, RouteId, client};
use std::{path::Path, time::Duration};
use tokio::io::join;

/// How long reaching one relay and dialing through it may take before the
/// next is tried.
const ATTEMPT: Duration = Duration::from_secs(10);
/// Initial wait before retrying a transient relay refusal (`Busy` or
/// `Timeout`). Doubles up to [`DIAL_RETRY_MAX`] so a busy relay never spends
/// the relay's per-address dial allowance (default 120/min, burst 30) within
/// [`ATTEMPT`]: at most 8 dials over 10 seconds.
const DIAL_RETRY_INITIAL: Duration = Duration::from_millis(250);
const DIAL_RETRY_MAX: Duration = Duration::from_secs(2);

/// How long the machine's last bytes may take once `ssh` closed its side.
/// `ssh` closes it only as it exits, and the machine may never close its
/// own, so waiting for both would keep this process, and the relay session
/// it holds, alive for good: a dial key holds only a few at once.
const LINGER: Duration = Duration::from_secs(3);

/// Carries `ssh`'s connection to `host`, `r-ID.sheprd.invalid`, until
/// either side closes.
pub(crate) async fn connect(state_dir: &Path, host: &str) -> Result<()> {
    let (connection, tunnel) = open(state_dir, host).await?;
    let spliced = splice(tokio::io::stdin(), tokio::io::stdout(), tunnel.io, LINGER).await;
    relay_result(connection.close_reason(), spliced.map_err(Error::Splice))
}

/// The tunnel pump exposes a closed relay stream as plain EOF. Recover a
/// confirmed ban while the underlying QUIC connection is still available.
fn relay_result<T>(closed: Option<quinn::ConnectionError>, result: Result<T>) -> Result<T> {
    match closed.map(client::Error::from) {
        Some(client::Error::Banned(ban)) => Err(Error::RelayBanned(ban)),
        _ => result,
    }
}

/// Copies `input` to `tunnel` and `tunnel` to `output` until the tunnel
/// closes, or until `linger` after `input` ends.
async fn splice<I, O, T>(
    mut input: I,
    mut output: O,
    tunnel: T,
    linger: Duration,
) -> std::io::Result<()>
where
    I: tokio::io::AsyncRead + Unpin,
    O: tokio::io::AsyncWrite + Unpin,
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite,
{
    use tokio::io::AsyncWriteExt as _;
    let (mut from_tunnel, mut to_tunnel) = tokio::io::split(tunnel);
    let upload = async {
        let copied = tokio::io::copy(&mut input, &mut to_tunnel).await;
        // Tells the machine nothing more comes, as a half-close does.
        let _ = to_tunnel.shutdown().await;
        copied.map(drop)
    };
    let download = async {
        tokio::io::copy(&mut from_tunnel, &mut output).await?;
        output.flush().await
    };
    tokio::pin!(upload, download);
    tokio::select! {
        downloaded = &mut download => downloaded,
        uploaded = &mut upload => {
            uploaded?;
            match tokio::time::timeout(linger, &mut download).await {
                Ok(downloaded) => downloaded,
                // `ssh` is gone: nothing reads what else arrives.
                Err(_) => Ok(()),
            }
        }
    }
}

/// The tunnel to `host`, through the first relay that lets this computer
/// through, and the relay connection carrying it. A machine the fleet
/// shares is told first that the session carries SSH.
pub(crate) async fn open(
    state_dir: &Path,
    host: &str,
) -> Result<(quinn::Connection, sheprd_tunnel::stream::Tunnel)> {
    let id = sheprd_client::relay_host::host_id(host)
        .ok_or_else(|| Error::Record(host.chars().take(80).collect()))?
        .to_owned();
    let state = state_dir.to_owned();
    let (record, keys) = tokio::task::spawn_blocking(move || {
        Ok::<_, Error>((Record::load(&state, &id)?, Keys::load_or_create(&state)?))
    })
    .await
    .map_err(|_| Error::Cancelled)??;
    let (connection, mut tunnel) = open_record(&record, &keys).await?;
    if record.opening {
        use tokio::io::AsyncWriteExt as _;
        let (state, hash) = (state_dir.to_owned(), tunnel.hash);
        let opening = tokio::task::spawn_blocking(move || {
            crate::fleet::opening(&state, crate::control::fleet::removal::PURPOSE_SSH, &hash)
        })
        .await
        .map_err(|_| Error::Cancelled)??;
        let written = tunnel.io.write_all(&opening).await;
        relay_result(connection.close_reason(), written.map_err(Error::Splice))?;
    }
    Ok((connection, tunnel))
}

/// The tunnel to the machine `record` describes, dialed with `keys`.
pub(crate) async fn open_record(
    record: &Record,
    keys: &Keys,
) -> Result<(quinn::Connection, sheprd_tunnel::stream::Tunnel)> {
    let identity = record.tunnel_identity()?;
    let route = RouteId::parse(&record.route).ok_or(Error::RelayInfo)?;
    let mut failure = Error::NoRelays;
    for relay in &record.relays {
        let Ok(endpoint) = Endpoint::parse(relay) else {
            continue;
        };
        let attempt = async {
            let connection = client::reach(&endpoint).await?;
            let halves = dial(&connection, route, &keys.dial).await?;
            Ok::<_, client::Error>((connection, halves))
        };
        let (connection, (send, recv)) = match tokio::time::timeout(ATTEMPT, attempt).await {
            Ok(Ok(dialed)) => dialed,
            Ok(Err(error)) => {
                failure.record_relay(&endpoint, error);
                continue;
            }
            Err(_) => continue,
        };
        let (initiator, first) =
            sheprd_tunnel::Initiator::session(&keys.tunnel, &identity, route.as_bytes())?;
        let tunnel = tokio::time::timeout(
            ATTEMPT,
            sheprd_tunnel::stream::connect(join(recv, send), initiator, &first, identity.x25519()),
        )
        .await
        .unwrap_or(Err(sheprd_tunnel::Error::Closed));
        let tunnel = match relay_result(connection.close_reason(), tunnel.map_err(Error::Tunnel)) {
            Err(Error::RelayBanned(ban)) => {
                failure.record_relay(&endpoint, client::Error::Banned(ban));
                continue;
            }
            result => result?,
        };
        return Ok((connection, tunnel));
    }
    Err(failure)
}

/// Dial until the relay accepts the session or the caller's attempt deadline
/// expires. `Busy` and `Timeout` mean the route was found but could not accept
/// this particular stream yet; all other responses are durable failures and
/// should move on to the next relay immediately.
async fn dial(
    connection: &quinn::Connection,
    route: RouteId,
    key: &p256::ecdsa::SigningKey,
) -> Result<(quinn::SendStream, quinn::RecvStream), client::Error> {
    dial_with(|| client::dial(connection, route, key)).await
}

async fn dial_with<F, Fut, T>(mut attempt: F) -> Result<T, client::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, client::Error>>,
{
    let mut delay = DIAL_RETRY_INITIAL;
    loop {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(error) if retryable(&error) => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(DIAL_RETRY_MAX);
            }
            Err(error) => return Err(error),
        }
    }
}

fn retryable(error: &client::Error) -> bool {
    matches!(
        error,
        client::Error::Refused(
            sheprd_relay::wire::Status::Busy | sheprd_relay::wire::Status::Timeout
        )
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::desktop::connect::{ATTEMPT, dial, dial_with, relay_result, retryable, splice};
    use p256::ecdsa::SigningKey;
    use sheprd_relay::{Limits, Options as RelayOptions, RouteId, client, wire::Status};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    #[test]
    fn relay_ban_survives_tunnel_failure_and_clean_eof() {
        use crate::desktop::Error;
        use sheprd_relay::{ban::Ban, wire::code};

        let closed = quinn::ConnectionError::ApplicationClosed(quinn::ApplicationClose {
            error_code: code::BANNED.into(),
            reason: b"manual 60".as_slice().into(),
        });
        for result in [Err(Error::Tunnel(sheprd_tunnel::Error::Closed)), Ok(())] {
            let error = relay_result(Some(closed.clone()), result).unwrap_err();
            assert_eq!(error.ban(), Some(Ban::parse("manual 60")));
        }
        assert!(relay_result(None, Ok(())).is_ok());
        assert!(matches!(
            relay_result(
                Some(quinn::ConnectionError::TimedOut),
                Err::<(), _>(Error::Tunnel(sheprd_tunnel::Error::Closed))
            ),
            Err(Error::Tunnel(sheprd_tunnel::Error::Closed))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn ssh_leaving_ends_the_splice_though_the_machine_stays_open() {
        let (ours, mut machine) = tokio::io::duplex(64);
        let (output, _kept) = tokio::io::duplex(64);
        // `ssh` sends a little, then closes its side, as it does on exit.
        let input: &[u8] = b"bye";
        let spliced = tokio::spawn(splice(input, output, ours, Duration::from_secs(3)));
        let mut seen = [0_u8; 3];
        machine.read_exact(&mut seen).await.unwrap();
        assert_eq!(&seen, b"bye");
        // The machine never closes: the splice ends on its own.
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(spliced.is_finished());
        spliced.await.unwrap().unwrap();
        drop(machine);
    }

    #[tokio::test(start_paused = true)]
    async fn the_machines_last_bytes_still_reach_ssh() {
        let (ours, mut machine) = tokio::io::duplex(64);
        let (output, mut read) = tokio::io::duplex(64);
        let input: &[u8] = b"";
        let spliced = tokio::spawn(splice(input, output, ours, Duration::from_secs(3)));
        machine.write_all(b"last").await.unwrap();
        drop(machine);
        spliced.await.unwrap().unwrap();
        let mut seen = Vec::new();
        read.read_to_end(&mut seen).await.unwrap();
        assert_eq!(seen, b"last");
    }

    #[test]
    fn only_transient_relay_refusals_are_retried() {
        for status in [Status::Busy, Status::Timeout] {
            assert!(retryable(&client::Error::Refused(status)));
        }
        for status in [Status::Offline, Status::Limited, Status::TooMany] {
            assert!(!retryable(&client::Error::Refused(status)));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn dial_backs_off_within_the_relay_allowance_and_stops_at_the_deadline() {
        let start = tokio::time::Instant::now();
        let mut attempts = Vec::new();
        let result = tokio::time::timeout(
            ATTEMPT,
            dial_with(|| {
                attempts.push(start.elapsed());
                std::future::ready(Err::<(), _>(client::Error::Refused(Status::Busy)))
            }),
        )
        .await;
        assert!(result.is_err(), "persistent Busy must stop at ATTEMPT");
        assert_eq!(
            attempts,
            [
                Duration::from_millis(0),
                Duration::from_millis(250),
                Duration::from_millis(750),
                Duration::from_millis(1_750),
                Duration::from_millis(3_750),
                Duration::from_millis(5_750),
                Duration::from_millis(7_750),
                Duration::from_millis(9_750),
            ],
            "backoff must stay well below the relay's 30-dial burst / 120/min allowance",
        );
    }

    #[tokio::test(start_paused = true)]
    async fn dial_retries_transient_refusals_until_success() {
        let start = tokio::time::Instant::now();
        let mut attempts = 0usize;
        let result = dial_with(|| {
            attempts += 1;
            let outcome = match attempts {
                1 => Err(client::Error::Refused(Status::Busy)),
                2 => Err(client::Error::Refused(Status::Timeout)),
                _ => Ok("spliced"),
            };
            std::future::ready(outcome)
        })
        .await;
        assert_eq!(result.unwrap(), "spliced");
        assert_eq!(attempts, 3);
        assert_eq!(start.elapsed(), Duration::from_millis(750));
    }

    #[tokio::test(start_paused = true)]
    async fn dial_returns_durable_refusals_without_retrying() {
        let start = tokio::time::Instant::now();
        let mut attempts = 0usize;
        let result = dial_with(|| {
            attempts += 1;
            std::future::ready(Err::<(), _>(client::Error::Refused(Status::Offline)))
        })
        .await;
        assert!(matches!(
            result,
            Err(client::Error::Refused(Status::Offline))
        ));
        assert_eq!(attempts, 1);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dial_retries_busy_on_the_same_connection_until_a_slot_opens() {
        let (_, quic, pin) = crate::relay::tests::relay_with(RelayOptions {
            limits: Limits {
                max_sessions_per_key: 1,
                ..Limits::default()
            },
            metrics: None,
            ..RelayOptions::default()
        })
        .await;
        let host_key = SigningKey::from_slice(&[0x41; 32]).unwrap();
        let dial_key = SigningKey::from_slice(&[0x42; 32]).unwrap();
        let listener_conn = client::connect(quic, pin).await.unwrap();
        let mut listener = client::Listener::listen(listener_conn, &host_key, &[])
            .await
            .unwrap();
        listener
            .set_dial_keys(vec![client::public(&dial_key)])
            .await
            .unwrap();
        let route = listener.route();

        // One shared QUIC connection for the dialer across all attempts.
        let connection = client::connect(quic, pin).await.unwrap();

        // A durable refusal on an unknown route returns promptly without retrying.
        let unknown = RouteId::of(SigningKey::from_slice(&[0x43; 32]).unwrap().verifying_key());
        let offline = tokio::time::timeout(
            Duration::from_secs(2),
            dial(&connection, unknown, &dial_key),
        )
        .await
        .expect("durable refusal should return promptly");
        assert!(matches!(
            offline,
            Err(client::Error::Refused(Status::Offline))
        ));

        // Fill the single session slot so the relay answers Busy.
        let (first_dial, first_accept) = tokio::join!(
            client::dial(&connection, route, &dial_key),
            listener.accept()
        );
        let (mut first_send, mut first_recv) = first_dial.unwrap();
        let mut first_incoming = first_accept.unwrap();
        assert!(matches!(
            client::dial(&connection, route, &dial_key).await,
            Err(client::Error::Refused(Status::Busy))
        ));

        // While the slot remains occupied, persistent Busy stops at the deadline
        // rather than exhausting the dial rate limit into Limited.
        let timed_out = tokio::time::timeout(
            Duration::from_millis(600),
            dial(&connection, route, &dial_key),
        )
        .await;
        assert!(timed_out.is_err(), "persistent Busy must stop at deadline");

        // Free the occupied slot while `dial` is retrying on the same connection.
        let release = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = first_send.finish();
            let _ = first_recv.stop(0u32.into());
            let _ = first_incoming.send.finish();
            let _ = first_incoming.recv.stop(0u32.into());
            drop((first_send, first_recv, first_incoming));
            listener.accept().await.unwrap()
        };
        let (retried, mut second_incoming) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(dial(&connection, route, &dial_key), release)
        })
        .await
        .expect("retried dial should succeed once the first session closes");
        let (mut second_send, _second_recv) = retried.unwrap();
        second_send.write_all(b"ok").await.unwrap();
        let _ = second_send.finish();
        let mut seen = [0_u8; 2];
        second_incoming.recv.read_exact(&mut seen).await.unwrap();
        assert_eq!(&seen, b"ok");
    }
}
