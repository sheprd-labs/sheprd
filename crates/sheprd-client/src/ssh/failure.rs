//! Why `ssh` closed the bridge before its ready banner. The child's exit code
//! and a bounded stderr tail are read only to pick a class here; the text is
//! never retained, logged, or shown, since it can name users, keys, and paths.
//! A relay ban's line is read into its closed set of reasons
//! ([`sheprd_relay::ban`]), never kept as text. The one class that carries text
//! of its own is [`SshFailure::Tailscale`], whose address is the question the
//! user is asked; see [`check`].
use super::check::{self, TailscaleCheck};
use crate::Result;
use sheprd_relay::ban::Ban;
use std::{
    io::{ErrorKind, Read},
    net::{IpAddr, ToSocketAddrs},
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc,
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

/// What stopped an SSH bridge before it was ready.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SshFailure {
    /// The host key is unknown or changed; strict checking refused it.
    #[error("the host key is not trusted; verify it by running ssh to the host in a terminal")]
    HostKey,
    /// No credential was accepted without a prompt.
    #[error(
        "authentication failed; load a key into the agent or open a ControlMaster connection in a terminal"
    )]
    Auth,
    /// The host could not be resolved or reached.
    #[error("the host is unreachable")]
    Unreachable,
    /// The system had no route to the address.
    #[error("there is no route to the host")]
    NoRoute,
    /// macOS refused a local network address because Herdr lacks Local
    /// Network permission, or is still waiting for the user to answer its
    /// alert. `ssh` only sees no route to the host; this class is kept for
    /// when the app's own check confirms the refusal, so an offline device
    /// or a bad route stays `NoRoute`.
    #[error(
        "macOS denied Sheprd access to your local network. If it just asked, allow it and try again; otherwise turn on Sheprd in System Settings › Privacy & Security › Local Network"
    )]
    LocalNetworkDenied,
    /// No Herdr was found in the known install locations. One that is
    /// installed but cannot serve this client is `crate::Error::BridgeIncompatible`.
    #[error("no Herdr is installed on the host")]
    HerdrMissing,
    /// The relay that carries this host (`crate::relay_host`) banned this
    /// computer's network address; why and for how long when it said.
    #[error("the relay banned this computer's network address{0}")]
    RelayBanned(Ban),
    /// Tailscale SSH authenticates from the tailnet policy and, in check mode,
    /// holds the connection until the user proves their identity in a browser.
    /// `ssh` prints the address in its auth banner and never prompts, so a
    /// noninteractive client can only wait; see [`check`].
    #[error("Tailscale SSH wants you to verify your identity: {0}")]
    Tailscale(TailscaleCheck),
    /// None of the above could be told from what `ssh` reported.
    #[error("check host trust, authentication, and remote Herdr installation")]
    Other,
}

impl SshFailure {
    /// Whether retrying soon is pointless: the user has to fix something
    /// outside the app first, such as a key, the known hosts, an install, or
    /// a Tailscale check, or wait out a relay's ban.
    pub fn needs_user(&self) -> bool {
        matches!(
            self,
            Self::HostKey
                | Self::Auth
                | Self::HerdrMissing
                | Self::RelayBanned(_)
                | Self::Tailscale(_)
        )
    }

    /// `ssh` exits 255 for its own failures; the bridge script exits 127 when
    /// it found no candidate binary at all. Any other status is the remote's.
    pub(crate) fn classify(code: Option<i32>, stderr: &[u8]) -> Self {
        Self::classify_with(code, stderr, resolve, crate::local_network_access)
    }

