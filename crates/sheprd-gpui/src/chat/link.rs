//! One connection to a chat program. A reader thread connects, starts a
//! writer thread, and reads; the window only ever polls and queues, through
//! bounded channels, so no socket I/O or thread join reaches the UI thread.
//! The endpoint is `sheprd_client::private_endpoint`: a socket on Unix, and on
//! Windows a pipe checked to be this user's before anything is sent. A chat
//! on an SSH host is reached through a child process instead, `ssh` running
//! `herdr-acp connect` there, whose standard streams carry the same frames,
//! or a stream of the host's one multiplexer ([`mux`]) where it has one.
//! A reader that gets ahead of the window by [`MAX_EVENTS`] messages gives the
//! connection up rather than buffering more, and the window reconnects from
//! its cursor.

mod mux;
mod stderr;

use super::{Error, MAX_EVENTS, MAX_PENDING, Result};
use sheprd_chat_protocol::{
    ClientKind, ClientMessage, Cursor, REVISION, ServerMessage, VERSION, read_frame, write_frame,
};
use sheprd_client::private_endpoint::Connection;

use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::Duration,
};

/// How long connecting waits for a free pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Commands queued for the writer: room for every command a session may
/// have waiting, resent on a new connection, and as many more.
const MAX_COMMANDS: usize = 2 * MAX_PENDING;

/// What one poll of a link found.
#[derive(Debug, Default)]
pub(crate) struct Polled {
    pub(crate) messages: Vec<ServerMessage>,
    /// Why the connection ended, once every message before it was taken.
    pub(crate) end: Option<Error>,
}

/// How a link reaches its program.
pub(crate) enum Dial {
    /// The program's private endpoint on this machine.
    Socket(PathBuf),
    /// A child whose stdin and stdout carry the connection, such as `ssh`
    /// running `herdr-acp connect` on the program's host.
    Process(Command),
    Remote(sheprd_client::ConnectTarget, String),
    /// A script run on an SSH host over the host's shared stream master
    /// (`sheprd_client::SshLane::Stream`), so a window's chats on one host
    /// take one login, not one each. A relay host's every login is a dial
    /// the relay counts, and a dozen chats reconnecting would use up its
    /// rate limit. Resolved on the reader thread: it may wait for the master.
    Ssh(String, String),
    /// The chat with this endpoint id on an SSH host, over the host's
    /// multiplexer, so all its chats share one channel; or, when the host
    /// has none, over [`Self::Ssh`] running `connect`.
    Mux(String, sheprd_chat_protocol::EndpointId),
}

impl From<PathBuf> for Dial {
    fn from(path: PathBuf) -> Self {
        Self::Socket(path)
    }
}

/// Ends a connection from any thread without waiting: shuts the socket
/// down, or kills the child, whose reader then reaps it.
enum Closer {
    Socket(sheprd_client::private_endpoint::Closer),
    Process(Arc<Mutex<Child>>),
    Stream(sheprd_chat_protocol::mux::Stream),
}

impl Closer {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(match self {
            Self::Socket(closer) => Self::Socket(closer.try_clone()?),
            Self::Process(child) => Self::Process(child.clone()),
            Self::Stream(stream) => Self::Stream(stream.clone()),
        })
    }

    fn close(&self) {
        match self {
            Self::Socket(closer) => closer.close(),
            Self::Stream(stream) => stream.close(),
            // Held only while the reader reaps a child already killed.
            Self::Process(child) => {
                if let Ok(mut child) = child.try_lock() {
                    let _ = child.kill();
                }
            }
        }
    }

    /// Reaps a killed child; called only from the reader thread.
    fn reap(&self) {
        if let Self::Process(child) = self
            && let Ok(mut child) = child.lock()
        {
            let _ = child.wait();
        }
    }
}

type Parts = (
    Box<dyn Read + Send>,
    Box<dyn Write + Send>,
    Closer,
    Option<stderr::Tail>,
);

