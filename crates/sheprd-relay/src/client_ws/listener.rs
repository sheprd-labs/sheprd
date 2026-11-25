//! A listener that stays on its route: it reconnects with capped backoff,
//! names its dial keys and pairing keys again after each reconnect, and
//! accepts each dial on a socket of its own.

use super::{
    Config, Error, Result, Session,
    connect::{self, Socket},
};
use crate::{
    Endpoint, RouteId, SessionId,
    socket::{FromListener, Id, ToPeer, listen_signed},
    wire::{Key, MAX_DIAL_KEYS, MAX_PAIRING_SECS, Status},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use p256::ecdsa::SigningKey;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::{
    Message,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

/// How many accepted sessions wait for [`Listener::accept`] at once.
const WAITING: usize = 16;

/// Where a listener stands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ListenerStatus {
    /// Opening the socket and proving the key, or waiting to try again.
    Connecting,
    /// On its route: dials reach it.
    Listening,
    /// The last try failed; it tries again after a backoff.
    Unreachable(Unreachable),
    /// Another listener proved the same key and took the route (4409). It
    /// does not try again: two listeners would take turns forever.
    Replaced,
    /// It was closed, or every handle to it dropped.
    Stopped,
}

/// Why a listener is not on its route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unreachable {
    /// The relay banned this address.
    Banned,
    /// The relay closed the socket with this code (`socket::code`), or
    /// without one.
    Closed(Option<u16>),
    /// The relay answered HTTP with this status instead of a socket.
    Http(u16),
    /// The relay could not be reached: DNS, TCP, TLS, or a timeout.
    Network,
    /// The relay said something that is not the protocol.
    Protocol,
}

impl From<&Error> for Unreachable {
    fn from(error: &Error) -> Self {
        match error {
            Error::Banned => Self::Banned,
            Error::Closed { code } => Self::Closed(*code),
            Error::Http { status } => Self::Http(*status),
            Error::Malformed | Error::WrongRoute | Error::Refused(_) => Self::Protocol,
            _ => Self::Network,
        }
    }
}

/// How long a listener waits before trying again: `first`, doubling up to
/// `max`, with up to half of it again at random so listeners a relay
/// restart dropped do not all return at once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Backoff {
    pub first: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            first: Duration::from_millis(500),
            max: Duration::from_secs(30),
        }
    }
}

/// What a listener needs.
#[derive(Clone, Debug)]
pub struct ListenerConfig {
    pub backoff: Backoff,
    pub endpoint: Endpoint,
    /// The route's key, which the listener proves it holds.
    pub key: SigningKey,
    /// Sessions accepted, and the control socket's handshake.
    pub session: Config,
    /// The relay's listen token, when it asks for one.
    pub token: Option<String>,
}

impl ListenerConfig {
    /// Listening on `endpoint` as `key`, with defaults for the rest.
    #[must_use]
    pub fn new(endpoint: Endpoint, key: SigningKey) -> Self {
        Self {
            backoff: Backoff::default(),
            endpoint,
            key,
            session: Config::default(),
            token: None,
        }
    }
}

/// What became of a change to who may dial.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Applied {
    /// The relay answered it.
    Answered(Status),
    /// The listener is not on its route now; the change is kept and sent
    /// when it is.
    Queued,
}

/// A dial the listener accepted: the dialer's key, and the session.
#[derive(Debug)]
pub struct Incoming {
    pub key: Key,
    pub session: Session,
}

enum Command {
    DialKeys(Vec<Key>, oneshot::Sender<Applied>),
    PairingKey(Key, u16, oneshot::Sender<Applied>),
    Close(oneshot::Sender<()>),
}

/// A listener on its route, kept there by a task of its own until it is
/// closed, replaced, or dropped.
#[derive(Debug)]
pub struct Listener {
    commands: mpsc::Sender<Command>,
    incoming: mpsc::Receiver<Result<Incoming>>,
    route: RouteId,
    status: watch::Receiver<ListenerStatus>,
    task: JoinHandle<()>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Command")
    }
}

impl Listener {
    /// Starts listening. Call from within a Tokio runtime.
    #[must_use]
    pub fn spawn(config: ListenerConfig) -> Self {
        let route = RouteId::of(config.key.verifying_key());
        let (commands, commands_rx) = mpsc::channel(16);
        let (incoming_tx, incoming) = mpsc::channel(WAITING);
        let (status_tx, status) = watch::channel(ListenerStatus::Connecting);
        let task = tokio::spawn(run(Task {
            commands: commands_rx,
            config,
            dial_keys: None,
            incoming: incoming_tx,
            next_id: 0,
            pairing: Vec::new(),
            route,
            status: status_tx,
        }));
        Self {
            commands,
            incoming,
            route,
            status,
            task,
        }
    }

