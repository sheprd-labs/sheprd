//! A T3 Code thread, followed live, served on the attach wire, and steered.
//!
//! T3 Code runs the thread's agent; this program reads the thread's stream
//! (`sheprd_t3`) and shows it to Sheprd's chat views through the same
//! [`Mirror`] that serves existing CLI transcripts. A client's prompts,
//! answers, stops, and model changes go back to T3 as dispatched commands
//! ([`sheprd_t3::steer`]), and each is acknowledged once T3 has taken or
//! refused it. One worker thread owns the T3 connection: it hands the main
//! loop a fresh transcript after each chunk and sends the commands the loop
//! passes it. When the connection ends, the chat says why, keeps showing
//! what it had, and takes no more commands.

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    thread,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use crossbeam_channel::{Receiver, Sender, bounded, select};
use serde_json::json;
use sheprd_chat_protocol::{Command, CommandId, EndpointId, Rejection, Status, Transcript};
use sheprd_t3::{
    connection::Connection,
    discover,
    frame::{Exit, ServerFrame},
    http::Server,
    steer::{self, Provider},
    thread::Thread,
};

use crate::{
    cli::{T3Options, T3Thread},
    mirror::Mirror,
    session::{ConnId, Input},
};

/// How long the worker waits for a frame before sending waiting commands.
const POLL: Duration = Duration::from_millis(200);
/// How long a quiet connection waits before checking the server is there,
/// and then for its answer.
const QUIET: Duration = Duration::from_secs(30);
/// The most commands waiting for the worker; more are refused.
const MAX_ORDERS: usize = 32;
/// How many of T3's answers are kept, to give a client sending a command
/// again the answer it missed.
const MAX_ANSWERED: usize = 256;
/// The longest the worker holds the next command for the thread to show
/// the last one's effect, such as the run a prompt started.
const SETTLE: Duration = Duration::from_secs(2);

/// What one frame of the thread's stream does to the chat.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// T3 took a client's command, or refused it.
    Acked {
        conn: ConnId,
        id: CommandId,
        rejected: Option<Rejection>,
    },
    /// The thread changed; this is its transcript now.
    Changed(Box<Transcript>),
    /// Nothing the chat shows changed.
    Same,
    /// The stream ended, for this reason.
    Ended(String),
}

/// A client's command, on its way to T3.
#[derive(Debug)]
pub(crate) struct Order {
    pub command: Command,
    pub conn: ConnId,
    pub id: CommandId,
}

/// The thread a stream follows, what it last looked like, and the commands
/// sent to it.
///
/// Commands go to T3 one at a time, each judged against the thread as the
/// one before left it: a Stop sent right after a prompt waits for T3 to
/// answer the prompt and for the thread to reach the answer's sequence,
/// where the run it started shows, rather than being refused as having
/// nothing to stop.
pub(crate) struct Follower {
    /// T3's answers to commands, newest last, by chat command id.
    answered: VecDeque<(CommandId, Option<Rejection>)>,
    /// The sequence the thread reaches once it shows the last answered
    /// command, and when T3 answered it.
    awaited: Option<(u64, Instant)>,
    /// Dispatches waiting for their `Exit`, by RPC request id.
    dispatched: HashMap<String, (ConnId, CommandId)>,
    providers: Vec<Provider>,
    request: String,
    thread: Thread,
}

/// What becomes of a client's command.
#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    /// Send this to T3.
    Send(Box<steer::Dispatch>),
    /// Answer the client now: T3 already answered this command, or it
    /// cannot take it.
    Answer(Option<Rejection>),
}

impl Follower {
    pub(crate) fn new(request: String, providers: Vec<Provider>) -> Self {
        Self {
            answered: VecDeque::new(),
            awaited: None,
            dispatched: HashMap::new(),
            providers,
            request,
            thread: Thread::default(),
        }
    }

    /// What to do with `order`.
    pub(crate) fn verdict(&self, order: &Order) -> Verdict {
        // A client sending a command again, as after reconnecting, hears
        // what T3 said the first time, not what the thread allows now.
        if let Some((_, rejected)) = self.answered.iter().find(|(id, _)| *id == order.id) {
            return Verdict::Answer(*rejected);
        }
        if !self.ready() {
            return Verdict::Answer(Some(Rejection::NotReady));
        }
        match steer::dispatch(&self.thread, &self.providers, order.id, &order.command) {
            Ok(dispatch) => Verdict::Send(Box::new(dispatch)),
            Err(rejection) => Verdict::Answer(Some(rejection)),
        }
    }

    /// Whether the next command can be judged: none waits for T3, and the
    /// thread shows the last one's effect, or has had [`SETTLE`] to.
    pub(crate) fn settled(&self, now: Instant) -> bool {
        self.dispatched.is_empty()
            && self.awaited.is_none_or(|(sequence, answered)| {
                self.thread.sequence >= Some(sequence) || now.duration_since(answered) >= SETTLE
            })
    }