/// Connects as `dial` says, into a reader, a writer, and their closer.
fn open(dial: Dial) -> std::io::Result<Parts> {
    match dial {
        Dial::Socket(path) => {
            let (reader, writer, closer) = Connection::connect(&path, CONNECT_TIMEOUT)?.split()?;
            Ok((
                Box::new(reader),
                Box::new(writer),
                Closer::Socket(closer),
                None,
            ))
        }
        Dial::Process(mut command) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let tail = child.stderr.take().and_then(stderr::Tail::watch);
            let streams = child.stdout.take().zip(child.stdin.take());
            let closer = Closer::Process(Arc::new(Mutex::new(child)));
            let Some((stdout, stdin)) = streams else {
                closer.close();
                closer.reap();
                return Err(std::io::ErrorKind::BrokenPipe.into());
            };
            Ok((Box::new(stdout), Box::new(stdin), closer, tail))
        }
        Dial::Remote(..) | Dial::Ssh(..) | Dial::Mux(..) => {
            Err(std::io::ErrorKind::Unsupported.into())
        }
    }
}

pub(crate) struct Link {
    commands: SyncSender<ClientMessage>,
    events: Receiver<ServerMessage>,
    end: Arc<Mutex<Option<Error>>>,
    socket: Arc<Socket>,
}

/// The connection's closer, kept so dropping the link closes it at once: the
/// threads end on their own, and nothing joins them.
#[derive(Default)]
struct Socket {
    closed: AtomicBool,
    closer: Mutex<Option<Closer>>,
}

impl Socket {
    /// Keeps `closer`, closing at once if the link was dropped while it
    /// connected. The flag is read after the store, so a close that missed
    /// the connection is seen here.
    fn attach(&self, closer: &Closer) {
        if let Ok(clone) = closer.try_clone()
            && let Ok(mut slot) = self.closer.lock()
        {
            *slot = Some(clone);
        }
        if self.closed.load(Ordering::SeqCst) {
            closer.close();
        }
    }

    /// Never waits: a connection being attached meanwhile sees the flag.
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(slot) = self.closer.try_lock()
            && let Some(closer) = slot.as_ref()
        {
            closer.close();
        }
    }
}

impl Link {
    /// Starts connecting to the program `dial` reaches, resuming after
    /// `since`. `resend` goes out right after the greeting: commands an
    /// earlier connection may not have delivered, which the program runs once.
    pub(crate) fn start(
        dial: impl Into<Dial>,
        since: Option<Cursor>,
        resend: impl IntoIterator<Item = ClientMessage>,
    ) -> Result<Self> {
        let dial = dial.into();
        let (commands, queued) = mpsc::sync_channel(MAX_COMMANDS);
        for message in resend {
            commands.try_send(message).map_err(|_| Error::Busy)?;
        }
        let (sender, events) = mpsc::sync_channel(MAX_EVENTS);
        let end = Arc::new(Mutex::new(None));
        let socket = Arc::new(Socket::default());
        let (thread_end, thread_socket) = (end.clone(), socket.clone());
        thread::Builder::new()
            .name("herdr-gui-chat".into())
            .spawn(move || {
                let error = run(dial, since, queued, &sender, &thread_socket);
                if let Ok(mut slot) = thread_end.lock() {
                    *slot = Some(error);
                }
                // The sender drops after the reason is stored, so a window
                // that sees the channel close always finds it.
                drop(sender);
            })
            .map_err(Error::Spawn)?;
        Ok(Self {
            commands,
            events,
            end,
            socket,
        })
    }

    /// Queues `message` for the writer without waiting.
    pub(crate) fn send(&self, message: ClientMessage) -> Result<()> {
        self.commands
            .try_send(message)
            .map_err(|error| match error {
                TrySendError::Full(_) => Error::Busy,
                TrySendError::Disconnected(_) => Error::NotAttached,
            })
    }

    /// Takes at most `budget` messages, and the reason the connection ended
    /// once none are left before it.
    pub(crate) fn poll(&self, budget: usize) -> Polled {
        let mut polled = Polled::default();
        while polled.messages.len() < budget {
            match self.events.try_recv() {
                Ok(message) => polled.messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let end = self.end.try_lock().ok().and_then(|mut end| end.take());
                    polled.end = Some(end.unwrap_or(Error::Closed));
                    break;
                }
            }
        }
        polled
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.socket.close();
    }
}

