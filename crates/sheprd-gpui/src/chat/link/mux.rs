//! One multiplexer per SSH host: `sheprd-acp multiplex` run there over a
//! single channel of the host's stream master, carrying every chat this app
//! attaches on that host (`sheprd_chat_protocol::mux`). A channel per chat
//! would run into sshd's `MaxSessions` on one login, and through a relay,
//! which holds each device key to a few sessions, into refused logins.
//!
//! A host whose `sheprd-acp` predates the subcommand, or that cannot start
//! one, sends no greeting; its chats then dial a channel each, as before,
//! and the multiplexer is tried again later. Everything here blocks, so it
//! runs on a link's reader thread, never the UI thread.

use sheprd_chat_protocol::{
    EndpointId,
    mux::{self, Mux, Stream},
};
use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

/// How long a new multiplexer may take to greet: the host's stream master
/// may be starting (up to 20 s), then the program must start there.
const HELLO_TIMEOUT: Duration = Duration::from_secs(35);
/// After a host's program refused to multiplex, as an older one does, its
/// chats dial one channel each for this long before it is asked again.
const FALLBACK: Duration = Duration::from_secs(10 * 60);
/// After a multiplexer failed to connect or greet in time, as when the host
/// is briefly unreachable, how long its chats dial one channel each.
const RETRY: Duration = Duration::from_secs(30);
/// A multiplexer that carries no chat for this long stops, freeing its
/// channel; the next chat starts another.
const IDLE: Duration = Duration::from_secs(60);
/// How long a multiplexer that ended without greeting has to exit on its
/// own, so its status can tell a refusal from a failed login.
const EXIT_WAIT: Duration = Duration::from_secs(2);
/// The exit status `ssh` reports for its own failures, such as a host it
/// could not reach, as opposed to the remote program's.
const SSH_FAILED: i32 = 255;

static HOSTS: LazyLock<Registry> = LazyLock::new(Registry::default);

/// A stream to `endpoint`'s chat over `host`'s multiplexer, started if it
/// has none. `None` means dial the chat on a channel of its own.
pub(super) fn open(host: &str, endpoint: &EndpointId) -> Option<Stream> {
    let generation = sheprd_client::shared_generation(host);
    HOSTS.open(host, endpoint, generation, || start(host))
}

/// Each host's multiplexer, by SSH target.
#[derive(Default)]
struct Registry {
    hosts: Mutex<HashMap<String, Arc<Mutex<Slot>>>>,
}

#[derive(Default)]
enum Slot {
    #[default]
    None,
    /// Started under the host's connection `generation`
    /// ([`sheprd_client::shared_generation`]).
    Up { mux: Mux, generation: u64 },
    /// Chats dial on their own until then.
    Direct(Instant),
}

/// Why a multiplexer did not start.
#[derive(Debug)]
enum Failed {
    /// The host's program does not multiplex: try again much later.
    Refused,
    /// The connection or the greeting failed: try again soon.
    Unreachable,
}

impl Registry {
    /// Opens a stream on `host`'s multiplexer, starting one with `start`
    /// when there is none, it ended, or it was started under an earlier
    /// connection `generation`: after a reconnect the target may name
    /// another machine, so new chats must not reach the old one. A replaced
    /// multiplexer keeps the chats it carries and ends once they close. A
    /// host's slot stays locked while it starts, so the chats attaching at
    /// once wait for one multiplexer rather than each starting their own.
    fn open(
        &self,
        host: &str,
        endpoint: &EndpointId,
        generation: u64,
        start: impl Fn() -> Result<Mux, Failed>,
    ) -> Option<Stream> {
        let slot = self
            .hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(host.to_owned())
            .or_default()
            .clone();
        let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
        // A multiplexer that ended since it was last used is started again,
        // once.
        for _ in 0..2 {
            match &*slot {
                Slot::Direct(until) if Instant::now() < *until => return None,
                Slot::Up {
                    mux,
                    generation: started,
                } if *started == generation => match mux.open(endpoint) {
                    Ok(stream) => return Some(stream),
                    Err(mux::Error::Ended) => {}
                    // Full: this chat dials on its own.
                    Err(_) => return None,
                },
                Slot::Up { .. } | Slot::Direct(_) | Slot::None => {}
            }
            *slot = match start() {
                Ok(mux) => Slot::Up { mux, generation },
                Err(failed) => {
                    let wait = match failed {
                        Failed::Refused => FALLBACK,
                        Failed::Unreachable => RETRY,
                    };
                    tracing::info!(host, ?failed, "Chats on this host dial one channel each");
                    Slot::Direct(Instant::now() + wait)
                }
            };
        }
        None
    }
}

