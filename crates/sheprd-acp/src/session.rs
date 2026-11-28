//! The conversation's single owner: it talks to the agent, keeps the
//! transcript, and fans every change out to the attached clients.
//!
//! Everything runs on one thread fed by one bounded input queue, so changes
//! have one order and a client's catch-up can never interleave with a change.

mod agent;
mod briefs;
mod carry;
mod checkpoint;
mod commands;
mod guests;
mod harness;
mod keep;
mod known;
mod picks;
mod pictures;
mod pull_requests;
mod steer;
mod thread;
mod watchdog;

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use acp::v1 as wire;
use crossbeam_channel::{Sender, TrySendError};
use sheprd_chat_protocol::{
    Agent, ByeReason, Change, Command, CommandId, Cursor, Harnesses, Origin, PermissionId, Queued,
    Rejection, ServerMessage, Status, Transcript, VERSION,
};
use uuid::Uuid;

use crate::{context::Context, project::Projector, rpc};

pub(crate) use keep::Start as ThreadStart;

/// Changes kept for clients that reconnect with a cursor.
const LOG: usize = 8192;
/// Bytes of logged changes kept, so a run of pictures cannot pin the log's
/// memory; a client older than the log gets a snapshot.
const LOG_BYTES: usize = 32 * 1024 * 1024;
/// The most logged changes replayed to a reconnecting client. One further
/// behind, as a window left in the background is, gets a snapshot instead:
/// it shows the transcript as it is now in one step, rather than scrolling
/// through every chunk that streamed while it was away.
const CATCH_UP: u64 = 256;
/// Commands remembered so a resent one is not run twice.
const HANDLED: usize = 256;
/// The most entries a transcript keeps.
pub(crate) const MAX_ENTRIES: usize = 2000;
/// The most text a transcript keeps, so a snapshot always fits in one frame.
pub(crate) const MAX_TRANSCRIPT_TEXT: usize = 2 * 1024 * 1024;
/// The most image data a transcript keeps, apart from its text, so a
/// snapshot with both still fits in one frame.
pub(crate) const MAX_TRANSCRIPT_MEDIA: usize = 8 * 1024 * 1024;

/// A connection to the session: the in-pane view or an attached client.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConnId(pub u64);

/// One started agent process. A chat that switches agents starts another,
/// and what the one before says from then on is ignored.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct HarnessId(pub u64);

#[derive(Debug)]
pub(crate) enum Input {
    Agent(HarnessId, rpc::Inbound),
    /// The agent process ended; carries its last standard error lines.
    AgentExited {
        code: Option<i32>,
        harness: HarnessId,
        stderr: Vec<String>,
    },
    Attach(Attach),
    Command {
        command: Command,
        conn: ConnId,
        id: CommandId,
    },
    Detach(ConnId),
    Shutdown,
    /// A command this version does not know, from a newer client: refused
    /// with an acknowledgement rather than by dropping the connection.
    Unsupported {
        conn: ConnId,
        id: CommandId,
    },
}

#[derive(Debug)]
pub(crate) struct Attach {
    pub conn: ConnId,
    /// The program's own view, which does not count as an attached client.
    pub local: bool,
    pub outbox: Sender<ServerMessage>,
    pub since: Option<Cursor>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Flow {
    Continue,
    Exit,
    /// Start `agent` as `harness` and hand the session its outbox with
    /// [`Session::launched`], or say why it did not start with
    /// [`Session::launch_failed`]. The agent before it is no longer heard.
    Launch {
        agent: Agent,
        harness: HarnessId,
        /// The agent's own earlier session, which it continues.
        load: Option<String>,
    },
}

/// What Herdr is told about the pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Activity {
    Idle,
    Working,
    Blocked,
}

/// What the session passes on for Herdr.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Update {
    Activity(Activity),
    /// The chat switched to another built-in agent, which starts next.
    Harness(Agent),
    /// The agent's session opened with this id, new or continued.
    Opened(String),
    /// The agent a switch asked for started as this process.
    Started(u32),
    /// A prompt just sent to the agent, as the transcript shows it, which
    /// names the pane and its tab.
    Prompt(String),
    /// A new session of this built-in agent offered these settings, which
    /// differ from those known for it: kept for its next start, or, when
    /// empty, what was known forgotten.
    Settings(Agent, crate::known::Settings),
}