    /// Refuses every command T3 has not answered, for a connection that
    /// ended.
    pub(crate) fn abandon(&mut self) -> Vec<Step> {
        self.dispatched
            .drain()
            .map(|(_, (conn, id))| Step::Acked {
                conn,
                id,
                rejected: Some(Rejection::AgentUnavailable),
            })
            .collect()
    }

    /// Remembers that RPC `request` carries `order`, to acknowledge it when
    /// T3 answers.
    pub(crate) fn sent(&mut self, request: String, order: &Order) {
        self.dispatched.insert(request, (order.conn, order.id));
    }

    /// Whether the thread's snapshot has arrived.
    pub(crate) fn ready(&self) -> bool {
        self.thread.sequence.is_some()
    }

    pub(crate) fn frame(&mut self, frame: ServerFrame) -> Step {
        match frame {
            ServerFrame::Chunk { request_id, values } if request_id == self.request => {
                let before = self.thread.sequence;
                for value in values {
                    self.thread.apply(value);
                }
                if self.thread.sequence == before {
                    Step::Same
                } else {
                    Step::Changed(Box::new(self.transcript()))
                }
            }
            ServerFrame::Exit { request_id, exit } if self.dispatched.contains_key(&request_id) => {
                let Some((conn, id)) = self.dispatched.remove(&request_id) else {
                    return Step::Same;
                };
                let rejected = match exit {
                    Exit::Success { value } => {
                        self.awaited = value
                            .get("sequence")
                            .and_then(serde_json::Value::as_u64)
                            .map(|sequence| (sequence, Instant::now()));
                        None
                    }
                    failure => {
                        // T3's own words, for the pane; the client hears
                        // that the command did not reach the agent.
                        eprintln!(
                            "T3 Code refused a command: {}",
                            plain(&failure.cause().unwrap_or_default())
                        );
                        Some(Rejection::AgentUnavailable)
                    }
                };
                if self.answered.len() >= MAX_ANSWERED {
                    self.answered.pop_front();
                }
                self.answered.push_back((id, rejected));
                Step::Acked { conn, id, rejected }
            }
            ServerFrame::Exit { request_id, exit } if request_id == self.request => Step::Ended(
                exit.cause()
                    .unwrap_or_else(|| "T3 Code ended the thread's stream.".to_owned()),
            ),
            ServerFrame::Defect { defect } => Step::Ended(defect.to_string()),
            _ => Step::Same,
        }
    }

    /// The thread as a chat transcript, with its provider and model as
    /// settings.
    pub(crate) fn transcript(&self) -> Transcript {
        let agent = match &self.thread.header.provider_instance_id {
            Some(provider) => format!("{provider} in T3 Code"),
            None => "T3 Code".to_owned(),
        };
        let mut transcript = sheprd_t3::transcript::transcript(&self.thread, &agent);
        transcript.options = steer::options(&self.thread, &self.providers);
        transcript
    }
}