    /// [`Self::classify`] with the name lookup a hostname in the error needs
    /// and the check of this app's Local Network permission, which `ssh`, as
    /// this app's child, shares.
    fn classify_with(
        code: Option<i32>,
        stderr: &[u8],
        resolve: impl FnOnce(&str) -> Vec<IpAddr>,
        access: impl FnOnce() -> crate::LocalNetworkAccess,
    ) -> Self {
        // Tailscale's banner is sent before any host key is checked or key is
        // offered, so it explains the run whatever else `ssh` went on to
        // print, including a `Permission denied` from a policy that refused
        // the check outright. It also explains a run that reported no exit
        // code at all because the client gave up on it.
        if let Some(check) = check::pending(stderr) {
            return Self::Tailscale(check);
        }
        match code {
            Some(127) => Self::HerdrMissing,
            Some(255) => {
                let text = String::from_utf8_lossy(stderr);
                let has = |markers: &[&str]| markers.iter().any(|m| text.contains(m));
                // `relay-connect` names the ban first; what `ssh` adds after
                // it, a closed connection, is only the consequence.
                if let Some(ban) = Ban::from_stderr(&text) {
                    Self::RelayBanned(ban)
                } else if has(&[
                    "Host key verification failed",
                    "REMOTE HOST IDENTIFICATION HAS CHANGED",
                ]) {
                    Self::HostKey
                } else if has(&["Permission denied", "Too many authentication failures"]) {
                    Self::Auth
                } else if has(&["No route to host"]) {
                    // No route to a LAN address is also how an offline
                    // device fails, so only a confirmed refusal names the
                    // permission.
                    if cfg!(target_os = "macos")
                        && refused_host(&text).is_some_and(|host| match host.parse::<IpAddr>() {
                            Ok(address) => is_local(address),
                            Err(_) => resolve(host).into_iter().any(is_local),
                        })
                        && access() == crate::LocalNetworkAccess::Denied
                    {
                        Self::LocalNetworkDenied
                    } else {
                        Self::NoRoute
                    }
                } else if has(&[
                    "Could not resolve hostname",
                    "Connection refused",
                    "timed out",
                    "Network is unreachable",
                    "Connection closed by",
                    "Connection reset",
                ]) {
                    Self::Unreachable
                } else {
                    Self::Other
                }
            }
            _ => Self::Other,
        }
    }
}

/// The host in `ssh: connect to host <host> port <port>: ...`. `ssh` prints
/// the name it was given, so this is an address only when the target was one.
fn refused_host(stderr: &str) -> Option<&str> {
    stderr.lines().rev().find_map(|line| {
        let (_, rest) = line.split_once("connect to host ")?;
        let (host, _) = rest.split_once(" port ")?;
        Some(host)
    })
}

/// Addresses macOS treats as the local network, which Local Network privacy
/// gates: private and link-local ranges, not loopback or the Internet.
/// Tailscale's ranges are a tunnel, not the LAN: its IPv4 range (100.64/10)
/// is not private, and its IPv6 one is carved out of unique-local below.
fn is_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => (v6.is_unique_local() && !is_tailscale(v6)) || v6.is_unicast_link_local(),
    }
}

/// Tailscale's IPv6 prefix, `fd7a:115c:a1e0::/48`.
fn is_tailscale(address: std::net::Ipv6Addr) -> bool {
    address.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0]
}

/// How long classification waits for a hostname's addresses. `ssh` has just
/// resolved the same name, so the system usually answers from its cache.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(1);