    /// The route dialers reach this listener on.
    #[must_use]
    pub const fn route(&self) -> RouteId {
        self.route
    }

    /// Where it stands now, and every change after.
    #[must_use]
    pub fn status(&self) -> watch::Receiver<ListenerStatus> {
        self.status.clone()
    }

    /// Allows exactly `keys` to dial, replacing the previous set; the
    /// relay ends sessions of keys left out. Kept and sent again after
    /// every reconnect.
    pub async fn set_dial_keys(&self, keys: Vec<Key>) -> Result<Applied> {
        if keys.len() > MAX_DIAL_KEYS {
            return Err(Error::TooManyKeys {
                count: keys.len(),
                max: MAX_DIAL_KEYS,
            });
        }
        self.ask(|reply| Command::DialKeys(keys, reply)).await
    }

    /// Allows `key` one dial within `seconds`. After a reconnect it is
    /// named again with the seconds it has left, until it is used.
    pub async fn add_pairing_key(&self, key: Key, seconds: u16) -> Result<Applied> {
        if seconds == 0 || seconds > MAX_PAIRING_SECS {
            return Err(Error::Lifetime {
                max: MAX_PAIRING_SECS,
                seconds,
            });
        }
        self.ask(|reply| Command::PairingKey(key, seconds, reply))
            .await
    }