pub(crate) struct Config {
    pub antigravity_auth_method: Option<String>,
    pub agent: String,
    /// The agents a client may switch to while the agent is idle, as
    /// `--choose` asks; `None` keeps the agent the chat started with.
    pub choices: Option<Vec<Agent>>,
    pub cwd: PathBuf,
    /// Exit when the agent does, rather than leave its last words on screen.
    pub exit_with_agent: bool,
    /// The ACP auth method Gemini tries when `session/new` asks for sign-in.
    pub gemini_auth_method: Option<String>,
    /// How the agent running gets the server that reads the chat's thread.
    pub given: crate::thread_mcp::Given,
    /// The built-in agent running, or `None` for a custom command.
    pub harness: Option<Agent>,
    /// The conversation so far, for a continued session whose agent does
    /// not replay it.
    pub history: Option<Transcript>,
    /// The settings each built-in agent offered last, shown while it starts.
    pub known: crate::known::Known,
    /// Clients can attach, and the agent was told where: its
    /// `sheprd chat pr add` reaches this chat.
    pub listens: bool,
    pub load: Option<String>,
    pub login_hint: Option<String>,
    pub pane: Option<String>,
    /// The note templates and the user's instructions, sent ahead of the
    /// first prompt and whenever the person switches devices.
    pub prompts: crate::context::Prompts,
    /// Fail rather than start a new session when the agent cannot continue
    /// `load`.
    pub require_load: bool,
    /// What the chat that asked for this one ran with, applied once the
    /// agent's new session opens; never to a continued one.
    pub settings: Option<sheprd_chat_protocol::Settings>,
    /// The `sheprd` command the first note tells the agent to keep the
    /// repository's memory with; `None` leaves memory out of it.
    pub sheprd: Option<PathBuf>,
    /// The thread the chat keeps; `None` keeps none, as for a custom
    /// command, which no segment can name.
    pub thread: Option<keep::Start>,
}

#[derive(Debug)]
enum Pending {
    Authenticate {
        load_session: bool,
    },
    Initialize,
    NewSession,
    LoadSession,
    Prompt,
    SetMode(String),
    SetModel(String),
    SetOption,
    /// A prompt steered into the running turn.
    Steer(CommandId),
}

struct Subscriber {
    conn: ConnId,
    local: bool,
    outbox: Sender<ServerMessage>,
}

struct Asked {
    options: Vec<String>,
    request: wire::RequestId,
}