/// The addresses `host` resolves to, or none once `RESOLVE_TIMEOUT` passes.
/// The lookup runs on its own thread because the resolver cannot be
/// cancelled; a slow one is abandoned rather than waited on.
fn resolve(host: &str) -> Vec<IpAddr> {
    let (tx, rx) = mpsc::sync_channel(1);
    let host = host.to_owned();
    let spawned = thread::Builder::new()
        .name("herdr-ssh-resolve".into())
        .spawn(move || {
            let addresses = (host.as_str(), 0)
                .to_socket_addrs()
                .map(|addresses| addresses.map(|address| address.ip()).collect())
                .unwrap_or_default();
            let _ = tx.send(addresses);
        });
    match spawned {
        Ok(_) => rx.recv_timeout(RESOLVE_TIMEOUT).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Bytes of stderr kept for classification. `ssh` reports its failure last.
const STDERR_TAIL: usize = 4096;
/// How long a closed bridge may take to exit and finish its stderr.
const DIAGNOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// What a child has written to stderr, read as it arrives rather than only
/// once the pipe closes. A run that never reports a result — a Tailscale check
/// waiting on a browser, a deadline the client gave up on — is still readable
/// for why, which is the whole point of classifying these.
#[derive(Clone, Default)]
pub(super) struct Stderr(Arc<Mutex<Tail>>);

#[derive(Default)]
struct Tail {
    bytes: Vec<u8>,
    /// Bytes read from the child, which keeps growing once `bytes` is full. A
    /// caller watching for a banner reads this to know the output moved on.
    read: usize,
    /// The reader reached EOF, so `bytes` is all the child wrote.
    done: bool,
}

impl Stderr {
    fn with<T>(&self, read: impl FnOnce(&mut Tail) -> T) -> T {
        read(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Bytes read so far, whether or not the retained tail is still growing.
    /// Comparing this between looks is how a caller avoids re-reading output
    /// that has not changed.
    pub(super) fn total(&self) -> usize {
        self.with(|tail| tail.read)
    }

    /// The tail read so far, for classification only.
    pub(super) fn text(&self) -> Vec<u8> {
        self.with(|tail| tail.bytes.clone())
    }

    /// The tail once the reader has finished, or what it holds after `limit`.
    pub(super) fn finished(&self, limit: Duration) -> Vec<u8> {
        let deadline = Instant::now() + limit;
        loop {
            let (done, bytes) = self.with(|tail| (tail.done, tail.bytes.clone()));
            if done || Instant::now() >= deadline {
                return bytes;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Drain the child's stderr for its whole life, so a chatty remote can never
/// fill the pipe and stall `ssh`, publishing the bounded tail as it arrives.
///
/// The reader is any stream, not just a `ChildStderr`: on Windows the child's
/// stderr is a private overlapped pipe this process made, and it arrives here
/// as a file.
pub(super) fn drain_live(stderr: impl Read + Send + 'static) -> Result<Stderr> {
    let shared = Stderr::default();
    let reader = shared.clone();
    thread::Builder::new()
        .name("herdr-ssh-stderr".into())
        .spawn(move || pump(stderr, &reader))?;
    Ok(shared)
}

/// The check this output says the connection is waiting on, if any.
pub(super) fn waiting(stderr: &Stderr) -> Option<TailscaleCheck> {
    check::pending(&stderr.text())
}

/// The check this output says the connection is waiting on, once the address it
/// names has fully arrived. A banner arrives over several reads, so a tail that
/// still has output coming can be missing its address, or hold one that runs to
/// its end and may still grow; [`waiting`] is for output that has finished.
pub(super) fn waiting_live(stderr: &Stderr) -> Option<TailscaleCheck> {
    check::pending_live(&stderr.text())
}

fn pump(mut stderr: impl Read, shared: &Stderr) {
    let mut buffer = [0u8; 1024];
    loop {
        let n = match stderr.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        shared.with(|kept| {
            kept.read += n;
            kept.bytes.extend_from_slice(&buffer[..n]);
            if kept.bytes.len() > STDERR_TAIL {
                kept.bytes.drain(..kept.bytes.len() - STDERR_TAIL);
            }
        });
    }
    shared.with(|kept| kept.done = true);
}

/// Classify a bridge whose stdout closed before it was ready, with its exit
/// code when it had one. Waits on the connection worker, at most
/// `DIAGNOSE_TIMEOUT`, for the exit and stderr.
pub(super) fn diagnose(
    child: &mut Child,
    stderr: &Stderr,
    stop: &AtomicBool,
) -> (Option<i32>, SshFailure) {
    let deadline = Instant::now() + DIAGNOSE_TIMEOUT;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < deadline && !stop.load(Ordering::Acquire) => {
                thread::sleep(Duration::from_millis(10));
            }
            _ => {
                // A child still running, or one the client killed, reports no
                // code; what it wrote may still say the host wants a check.
                return (
                    None,
                    waiting(stderr).map_or(SshFailure::Other, SshFailure::Tailscale),
                );
            }
        }
    };
    let stderr = stderr.finished(deadline.saturating_duration_since(Instant::now()));
    (code, SshFailure::classify(code, &stderr))
}

#[cfg(test)]
mod tests;