pub(crate) fn run(options: T3Options) -> anyhow::Result<()> {
    let origin = match &options.server {
        Some(server) => url::Url::parse(server)?,
        None => {
            let home = discover::t3_home(
                std::env::var_os("T3CODE_HOME").as_deref(),
                std::env::home_dir().as_deref(),
            )
            .context("no home directory to find T3 Code in")?;
            discover::local_origin(&home)?
        }
    };
    let server = Server::new(origin);
    let descriptor = server.describe()?;
    // Only the server that issued the token may receive it.
    let token = sheprd_t3::token::load(&options.token_file, &descriptor.environment_id)?;
    let ticket = server.ticket(&token)?;
    let mut connection = Connection::open(&server.socket(&ticket)?)?;
    // Without the server's providers the chat offers no model to change,
    // and the thread still shows and takes prompts.
    let providers = match connection.call("server.getConfig", json!({})) {
        Ok(config) => steer::providers(&config),
        Err(error) => {
            eprintln!(
                "T3 Code did not list its agents: {}",
                plain(&error.to_string())
            );
            Vec::new()
        }
    };
    let thread = match options.thread {
        T3Thread::Existing(thread) => thread,
        T3Thread::New(request) => {
            sheprd_t3::launch::start(&mut connection, &request, uuid::Uuid::new_v4())?
        }
    };
    let request = connection.request(
        "orchestration.subscribeThread",
        json!({ "threadId": thread }),
    )?;
    let mut follower = Follower::new(request, providers);
    while !follower.ready() {
        let frame = connection.next_frame()?;
        if let Step::Ended(reason) = follower.frame(frame) {
            anyhow::bail!("T3 Code refused the thread: {reason}");
        }
    }
    let mut shown = follower.transcript();
    let mut mirror = Mirror::new(shown.clone(), options.pane);

    let id = options.endpoint.unwrap_or_else(EndpointId::random);
    let address = sheprd_chat_protocol::endpoint_address(&id)
        .context("neither XDG_STATE_HOME nor HOME is set")?;
    let (inputs, receiver) = bounded(128);
    let _server = crate::attach::Server::listen(&address, inputs)?;
    // Only connection metadata goes to stdout, never thread content.
    println!("{id}");
    std::io::stdout().flush()?;

    // A short read timeout lets the worker send commands between frames;
    // a partly read frame waits in the socket for the next read.
    connection
        .stream()
        .set_read_timeout(Some(POLL))
        .context("could not set the T3 Code socket's timeout")?;
    // One transcript in flight: the worker waits while the loop takes the
    // last, which holds the server's next chunk back too.
    let (updates, mut changes) = bounded::<Step>(1);
    let (orders, queue) = bounded::<Order>(MAX_ORDERS);
    // Kept to refuse what the worker left unread when the connection ends;
    // the worker has stopped by then, so the two never compete for one.
    let leftover = queue.clone();
    thread::Builder::new()
        .name("t3-thread".to_owned())
        .spawn(move || follow(connection, follower, &updates, &queue, QUIET))?;
    let mut ended = false;
    loop {
        select! {
            recv(receiver) -> input => match input {
                Ok(Input::Shutdown) | Err(_) => break,
                Ok(Input::Command { command, conn, id }) => {
                    if ended || orders.try_send(Order { command, conn, id }).is_err() {
                        mirror.ack(conn, id, Some(Rejection::AgentUnavailable));
                    }
                }
                Ok(input) => mirror.input(input),
            },
            recv(changes) -> step => match step {
                Ok(Step::Acked { conn, id, rejected }) => mirror.ack(conn, id, rejected),
                Ok(Step::Changed(transcript)) => {
                    shown = *transcript;
                    mirror.update(shown.clone());
                }
                Ok(Step::Ended(reason)) => {
                    ended = true;
                    for order in leftover.try_iter() {
                        mirror.ack(order.conn, order.id, Some(Rejection::AgentUnavailable));
                    }
                    shown.status = Status::Failed {
                        message: format!("The T3 Code connection ended: {reason}"),
                    };
                    // Nothing reaches T3 any more.
                    shown.permissions.clear();
                    shown.read_only = true;
                    mirror.update(shown.clone());
                }
                Ok(Step::Same) => {}
                // The worker is done; a disconnected channel is always
                // ready, so stop selecting on it.
                Err(_) => changes = crossbeam_channel::never(),
            },
        }
    }
    Ok(())
}

/// `text` from the server, safe for the pane's terminal: control
/// characters, which could start an escape sequence, become `\u{fffd}`.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

/// Reads the stream until it ends, passing each change on, and sends the
/// commands waiting in `orders` between reads.
///
/// After `quiet` with nothing heard it sends one ping; `quiet` more with
/// that ping unanswered means the server is gone even if its socket stays
/// open.
fn follow<S: Read + Write>(
    mut connection: Connection<S>,
    mut follower: Follower,
    updates: &Sender<Step>,
    orders: &Receiver<Order>,
    quiet: Duration,
) {
    let mut heard = Instant::now();
    let mut pinged = None;
    loop {
        while follower.settled(Instant::now()) {
            let Ok(order) = orders.try_recv() else {
                break;
            };
            let rejected = match follower.verdict(&order) {
                Verdict::Send(dispatch) => {
                    let sent = serde_json::to_value(&dispatch)
                        .map_err(sheprd_t3::Error::from)
                        .and_then(|payload| connection.request(steer::DISPATCH, payload));
                    match sent {
                        Ok(request) => {
                            follower.sent(request, &order);
                            continue;
                        }
                        Err(_) => Some(Rejection::AgentUnavailable),
                    }
                }
                Verdict::Answer(rejected) => rejected,
            };
            let step = Step::Acked {
                conn: order.conn,
                id: order.id,
                rejected,
            };
            if updates.send(step).is_err() {
                return;
            }
        }
        let step = match connection.next_frame() {
            Ok(frame) => {
                heard = Instant::now();
                pinged = None;
                follower.frame(frame)
            }
            Err(sheprd_t3::Error::Idle) => match pinged {
                Some(at) if Instant::now().duration_since(at) >= quiet => {
                    Step::Ended("T3 Code stopped answering.".to_owned())
                }
                Some(_) => Step::Same,
                // A quiet thread: check the server is still there.
                None if Instant::now().duration_since(heard) >= quiet => match connection.ping() {
                    Ok(()) => {
                        pinged = Some(Instant::now());
                        Step::Same
                    }
                    Err(error) => Step::Ended(error.to_string()),
                },
                None => Step::Same,
            },
            Err(error) => Step::Ended(error.to_string()),
        };
        if let Step::Ended(_) = step {
            // Commands T3 never answered are answered here, before the end.
            for refused in follower.abandon() {
                if updates.send(refused).is_err() {
                    return;
                }
            }
            let _ = updates.send(step);
            return;
        }
        if !matches!(step, Step::Same) && updates.send(step).is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