pub(crate) struct Session<R: FnMut(Update)> {
    /// What the agent takes in a prompt besides text, from its handshake.
    accepts: crate::files::Accepts,
    acp_session: Option<wire::SessionId>,
    activity: Option<Activity>,
    asked: HashMap<PermissionId, Asked>,
    /// Whether this harness already sent ACP `authenticate`.
    auth_attempted: bool,
    /// ACP authentication methods advertised in `initialize`.
    auth_methods: Vec<wire::AuthMethod>,
    briefs: briefs::Briefs,
    /// Whether this harness advertised `loadSession` in `initialize`.
    can_load_session: bool,
    /// Whether this harness takes prompts into a running turn.
    can_steer: bool,
    /// The whole commands of shell calls, which their titles cut short.
    commands: pull_requests::Commands,
    checkpoint: Option<checkpoint::Checkpoint>,
    config: Config,
    /// What the agent has been told besides the person's words.
    context: Context,
    /// Settings picked while the agent started, sent once its session opens.
    deferred: Vec<known::Deferred>,
    /// The notice under the conversation this chat continued, decided when
    /// its first session opens and kept for the chat's life, whichever
    /// agent it switches to; `None` until then.
    divider: Option<Option<sheprd_chat_protocol::EntryId>>,
    epoch: Uuid,
    /// Guest prompts waiting for the person, and the connections relaying them.
    guests: guests::Guests,
    handled: VecDeque<(CommandId, Option<Rejection>)>,
    /// The agent process heard now.
    harness: HarnessId,
    /// The thread kept on disk, while it can be.
    keep: Option<keep::Keep>,
    /// Why the thread stopped being kept, until the chat says so.
    keep_failure: Option<String>,
    /// An agent to start once the input that chose it is handled.
    launch: Option<Agent>,
    /// Whether the `model` config option came from legacy `models` and uses
    /// `session/set_model` rather than `session/set_config_option`.
    legacy_model_option: bool,
    log: VecDeque<(u64, Change)>,
    /// What the logged changes weigh, by [`change_size`].
    log_bytes: usize,
    next_permission: u64,
    next_request: i64,
    outbox: rpc::Outbox,
    pending: HashMap<i64, Pending>,
    picks: picks::Picks,
    pictures: pictures::Pictures,
    projector: Projector,
    /// Where each queued prompt was typed, by its command; the transcript's
    /// queue does not carry it, as no client needs it.
    queued_origins: HashMap<CommandId, Origin>,
    ready: bool,
    /// The note a replayed message from the person held and the entry
    /// for their words that comes next takes.
    replayed_note: Option<String>,
    report: R,
    seq: u64,
    /// Prompts steered into the running turn, as sent, until the agent
    /// takes them or they go as the next prompt.
    steering: HashMap<CommandId, commands::Outgoing>,
    subscribers: Vec<Subscriber>,
    thread: thread::Thread,
    transcript: Transcript,
    turn: Option<i64>,
    /// The pull requests the running turn created or pushed to.
    turn_pulls: pull_requests::TurnPulls,
    watchdog: watchdog::Watchdog,
}

