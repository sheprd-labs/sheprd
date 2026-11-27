//! Running an `ssh` child whose stderr may name a Tailscale check.
//!
//! Tailscale SSH in check mode prints a banner naming an address and then holds
//! the connection, and OpenSSH at its default `LogLevel` never prompts: a client
//! that watches nothing sees only its own timeout. Reading the banner as it
//! appears turns that timeout into a refusal that says what to open, within one
//! poll instead of after the whole budget.
//!
//! The connection is given up on as the refusal is returned: the address the
//! banner names authorises the child that printed it, so a surface shows the
//! address and the next connection is the one that check lets through. Nothing
//! here keeps a child alive for a check nobody may answer. Everything else
//! about a run — its timeout, its cancellation, its bounded output — is the
//! caller's, and unchanged.

use super::{ChildGuard, Error, PROBE_OUTPUT_LIMIT, SshFailure, failure};
use crate::Result;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// An `ssh` command's outcome with a Tailscale check reported as a refusal.
///
/// When the host stops for a check, this returns
/// [`Error::Refused`]`(`[`SshFailure::Tailscale`]`)` at the banner rather than
/// waiting out `timeout`; every other run ends as it would without the check.
pub(super) fn run(
    command: &mut Command,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    #[cfg(unix)]
    let mut child = ChildGuard(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    #[cfg(unix)]
    let stdout = child.0.stdout.take().ok_or(Error::Closed)?;
    #[cfg(unix)]
    let stderr = match child.0.stderr.take() {
        Some(stderr) => Some(failure::drain_live(stderr)?),
        None => None,
    };
    // OpenSSH for Windows' `ssh.exe` stops delivering output into a
    // synchronous pipe once its stdin has ended, on stderr as on stdout (a
    // Windows host's PowerShell writes progress there); see
    // `overlapped_output`. So stderr is always read here, and a caller that
    // wanted it discarded ignores its tail.
    #[cfg(windows)]
    let (mut child, stdout, stderr) = {
        let (stdout, out) = crate::overlapped_output()?;
        let (stderr, err) = crate::overlapped_output()?;
        let child = command.stdin(Stdio::null()).stdout(out).stderr(err).spawn();
        // The caller's command holds the child's ends until replaced, and an
        // exited child must read as closed.
        command.stdout(Stdio::null()).stderr(Stdio::null());
        (
            ChildGuard(child?),
            stdout,
            Some(failure::drain_live(stderr)?),
        )
    };
    // The pipe reaches EOF once the child exits or the guard kills it.
    let reader = std::thread::Builder::new()
        .name("herdr-ssh-command".into())
        .spawn(move || {
            let mut output = Vec::new();
            stdout
                .take(PROBE_OUTPUT_LIMIT + 1)
                .read_to_end(&mut output)
                .map(|_| output)
        })?;
    // Borrowed for the scan; the run hands over its tail once the child ends.
    let tail = stderr.as_ref();
    let deadline = Instant::now() + timeout;
    // The scan is only worth repeating when more output arrived. That is counted
    // over the child's whole life: the retained tail stops growing once it is
    // full, and a chatty host fills it long before it asks for a check.
    let mut scanned = 0;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if cancelled() {
            return Err(Error::Cancelled.into());
        }
        let read = tail.map_or(0, failure::Stderr::total);
        if read != scanned {
            scanned = read;
            if let Some(found) = tail.and_then(failure::waiting_live) {
                return Err(Error::Refused(SshFailure::Tailscale(found)).into());
            }
        }
        if Instant::now() >= deadline {
            // A check that went unfinished says so, with whatever address the
            // output managed to carry: the address belongs to this connection,
            // which the guard closes as this returns.
            return Err(match tail.and_then(failure::waiting) {
                Some(check) => Error::Refused(SshFailure::Tailscale(check)).into(),
                None => Error::Timeout.into(),
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = reader.join().map_err(|_| Error::Closed)??;
    if output.len() as u64 > PROBE_OUTPUT_LIMIT {
        return Err(Error::OutputLimit.into());
    }
    // The pipe closes once the child exits, so this waits only for the drain.
    let stderr = stderr
        .map(|stderr| stderr.finished(Duration::from_secs(2)))
        .unwrap_or_default();
    Ok((status, output, stderr))
}