/// The reader thread: connects, greets through the writer, and reads until
/// the connection ends, returning why.
fn run(
    dial: Dial,
    since: Option<Cursor>,
    queued: Receiver<ClientMessage>,
    events: &SyncSender<ServerMessage>,
    socket: &Socket,
) -> Error {
    if socket.closed.load(Ordering::Acquire) {
        return Error::NotAttached;
    }
    if let Dial::Remote(target, script) = dial {
        return super::remote_route::with_command(&target, &script, &socket.closed, |command| {
            run(Dial::Process(command), since, queued, events, socket)
        })
        .unwrap_or_else(Error::Connect);
    }
    if let Dial::Ssh(host, script) = dial {
        let shared =
            sheprd_client::shared_script_command(&host, &script, sheprd_client::SshLane::Stream);
        // The channel stays open until the child below is reaped. Through a
        // relay, every login of a chat's own is a dial, which a window full
        // of chats retrying would run past the relay's rate with: such a
        // chat waits for room on the shared connection instead.
        return match shared {
            Ok((_, None))
                if waits_for_shared(
                    &host,
                    sheprd_client::rides_own_master(&host, sheprd_client::SshLane::Stream),
                ) =>
            {
                Error::NoSharedLogin
            }
            Ok((command, _channel)) => run(Dial::Process(command), since, queued, events, socket),
            Err(error) => Error::Remote(error),
        };
    }
    if let Dial::Mux(host, endpoint) = dial {
        let Some(stream) = mux::open(&host, &endpoint) else {
            let script = super::remote_program_script(&format!("connect {endpoint}"));
            return run(Dial::Ssh(host, script), since, queued, events, socket);
        };
        let parts: Parts = (
            Box::new(stream.clone()),
            Box::new(stream.clone()),
            Closer::Stream(stream),
            None,
        );
        return serve(parts, since, queued, events, socket);
    }
    match open(dial) {
        Ok(parts) => serve(parts, since, queued, events, socket),
        Err(error) => Error::Connect(error),
    }
}

/// Greets over `parts` through a writer thread, and reads until the
/// connection ends, returning why.
fn serve(
    (reader, writer, closer, tail): Parts,
    since: Option<Cursor>,
    queued: Receiver<ClientMessage>,
    events: &SyncSender<ServerMessage>,
    socket: &Socket,
) -> Error {
    let writer_closer = match closer.try_clone() {
        Ok(writer_closer) => writer_closer,
        Err(error) => {
            closer.close();
            closer.reap();
            return Error::Connect(error);
        }
    };
    socket.attach(&closer);
    let hello = ClientMessage::Hello {
        client: ClientKind::Gui,
        revision: REVISION,
        since,
        version: VERSION,
    };
    if let Err(error) = thread::Builder::new()
        .name("herdr-gui-chat-writer".into())
        .spawn(move || write(writer, &writer_closer, hello, &queued))
    {
        closer.close();
        return Error::Spawn(error);
    }
    let mut reader = BufReader::new(reader);
    let end = loop {
        match read_frame::<_, ServerMessage>(&mut reader) {
            Ok(Some(message)) => match events.try_send(message) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => break Error::Overflow,
                Err(TrySendError::Disconnected(_)) => break Error::NotAttached,
            },
            Ok(None) => break Error::Closed,
            Err(error) => break Error::Read(error),
        }
    };
    closer.close();
    closer.reap();
    // `ssh` says why a remote chat's connection failed only here.
    if let Some(said) = tail.and_then(stderr::Tail::said) {
        tracing::debug!(error = %end, said, "Chat connection's transport said");
    }
    end
}

/// Whether a chat on `host` that got no channel on the host's shared
/// connection waits rather than logging in on its own: a relay host's
/// every login is a dial its rate counts. A master of the user's own, which
/// the direct command rides without another login, carries it instead.
fn waits_for_shared(host: &str, rides_own_master: bool) -> bool {
    sheprd_client::relay_host::is_relay_target(host) && !rides_own_master
}

/// The writer thread: the greeting, then each queued command in order,
/// until the link is dropped or a write fails.
fn write(
    writer: impl Write,
    closer: &Closer,
    hello: ClientMessage,
    queued: &Receiver<ClientMessage>,
) {
    let mut writer = BufWriter::new(writer);
    if write_frame(&mut writer, &hello).is_ok() {
        while let Ok(message) = queued.recv() {
            if write_frame(&mut writer, &message).is_err() {
                break;
            }
        }
    }
    closer.close();
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_relay_hosts_chats_wait_for_its_shared_connection() {
        let relay = "penso@r-c83c5296e5b87858.sheprd.invalid";
        assert!(super::waits_for_shared(relay, false));
        // A master of the user's own carries it without another dial.
        assert!(!super::waits_for_shared(relay, true));
        assert!(!super::waits_for_shared("penso@studio.local", false));
    }
}