/// Starts `sheprd-acp multiplex` on `host` over its stream master, and
/// waits for its greeting.
fn start(host: &str) -> Result<Mux, Failed> {
    let script = super::super::remote_program_script("multiplex");
    let (command, channel) =
        sheprd_client::shared_script_command(host, &script, sheprd_client::SshLane::Stream)
            .map_err(|_| Failed::Unreachable)?;
    spawn(command, channel)
}

/// Runs `command` as a multiplexer's transport: its stdin and stdout carry
/// the frames. `keep` (the master's channel) lives until the child is
/// reaped.
fn spawn(mut command: Command, keep: impl Send + 'static) -> Result<Mux, Failed> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Failed::Unreachable)?;
    let streams = child.stdout.take().zip(child.stdin.take());
    let mux = Mux::client();
    let exited = reaper(mux.clone(), child, keep);
    let Some((stdout, stdin)) = streams else {
        mux.shutdown();
        return Err(Failed::Unreachable);
    };
    let receiver = mux.clone();
    let sender = mux.clone();
    let threads = thread::Builder::new()
        .name("herdr-gui-chat-mux-in".into())
        .spawn(move || {
            let _ = receiver.receive_loop(stdout);
        })
        .and_then(|_| {
            thread::Builder::new()
                .name("herdr-gui-chat-mux-out".into())
                .spawn(move || {
                    let _ = sender.send_loop(stdin);
                    sender.shutdown();
                })
        });
    if threads.is_err() {
        mux.shutdown();
        return Err(Failed::Unreachable);
    }
    match mux.wait_hello(HELLO_TIMEOUT) {
        Ok(_) => Ok(mux),
        Err(error) => {
            mux.shutdown();
            let code = exited
                .and_then(|exited| exited.recv_timeout(EXIT_WAIT * 2).ok())
                .flatten();
            Err(classify(&error, code))
        }
    }
}

/// Whether a multiplexer that never greeted was refused by the host's
/// program, given how its transport exited.
fn classify(error: &mux::Error, code: Option<i32>) -> Failed {
    match (error, code) {
        (mux::Error::NotMultiplexer | mux::Error::Version(_), Some(code)) if code != SSH_FAILED => {
            Failed::Refused
        }
        _ => Failed::Unreachable,
    }
}

/// A thread that ends the child once `mux` ends or idles, reaps it, and
/// then sends its exit code. `None` when the thread could not start, in
/// which case the child is ended here.
fn reaper(mux: Mux, child: Child, keep: impl Send + 'static) -> Option<Receiver<Option<i32>>> {
    let (sender, exited) = mpsc::sync_channel(1);
    let child = Arc::new(Mutex::new(child));
    let reaped = child.clone();
    let spawned = thread::Builder::new()
        .name("herdr-gui-chat-mux".into())
        .spawn(move || {
            mux.end_when_idle(IDLE);
            let code = reap(&reaped);
            drop(keep);
            let _ = sender.send(code);
        });
    if spawned.is_err() {
        let _ = reap(&child);
        return None;
    }
    Some(exited)
}

/// Gives the child [`EXIT_WAIT`] to exit by itself, kills it otherwise, and
/// returns its exit code; a killed child has none.
fn reap(child: &Mutex<Child>) -> Option<i32> {
    let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
    let deadline = Instant::now() + EXIT_WAIT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    child.wait().ok().and_then(|status| status.code())
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