impl<R: FnMut(Update)> Session<R> {
    pub(crate) fn new(mut config: Config, outbox: rpc::Outbox, report: R) -> Self {
        let mut transcript = Transcript::new(config.agent.clone());
        // The saved conversation keeps Sheprd's notes; the chat shows the
        // person's words only, and the notes say what not to tell again.
        let mut told = crate::context::Told::default();
        if let Some(mut history) = config.history.take() {
            told = crate::context::hide(&mut history);
            for entry in &mut history.entries {
                if let sheprd_chat_protocol::EntryKind::User { text, files: 0, .. } =
                    &mut entry.kind
                {
                    *text = crate::project::shown(text).to_owned();
                }
            }
            transcript.entries = history.entries;
            transcript.truncated = history.truncated;
            transcript.usage = history.usage;
        }
        transcript.harnesses = config.choices.clone().map(|available| Harnesses {
            available,
            current: config.harness,
        });
        known::seed(&config.known, config.harness, &mut transcript);
        let thread = thread::Thread::new(config.harness, &transcript.entries);
        let opened = config
            .thread
            .take()
            .zip(config.harness)
            .map(|(start, agent)| {
                keep::Keep::open(
                    start,
                    agent,
                    config.load.as_deref(),
                    &mut transcript.entries,
                    config.cwd.clone(),
                )
            });
        let mut last = transcript.last_entry_id();
        let keep = opened.map(|opened| {
            transcript.thread = Some(opened.thread);
            // The thread's file goes on from its newest entry, so ids never
            // repeat in it.
            last = last.max(opened.last);
            opened.keep
        });
        let projector = last.map_or_else(Projector::default, Projector::after);
        let facts = crate::context::Facts {
            agent: config.agent.clone(),
            cwd: config.cwd.display().to_string(),
        };
        // Memory opens here, on the session's thread, which also runs the
        // prompts that recall from it. A fixed Antigravity chat never sends
        // Sheprd notes, so it skips opening memory; a `--choose` chat keeps
        // context ready in case it switches to another agent, while bypassing
        // notes whenever Antigravity is active.
        let direct = config.harness == Some(Agent::Antigravity);
        let may_annotate = !direct || config.choices.is_some();
        let recall = if may_annotate {
            config
                .sheprd
                .as_ref()
                .and_then(|_| crate::context::Recall::open(&config.cwd))
        } else {
            None
        };
        let mut context = Context::new(std::mem::take(&mut config.prompts), facts, told)
            .with_memory(config.sheprd.as_deref(), recall)
            .with_pull_requests(config.sheprd.as_deref().filter(|_| config.listens))
            .with_launch(
                config
                    .sheprd
                    .as_deref()
                    .filter(|_| config.harness.is_some()),
            );
        context.set_direct(direct);
        Self {
            accepts: crate::files::Accepts::default(),
            acp_session: None,
            checkpoint: None,
            activity: None,
            asked: HashMap::new(),
            auth_attempted: false,
            auth_methods: Vec::new(),
            briefs: briefs::Briefs::default(),
            can_load_session: false,
            can_steer: false,
            commands: pull_requests::Commands::default(),
            config,
            context,
            deferred: Vec::new(),
            divider: None,
            epoch: Uuid::new_v4(),
            guests: guests::Guests::default(),
            handled: VecDeque::new(),
            harness: HarnessId::default(),
            keep,
            keep_failure: None,
            launch: None,
            legacy_model_option: false,
            log: VecDeque::new(),
            log_bytes: 0,
            next_permission: 0,
            next_request: 0,
            outbox,
            pending: HashMap::new(),
            picks: picks::Picks::default(),
            pictures: pictures::Pictures::default(),
            projector,
            queued_origins: HashMap::new(),
            ready: false,
            replayed_note: None,
            report,
            seq: 0,
            steering: HashMap::new(),
            subscribers: Vec::new(),
            thread,
            transcript,
            turn: None,
            turn_pulls: pull_requests::TurnPulls::default(),
            watchdog: watchdog::Watchdog::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// Says something about the session in its transcript.
    pub(crate) fn notice(&mut self, level: sheprd_chat_protocol::NoticeLevel, text: String) {
        let notice = self.projector.notice(level, text);
        self.emit(notice);
    }

    /// Opens the ACP connection.
    pub(crate) fn start(&mut self) {
        let mut request = wire::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
            wire::Implementation::new("sheprd", env!("CARGO_PKG_VERSION")),
        );
        request.client_capabilities.meta = Some(wire::Meta::from_iter([(
            crate::project::opencode::CAPABILITY.to_owned(),
            serde_json::Value::Bool(true),
        )]));
        self.request("initialize", &request, Pending::Initialize);
        self.report_activity();
    }

    pub(crate) fn handle(&mut self, input: Input) -> Flow {
        let turning = self.turn.is_some();
        match input {
            Input::Agent(harness, inbound) if harness == self.harness => self.agent(inbound),
            Input::AgentExited {
                code,
                harness,
                stderr,
            } if harness == self.harness => {
                self.exited(code, &stderr);
                if self.config.exit_with_agent {
                    self.keep_entries();
                    self.bye(ByeReason::Exiting);
                    return Flow::Exit;
                }
            }
            // An agent the chat switched away from.
            Input::Agent(..) | Input::AgentExited { .. } => {}
            Input::Attach(attach) => self.attach(attach),
            Input::Command { command, conn, id } => self.command(conn, id, command),
            Input::Detach(conn) => self.detach(conn),
            Input::Unsupported { conn, id } => {
                self.ack(conn, id, Some(Rejection::Unsupported));
            }
            Input::Shutdown => {
                self.save_history(true);
                self.keep_entries();
                self.bye(ByeReason::Exiting);
                return Flow::Exit;
            }
        }
        self.save_history(turning != self.turn.is_some());
        match self.launch.take() {
            Some(agent) => Flow::Launch {
                agent,
                harness: self.harness,
                load: self.config.load.clone(),
            },
            None => Flow::Continue,
        }
    }

    /// The next time the session thread should call [`Self::tick`], if any.
    pub(crate) fn next_tick(&self) -> Option<std::time::Instant> {
        self.watchdog
            .next_deadline(self.antigravity_quiet_after_reply())
    }

    /// Runs periodic session checks (such as the Antigravity silent-turn
    /// watchdog) as of `now`.
    pub(crate) fn tick(&mut self, now: std::time::Instant) {
        let eligible = self.antigravity_quiet_after_reply();
        match self.watchdog.poll(now, eligible) {
            Some(watchdog::Action::Warn(after)) => {
                let after = watchdog::describe_duration(after);
                let notice = self.projector.notice(
                    sheprd_chat_protocol::NoticeLevel::Info,
                    format!(
                        "{} has been quiet for {after} since its last reply; a background task \
                         may be holding this turn open. Press Esc or Stop to cancel it, or wait \
                         for it to finish.",
                        self.config.agent
                    ),
                );
                self.emit(notice);
            }
            Some(watchdog::Action::Cancel(after)) => {
                let after = watchdog::describe_duration(after);
                let notice = self.projector.notice(
                    sheprd_chat_protocol::NoticeLevel::Warning,
                    format!(
                        "{} sent no updates for {after} after its last reply; cancelling the turn \
                         to release any lingering background task.",
                        self.config.agent
                    ),
                );
                self.emit(notice);
                // Cancel the stuck turn without dropping any follow-up prompts
                // the person queued behind it.
                if let Some(session) = self.acp_session.clone()
                    && let Err(error) = self
                        .outbox
                        .notify("session/cancel", &wire::CancelNotification::new(session))
                {
                    self.fail(&error.to_string());
                }
            }
            None => {}
        }
    }

    /// Whether an Antigravity turn has already streamed a reply and has no
    /// pending permission or in-progress foreground tool call.
    fn antigravity_quiet_after_reply(&self) -> bool {
        if self.config.harness != Some(Agent::Antigravity)
            || self.turn.is_none()
            || !self.asked.is_empty()
        {
            return false;
        }
        let mut last_reply = false;
        for entry in self.transcript.entries.iter().rev() {
            match &entry.kind {
                sheprd_chat_protocol::EntryKind::User { .. }
                | sheprd_chat_protocol::EntryKind::Turn(_) => break,
                sheprd_chat_protocol::EntryKind::Notice { .. } => continue,
                sheprd_chat_protocol::EntryKind::Tool(tool)
                    if matches!(
                        tool.status,
                        sheprd_chat_protocol::ToolStatus::Pending
                            | sheprd_chat_protocol::ToolStatus::InProgress
                    ) =>
                {
                    return false;
                }
                sheprd_chat_protocol::EntryKind::Agent { .. } if !last_reply => {
                    last_reply = true;
                }
                _ => {
                    if !last_reply {
                        return false;
                    }
                }
            }
        }
        last_reply
    }

    pub(crate) fn cursor(&self) -> Cursor {
        Cursor {
            epoch: self.epoch,
            seq: self.seq,
        }
    }

    /// Applies a change and sends it to every client.
    fn emit(&mut self, change: Change) {
        let change = self.pictures_before(change);
        if let Change::Truncate { before } = &change {
            self.keep_before_truncate(*before);
        }
        if self.transcript.apply(&change).is_err() {
            // The session only emits changes made from its own transcript, so
            // one that does not fit is a bug; dropping it keeps every client
            // consistent with the transcript.
            return;
        }
        if let Change::Truncate { before } = &change {
            self.projector.forget_before(*before);
        }
        if let Some(checkpoint) = &mut self.checkpoint {
            checkpoint.changed(&change);
        }
        self.keep_mark(&change);
        self.seq += 1;
        let size = change_size(&change);
        while self.log.len() == LOG || (self.log_bytes + size > LOG_BYTES && !self.log.is_empty()) {
            if let Some((_, old)) = self.log.pop_front() {
                self.log_bytes -= change_size(&old);
            }
        }
        self.log_bytes += size;
        self.log.push_back((self.seq, change.clone()));
        let message = ServerMessage::Change {
            change: change.clone(),
            cursor: self.cursor(),
        };
        let before = self.remote_clients();
        // A client that cannot keep up is dropped; its connection closes and
        // it reconnects with its cursor.
        self.subscribers.retain(
            |subscriber| match subscriber.outbox.try_send(message.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            },
        );
        if matches!(
            change,
            Change::Push { .. } | Change::Append { .. } | Change::Replace { .. }
        ) {
            self.bound();
        }
        if self.remote_clients() != before {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
        self.report_activity();
        self.pictures_after(&change);
        self.find_pull_requests(&change);
        self.keep_after(&change);
        self.find_brief(&change);
    }

    fn emit_all(&mut self, changes: impl IntoIterator<Item = Change>) {
        for change in changes {
            self.emit(change);
        }
    }

    /// Drops the oldest entries once the transcript holds too many or too much.
    fn bound(&mut self) {
        let drop = keep_from(&self.transcript.entries);
        if drop > 0 {
            let before = self.transcript.entries[drop].id;
            self.emit(Change::Truncate { before });
        }
    }

    fn remote_clients(&self) -> u32 {
        let count = self.subscribers.iter().filter(|s| !s.local).count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn report_activity(&mut self) {
        let activity = if self.transcript.is_blocked() {
            Activity::Blocked
        } else if matches!(self.transcript.status, Status::Working) {
            Activity::Working
        } else {
            Activity::Idle
        };
        if self.activity != Some(activity) {
            self.activity = Some(activity);
            (self.report)(Update::Activity(activity));
        }
    }

    fn status(&mut self, status: Status) {
        if self.transcript.status != status {
            self.emit(Change::Status { status });
        }
    }

    fn queue_changed(&mut self, queued: Vec<Queued>) {
        self.queued_origins
            .retain(|id, _| queued.iter().any(|waiting| waiting.id == *id));
        // A prompt that left the queue, withdrawn or dropped, keeps nothing
        // it was prepared with.
        self.steering
            .retain(|id, _| queued.iter().any(|waiting| waiting.id == *id));
        self.emit(Change::Queue { queued });
    }

    fn attach(&mut self, attach: Attach) {
        let Attach {
            conn,
            local,
            outbox,
            since,
        } = attach;
        let welcome = ServerMessage::Welcome {
            agent: self.config.agent.clone(),
            pane: self.config.pane.clone(),
            version: VERSION,
        };
        if outbox.try_send(welcome).is_err() {
            return;
        }
        let first = self.log.front().map_or(self.seq + 1, |(seq, _)| *seq);
        let catch_up = since.filter(|since| {
            since.epoch == self.epoch
                && since.seq <= self.seq
                && since.seq + 1 >= first
                && self.seq - since.seq <= CATCH_UP
        });
        let sent = match catch_up {
            Some(since) => {
                self.log
                    .iter()
                    .filter(|(seq, _)| *seq > since.seq)
                    .all(|(seq, change)| {
                        outbox
                            .try_send(ServerMessage::Change {
                                change: change.clone(),
                                cursor: Cursor {
                                    epoch: self.epoch,
                                    seq: *seq,
                                },
                            })
                            .is_ok()
                    })
            }
            None => outbox
                .try_send(ServerMessage::Snapshot {
                    cursor: self.cursor(),
                    transcript: Box::new(self.transcript.clone()),
                })
                .is_ok(),
        };
        if !sent {
            return;
        }
        self.subscribers.push(Subscriber {
            conn,
            local,
            outbox,
        });
        if !local {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
    }

    fn detach(&mut self, conn: ConnId) {
        self.guests.detached(conn);
        let before = self.remote_clients();
        self.subscribers
            .retain(|subscriber| subscriber.conn != conn);
        if self.remote_clients() != before {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
    }

    fn bye(&mut self, reason: ByeReason) {
        for subscriber in self.subscribers.drain(..) {
            let _ = subscriber.outbox.try_send(ServerMessage::Bye { reason });
        }
    }

    fn ack(&mut self, conn: ConnId, id: CommandId, rejected: Option<Rejection>) {
        if self.handled.len() == HANDLED {
            self.handled.pop_front();
        }
        self.handled.push_back((id, rejected));
        self.send_to(conn, ServerMessage::Ack { id, rejected });
    }

    fn send_to(&mut self, conn: ConnId, message: ServerMessage) {
        if let Some(subscriber) = self.subscribers.iter().find(|s| s.conn == conn)
            && subscriber.outbox.try_send(message).is_err()
        {
            self.detach(conn);
        }
    }

    fn request(&mut self, method: &str, params: &impl serde::Serialize, pending: Pending) -> bool {
        self.next_request += 1;
        let id = self.next_request;
        match self.outbox.request(id, method, params) {
            Ok(()) => {
                self.pending.insert(id, pending);
                true
            }
            Err(error) => {
                self.fail(&error.to_string());
                false
            }
        }
    }

    /// The session cannot continue: say why, and stop taking prompts.
    fn fail(&mut self, message: &str) {
        let notice = self
            .projector
            .notice(sheprd_chat_protocol::NoticeLevel::Error, message);
        self.emit(notice);
        self.status(Status::Failed {
            message: message.to_owned(),
        });
        self.clear_queue();
        self.drop_guests(Rejection::NotReady);
    }

    fn clear_queue(&mut self) {
        self.steering.clear();
        if !self.transcript.queued.is_empty() {
            self.queue_changed(Vec::new());
        }
    }
}

/// The wall clock in milliseconds since the Unix epoch, which clients on the
/// same machine count a running turn from.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The index of the oldest entry a transcript keeps: at most [`MAX_ENTRIES`]
/// and within both byte budgets, but always the newest entry. Live chats and
/// mirrored logs share it so neither can hold more than a snapshot frame.
pub(crate) fn keep_from(entries: &[sheprd_chat_protocol::Entry]) -> usize {
    let mut drop = entries.len().saturating_sub(MAX_ENTRIES);
    let kept = &entries[drop..];
    let mut text: usize = kept.iter().map(entry_size).sum();
    let mut media: usize = kept.iter().map(entry_media).sum();
    while (text > MAX_TRANSCRIPT_TEXT || media > MAX_TRANSCRIPT_MEDIA) && drop + 1 < entries.len() {
        text -= entry_size(&entries[drop]);
        media -= entry_media(&entries[drop]);
        drop += 1;
    }
    drop
}

/// Roughly how many bytes an entry adds to a snapshot.
pub(crate) fn entry_size(entry: &sheprd_chat_protocol::Entry) -> usize {
    use sheprd_chat_protocol::{EntryKind, ToolContent};
    match &entry.kind {
        EntryKind::Notice {
            answers,
            details,
            text,
            ..
        } => {
            text.len()
                + details.as_ref().map_or(0, String::len)
                + answers
                    .iter()
                    .map(|answer| answer.question.len() + answer.answer.len())
                    .sum::<usize>()
        }
        EntryKind::Tool(tool) => {
            tool.title.len()
                + tool.locations.iter().map(String::len).sum::<usize>()
                + tool
                    .content
                    .iter()
                    .map(|content| match content {
                        ToolContent::Text { text } => text.len(),
                        ToolContent::Diff { new, old, path } => {
                            new.len() + old.as_ref().map_or(0, String::len) + path.len()
                        }
                        ToolContent::Terminal { id } => id.len(),
                        ToolContent::Image { .. } => 0,
                    })
                    .sum::<usize>()
        }
        EntryKind::Turn(_) => 32,
        // What went ahead of the person's words travels with them.
        EntryKind::User { text, note, .. } => text.len() + note.as_ref().map_or(0, String::len),
        kind => kind.text().map_or(0, str::len),
    }
}

/// The image data an entry adds to a snapshot, counted apart from its text.
fn entry_media(entry: &sheprd_chat_protocol::Entry) -> usize {
    use sheprd_chat_protocol::{EntryKind, ToolContent};
    match &entry.kind {
        EntryKind::Image { image, .. } => image.data.len(),
        EntryKind::Tool(tool) => tool
            .content
            .iter()
            .map(|content| match content {
                ToolContent::Image { image } => image.data.len(),
                _ => 0,
            })
            .sum(),
        _ => 0,
    }
}

/// Roughly how many bytes a logged change holds.
fn change_size(change: &Change) -> usize {
    match change {
        Change::Push { entry } | Change::Replace { entry } => {
            entry_size(entry) + entry_media(entry)
        }
        Change::Append { text, .. } => text.len(),
        _ => 64,
    }
}

#[cfg(test)]
mod tests;
