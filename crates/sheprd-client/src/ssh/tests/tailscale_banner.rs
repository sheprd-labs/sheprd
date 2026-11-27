//! The Tailscale banner a run has to notice. A host in check mode holds the
//! connection and `ssh` never prompts, so a run that does not read stderr can
//! only report its timeout; these tests pin that the refusal comes from the
//! banner, at once, with the whole address a surface can offer — including when
//! the banner arrives in pieces, after a host has been verbose.

use super::*;

/// `sh` printing Tailscale's banner to stderr and then holding the connection,
/// as a host in check mode does. Nothing here lets go of it on its own, so a
/// run that waited would report the timeout instead.
fn host_waiting_for_a_check() -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(
            r#"printf '%s\n' \
'Tailscale SSH requires an additional check to connect to alice@node1:' \
'    https://login.tailscale.com/a/abc123' >&2
while :; do sleep 1; done"#,
        )
        .stderr(Stdio::piped());
    command
}

/// The banner ends the run with the check it named, well before the budget the
/// caller gave: the address belongs to this connection, so holding it open for
/// a check nobody may answer is time the window spends showing nothing.
#[test]
fn a_banner_is_reported_at_once_instead_of_timing_out() {
    let started = Instant::now();
    let error = match run_reading_banner(&mut host_waiting_for_a_check(), PROBE_TIMEOUT, || false) {
        Ok(_) => panic!("a host holding a check never finishes the run"),
        Err(error) => error,
    };
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the check was read from the banner, not waited out: {:?}",
        started.elapsed()
    );
    match error {
        crate::Error::Ssh(Error::Refused(SshFailure::Tailscale(check))) => {
            assert_eq!(check.url(), Some("https://login.tailscale.com/a/abc123"));
        }
        other => panic!("expected the Tailscale check, got {other:?}"),
    }
}

/// The marker and the address reach the pipe as separate reads. Reporting the
/// first one would hand the window a check with nothing to open, so the run
/// waits for the address.
#[test]
fn an_address_written_after_the_marker_is_waited_for() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(
            r#"printf '%s\n' \
'Tailscale SSH requires an additional check to connect to alice@node1:' >&2
sleep 0.3
printf '%s\n' '    To authenticate, visit: https://login.tailscale.com/a/abc123' >&2
while :; do sleep 1; done"#,
        )
        .stderr(Stdio::piped());
    match run_reading_banner(&mut command, Duration::from_secs(20), || false) {
        Err(crate::Error::Ssh(Error::Refused(SshFailure::Tailscale(check)))) => assert_eq!(
            check.url(),
            Some("https://login.tailscale.com/a/abc123"),
            "the whole address, not the marker alone"
        ),
        other => panic!("expected the Tailscale check, got {other:?}"),
    }
}

/// A host that chatters past the retained tail and only then asks for a check.
/// The tail stops growing at its bound, so the scan has to notice output by
/// more than its length, or the banner is missed and the run reports a timeout.
#[test]
fn a_banner_after_a_full_tail_is_still_found() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(
            r#"i=0
while [ "$i" -lt 120 ]; do
    printf 'noise noise noise noise noise noise noise noise noise noise\n' >&2
    i=$((i + 1))
done
printf '%s\n' \
'Tailscale SSH requires an additional check to connect to alice@node1:' \
'    To authenticate, visit: https://login.tailscale.com/a/abc123' >&2
while :; do sleep 1; done"#,
        )
        .stderr(Stdio::piped());
    match run_reading_banner(&mut command, PROBE_TIMEOUT, || false) {
        Err(crate::Error::Ssh(Error::Refused(SshFailure::Tailscale(check)))) => assert_eq!(
            check.url(),
            Some("https://login.tailscale.com/a/abc123"),
            "the banner is read even though the tail had stopped growing"
        ),
        other => panic!("expected the Tailscale check, got {other:?}"),
    }
}

/// A banner whose address Sheprd cannot read still says why the host stopped,
/// so the run reports the check rather than a bare timeout.
#[test]
fn a_banner_without_a_readable_address_is_still_a_check() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("printf '%s\\n' 'Tailscale SSH requires an additional check' >&2; sleep 30")
        .stderr(Stdio::piped());
    match run_reading_banner(&mut command, Duration::from_secs(5), || false) {
        Err(crate::Error::Ssh(Error::Refused(SshFailure::Tailscale(check)))) => {
            assert_eq!(check.url(), None, "no address was readable");
        }
        other => panic!("expected the Tailscale check, got {other:?}"),
    }
}