    async fn ask(
        &self,
        command: impl FnOnce(oneshot::Sender<Applied>) -> Command,
    ) -> Result<Applied> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(command(reply))
            .await
            .map_err(|_| Error::Stopped)?;
        answer.await.map_err(|_| Error::Stopped)
    }

    /// The next accepted dial, an accept that failed, or `None` once the
    /// listener has stopped.
    pub async fn accept(&mut self) -> Option<Result<Incoming>> {
        self.incoming.recv().await
    }

    /// Leaves the route, closing the control socket cleanly.
    pub async fn close(self) {
        let (done, closed) = oneshot::channel();
        if self.commands.send(Command::Close(done)).await.is_ok() {
            let _ = closed.await;
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Task {
    commands: mpsc::Receiver<Command>,
    config: ListenerConfig,
    /// The keys to name after each reconnect, once any were set.
    dial_keys: Option<Vec<Key>>,
    incoming: mpsc::Sender<Result<Incoming>>,
    next_id: u64,
    /// Pairing keys not used yet, and when each expires.
    pairing: Vec<(Key, Instant)>,
    route: RouteId,
    status: watch::Sender<ListenerStatus>,
}

/// How one connection ended.
enum Ended {
    /// Try again after a backoff.
    Lost(Unreachable),
    Replaced,
    Stopped,
}

async fn run(mut task: Task) {
    let mut wait = task.config.backoff.first;
    loop {
        task.status.send_replace(ListenerStatus::Connecting);
        let ended = match task.connect().await {
            Ok(Some(socket)) => {
                wait = task.config.backoff.first;
                task.serve(socket).await
            }
            Ok(None) => Ended::Stopped,
            Err(Error::Replaced) => Ended::Replaced,
            Err(error) => Ended::Lost(Unreachable::from(&error)),
        };
        match ended {
            Ended::Lost(reason) => {
                task.status
                    .send_replace(ListenerStatus::Unreachable(reason));
                if !task.pause(jittered(wait)).await {
                    break;
                }
                wait = (wait * 2).min(task.config.backoff.max);
            }
            Ended::Replaced => {
                task.status.send_replace(ListenerStatus::Replaced);
                return;
            }
            Ended::Stopped => break,
        }
    }
    task.status.send_replace(ListenerStatus::Stopped);
}

/// `wait` plus up to half of it again.
fn jittered(wait: Duration) -> Duration {
    let mut random = [0; 2];
    let fraction = match getrandom::fill(&mut random) {
        Ok(()) => f64::from(u16::from_be_bytes(random)) / f64::from(u16::MAX),
        Err(_) => 0.5,
    };
    wait + wait.mul_f64(fraction / 2.0)
}

impl Task {
    /// Opens the control socket and proves the key, answering commands as
    /// queued meanwhile. `None` when told to stop.
    async fn connect(&mut self) -> Result<Option<Socket>> {
        // Its own copy, so commands can change what is kept meanwhile.
        let config = self.config.clone();
        let opening = open(&config, self.route);
        tokio::pin!(opening);
        loop {
            tokio::select! {
                socket = &mut opening => return socket.map(Some),
                command = self.commands.recv() => {
                    if !self.queue(command) {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Waits `wait`, answering commands as queued. False when told to stop.
    async fn pause(&mut self, wait: Duration) -> bool {
        let sleep = tokio::time::sleep(wait);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => return true,
                command = self.commands.recv() => {
                    if !self.queue(command) {
                        return false;
                    }
                }
            }
        }
    }

    /// Keeps a command for the next connection. False when it says stop.
    fn queue(&mut self, command: Option<Command>) -> bool {
        match command {
            Some(Command::DialKeys(keys, reply)) => {
                self.dial_keys = Some(keys);
                let _ = reply.send(Applied::Queued);
            }
            Some(Command::PairingKey(key, seconds, reply)) => {
                self.remember_pairing(key, seconds);
                let _ = reply.send(Applied::Queued);
            }
            Some(Command::Close(done)) => {
                let _ = done.send(());
                return false;
            }
            None => return false,
        }
        true
    }

    fn remember_pairing(&mut self, key: Key, seconds: u16) {
        let expires = Instant::now() + Duration::from_secs(u64::from(seconds));
        self.pairing.retain(|(kept, _)| *kept != key);
        self.pairing.push((key, expires));
    }

    fn id(&mut self) -> Id {
        self.next_id += 1;
        Id::Number(self.next_id)
    }

    /// Runs one connection until it ends.
    async fn serve(&mut self, mut socket: Socket) -> Ended {
        // Answers the relay owes, oldest first, with who waits on each.
        let mut owed: VecDeque<(Id, Option<oneshot::Sender<Applied>>)> = VecDeque::new();
        if let Err(error) = self.name_again(&mut socket, &mut owed).await {
            return Ended::Lost(Unreachable::from(&error));
        }
        self.status.send_replace(ListenerStatus::Listening);
        let ended = loop {
            tokio::select! {
                frame = socket.next() => match frame {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(message) = serde_json::from_str::<ToPeer>(&text) else {
                            break Ended::Lost(Unreachable::Protocol);
                        };
                        if let Err(reason) = self.heard(message, &mut owed) {
                            break Ended::Lost(reason);
                        }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Ok(Message::Binary(_))) => break Ended::Lost(Unreachable::Protocol),
                    Some(Ok(Message::Close(frame))) => {
                        let code = frame.map(|frame| u16::from(frame.code));
                        break match connect::closed(code) {
                            Error::Replaced => Ended::Replaced,
                            error => Ended::Lost(Unreachable::from(&error)),
                        };
                    }
                    Some(Err(_)) | None => break Ended::Lost(Unreachable::Network),
                },
                command = self.commands.recv() => match command {
                    Some(Command::Close(done)) => {
                        let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                        let _ = socket.send(Message::Close(Some(frame))).await;
                        let _ = done.send(());
                        break Ended::Stopped;
                    }
                    None => break Ended::Stopped,
                    Some(command) => {
                        if let Err(error) = self.send_command(&mut socket, command, &mut owed).await {
                            break Ended::Lost(Unreachable::from(&error));
                        }
                    }
                },
            }
        };
        // What the relay never answered is kept and sent on the next
        // connection.
        for (_, reply) in owed {
            if let Some(reply) = reply {
                let _ = reply.send(Applied::Queued);
            }
        }
        ended
    }

    /// Names the dial keys and unused pairing keys again on a new
    /// connection; pairing keys with the seconds they have left.
    async fn name_again(
        &mut self,
        socket: &mut Socket,
        owed: &mut VecDeque<(Id, Option<oneshot::Sender<Applied>>)>,
    ) -> Result<()> {
        if let Some(keys) = self.dial_keys.clone() {
            let id = self.id();
            connect::send(socket, &dial_keys(&keys, id.clone())).await?;
            owed.push_back((id, None));
        }
        let now = Instant::now();
        self.pairing.retain(|(_, expires)| *expires > now);
        for (key, expires) in self.pairing.clone() {
            let left = expires.saturating_duration_since(now);
            let left = left.as_secs() + u64::from(left.subsec_nanos() > 0);
            // At most the lifetime it was given, itself at most the cap.
            let seconds = u16::try_from(left)
                .unwrap_or(MAX_PAIRING_SECS)
                .clamp(1, MAX_PAIRING_SECS);
            let id = self.id();
            connect::send(socket, &pairing_key(&key, seconds, id.clone())).await?;
            owed.push_back((id, None));
        }
        Ok(())
    }

    async fn send_command(
        &mut self,
        socket: &mut Socket,
        command: Command,
        owed: &mut VecDeque<(Id, Option<oneshot::Sender<Applied>>)>,
    ) -> Result<()> {
        // What is kept changes before anything is sent: if the send fails,
        // the next connection names the new set, never the old one, so a
        // key just revoked does not get back in.
        let id = self.id();
        let (message, reply) = match command {
            Command::DialKeys(keys, reply) => {
                let message = dial_keys(&keys, id.clone());
                self.dial_keys = Some(keys);
                (message, reply)
            }
            Command::PairingKey(key, seconds, reply) => {
                self.remember_pairing(key, seconds);
                (pairing_key(&key, seconds, id.clone()), reply)
            }
            Command::Close(done) => {
                let _ = done.send(());
                return Ok(());
            }
        };
        if let Err(error) = connect::send(socket, &message).await {
            let _ = reply.send(Applied::Queued);
            return Err(error);
        }
        owed.push_back((id, Some(reply)));
        Ok(())
    }

    /// Handles one message from the relay.
    fn heard(
        &mut self,
        message: ToPeer,
        owed: &mut VecDeque<(Id, Option<oneshot::Sender<Applied>>)>,
    ) -> Result<(), Unreachable> {
        match message {
            ToPeer::Status { id, status } => {
                // A relay that echoes ids answers this one; one older than
                // ids answers in order.
                let at = match &id {
                    Some(id) => owed.iter().position(|(owed, _)| owed == id),
                    None => (!owed.is_empty()).then_some(0),
                };
                let Some((_, reply)) = at.and_then(|at| owed.remove(at)) else {
                    return Err(Unreachable::Protocol);
                };
                if let Some(reply) = reply {
                    let _ = reply.send(Applied::Answered(status));
                }
                Ok(())
            }
            ToPeer::Incoming { key, session } => {
                let key: Key = URL_SAFE_NO_PAD
                    .decode(key)
                    .ok()
                    .and_then(|key| key.try_into().ok())
                    .ok_or(Unreachable::Protocol)?;
                let session = SessionId::parse(&session).ok_or(Unreachable::Protocol)?;
                // The relay lets a pairing key through once.
                self.pairing.retain(|(kept, _)| *kept != key);
                let url = self.config.endpoint.accept(&session);
                let config = self.config.session.clone();
                let incoming = self.incoming.clone();
                tokio::spawn(async move {
                    let accepted = connect::open(&url, &config).await.map(|socket| Incoming {
                        key,
                        session: Session::new(socket, &config),
                    });
                    let _ = incoming.send(accepted).await;
                });
                Ok(())
            }
            ToPeer::Challenge { .. } | ToPeer::Ready { .. } => Err(Unreachable::Protocol),
        }
    }
}

/// Opens the control socket, proves the key, and checks the route the
/// relay names is this key's.
async fn open(config: &ListenerConfig, route: RouteId) -> Result<Socket> {
    let mut socket = connect::open(&config.endpoint.listen(), &config.session).await?;
    let name = config.endpoint.proof_name();
    let proved = async {
        connect::prove(&mut socket, &config.key, config.token.as_deref(), |nonce| {
            listen_signed(&name, nonce)
        })
        .await?;
        match connect::control(&mut socket).await? {
            ToPeer::Ready { route: named } if named == route.to_string() => Ok(()),
            ToPeer::Ready { .. } => Err(Error::WrongRoute),
            _ => Err(Error::Malformed),
        }
    };
    tokio::time::timeout(config.session.handshake_timeout, proved)
        .await
        .map_err(|_| Error::Timeout)??;
    Ok(socket)
}

fn dial_keys(keys: &[Key], id: Id) -> FromListener {
    FromListener::DialKeys {
        id: Some(id),
        keys: keys.iter().map(|key| URL_SAFE_NO_PAD.encode(key)).collect(),
    }
}

fn pairing_key(key: &Key, seconds: u16, id: Id) -> FromListener {
    FromListener::PairingKey {
        id: Some(id),
        key: URL_SAFE_NO_PAD.encode(key),
        seconds,
    }
}
