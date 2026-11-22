//! A terminal agent's conversation, shown to phones as a chat. A Claude
//! Code or Codex CLI running in a Herdr pane keeps its own transcript
//! (`sheprd_client::herdr::observe` names which agents do); the same
//! `sheprd-acp observe` Sheprd's `render_existing` uses projects it, read
//! only, while the terminal keeps all input. An observer starts when a
//! phone first opens the chat and ends with the agent, when the pane's
//! agent or session changes, or when newer ones push it out.
//!
//! The pane's session comes from Herdr (`agent_session`), the process from
//! its process info, and the process's birth is read here, on the host the
//! pane runs on, so the observer can refuse a PID reused since.

mod error;

pub use error::Error;

use super::{ApiMethod, Chat, Hub, core};
use serde::Deserialize;
use serde_json::{Value, json};
use sheprd_chat_protocol::Agent;
use sheprd_client::herdr::{
    observe,
    processes::{agent_args, agent_pid, local_identity},
    snapshot::{Pane, ProcessInfo, SessionKind},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Observers kept at once. Past it the least recently opened one that no
/// phone follows ends; one every phone still follows is never ended.
const MAX_OBSERVED: usize = 8;
/// How long a new observer has to read its first transcript.
const FIRST_READ: Duration = Duration::from_secs(10);
/// How often running observers' sessions are checked against Herdr's.
const SESSION_CHECK: Duration = Duration::from_secs(3);

/// The observers running now, by pane, with what each was started for.
#[derive(Default)]
pub(crate) struct Observed(Mutex<Vec<Running>>);

struct Running {
    binding: Binding,
    chat: Arc<Chat>,
}

/// What one observer reads: the pane's agent process and its session.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    pane: String,
    agent: Agent,
    session: String,
    pid: u32,
    started: u64,
}

impl Observed {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Running>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The running observer for `binding`, ending one for the same pane
    /// that reads anything else.
    fn find(&self, binding: &Binding) -> Option<Arc<Chat>> {
        let mut running = self.lock();
        running.retain(|kept| {
            let stale = kept.chat.is_closed()
                || (kept.binding.pane == binding.pane && kept.binding != *binding);
            if stale {
                kept.chat.close();
            }
            !stale
        });
        running
            .iter()
            .find(|kept| kept.binding == *binding)
            .map(|kept| kept.chat.clone())
    }

    fn keep(&self, binding: Binding, chat: Arc<Chat>) {
        let mut running = self.lock();
        running.push(Running { binding, chat });
        while running.len() > MAX_OBSERVED {
            // Never the one just added, which its phone is about to follow.
            let older = &running[..running.len() - 1];
            let Some(oldest) = older.iter().position(|kept| kept.chat.followers() == 0) else {
                break;
            };
            running.remove(oldest).chat.close();
        }
    }

    /// Each running observer's pane and the session it reads.
    fn sessions(&self) -> Vec<(String, String)> {
        self.lock()
            .iter()
            .map(|kept| (kept.binding.pane.clone(), kept.binding.session.clone()))
            .collect()
    }

    /// Ends `chat` and forgets it.
    fn discard(&self, chat: &Arc<Chat>) {
        chat.close();
        self.lock().retain(|kept| !Arc::ptr_eq(&kept.chat, chat));
    }

    /// Ends the observer of `pane` that reads `session`.
    fn end(&self, pane: &str, session: &str) {
        self.lock().retain(|kept| {
            let ended = kept.binding.pane == pane && kept.binding.session == session;
            if ended {
                kept.chat.close();
            }
            !ended
        });
    }

    /// Ends the observers of panes that no longer show an agent that can be
    /// observed.
    pub(crate) fn retain(&self, shown: impl Fn(&str) -> bool) {
        self.lock().retain(|kept| {
            let keep = shown(&kept.binding.pane) && !kept.chat.is_closed();
            if !keep {
                kept.chat.close();
            }
            keep
        });
    }
}

/// Whether the agent named `name` in `pane` has a conversation phones can
/// read as a chat: one [`observe::observable`] names, in a Herdr 1 pane (a
/// Herdr 2 terminal reports its processes another way). The same on every
/// host, so a phone sees one answer; [`Hub::observe`] refuses where
/// processes cannot be read.
pub(crate) fn observable(pane: &str, name: Option<&str>) -> bool {
    name.and_then(observe::observable).is_some() && !core::owns(&json!({"pane_id": pane}))
}

#[derive(Deserialize)]
struct PaneAnswer {
    pane: Pane,
}

#[derive(Deserialize)]
struct ProcessAnswer {
    process_info: ProcessInfo,
}

fn parse<T: for<'de> Deserialize<'de>>(answer: Value) -> Result<T> {
    serde_json::from_value(answer).map_err(Error::Answer)
}

impl Hub {
    /// Whether `pane` shows a terminal agent whose conversation can be
    /// observed, as the layout has it now.
    pub(crate) fn observable(&self, pane: &str) -> bool {
        self.layout().terminal(pane).is_some_and(|(_, terminal)| {
            terminal
                .agents
                .iter()
                .any(|agent| agent.pane == pane && observable(pane, agent.name.as_deref()))
        })
    }

    /// The chat observing `pane`'s terminal agent, started when none reads
    /// its current conversation yet.
    pub(crate) async fn observe(&self, pane: &str) -> super::Result<Arc<Chat>> {
        if !cfg!(any(target_os = "linux", target_os = "macos")) || !self.observable(pane) {
            return Err(Error::Unsupported.into());
        }
        let program = self.program().ok_or(super::Error::ProgramMissing)?;
        let binding = self.binding(pane).await?;
        let chat = match self.observed.find(&binding) {
            Some(chat) => chat,
            None => self.start_observer(program, binding)?,
        };
        // An observer that cannot find or read the conversation ends before
        // its first transcript; the phone is told, not shown an empty chat.
        // It is already among the kept ones, so a phone that leaves during
        // this wait leaves it bounded and ended with the rest.
        if !chat.opened(FIRST_READ).await {
            self.observed.discard(&chat);
            return Err(Error::Unreadable.into());
        }
        Ok(chat)
    }

    fn start_observer(
        &self,
        program: std::path::PathBuf,
        binding: Binding,
    ) -> super::Result<Arc<Chat>> {
        let pane = binding.pane.clone();
        let mut command = sheprd_client::new_command(program);
        command.args([
            "observe".to_owned(),
            binding.agent.name().to_owned(),
            binding.session.clone(),
            binding.pid.to_string(),
            binding.started.to_string(),
            "--pane".to_owned(),
            pane.clone(),
        ]);
        let chat = Chat::observe(pane, command).map_err(Error::Start)?;
        self.observed.keep(binding, chat.clone());
        Ok(chat)
    }

    /// Ends observers whose agent moved to another session in the same
    /// process, as Claude Code's `/resume` or `/clear` does: its old
    /// transcript stays valid, so only Herdr's report tells. Runs for the
    /// hub's life, asking only while observers run.
    pub(crate) async fn follow_observed_sessions(self: Arc<Self>) {
        let mut tick = tokio::time::interval(SESSION_CHECK);
        loop {
            tick.tick().await;
            for (pane, session) in self.observed.sessions() {
                let Ok(answer) = self
                    .call(ApiMethod::PaneGet, json!({"pane_id": pane}))
                    .await
                else {
                    continue;
                };
                let Ok(found) = parse::<PaneAnswer>(answer) else {
                    continue;
                };
                if found.pane.agent_session.map(|now| now.value).as_deref()
                    != Some(session.as_str())
                {
                    self.observed.end(&pane, &session);
                }
            }
        }
    }

    /// Asks Herdr which session and process `pane`'s agent runs, then
    /// reads that process's birth here.
    async fn binding(&self, pane: &str) -> super::Result<Binding> {
        let params = json!({"pane_id": pane});
        let found: PaneAnswer = parse(self.call(ApiMethod::PaneGet, params.clone()).await?)?;
        let agent = found
            .pane
            .agent
            .as_deref()
            .and_then(observe::observable)
            .ok_or(Error::Unsupported)?;
        // The session and the pane must name the same agent.
        let session = found
            .pane
            .agent_session
            .filter(|session| {
                session.kind == SessionKind::Id
                    && session.agent == agent.name()
                    && uuid::Uuid::parse_str(&session.value).is_ok()
            })
            .ok_or(Error::Session)?;
        let info: ProcessAnswer = parse(self.call(ApiMethod::PaneProcessInfo, params).await?)?;
        let identities: HashMap<u32, _> = info
            .process_info
            .foreground_processes
            .iter()
            .filter_map(|process| Some((process.pid, local_identity(process.pid)?)))
            .collect();
        let pid = agent_pid(agent, &info.process_info, |pid| {
            identities
                .get(&pid)
                .copied()
                .or_else(|| local_identity(pid))
        })
        .ok_or(Error::Process)?;
        let started = identities.get(&pid).ok_or(Error::Process)?.started;
        // A launch that names another session than Herdr's cannot be
        // trusted to be showing Herdr's.
        let argv = info
            .process_info
            .foreground_processes
            .iter()
            .find(|process| process.pid == pid)
            .and_then(|process| agent_args(agent, &process.argv))
            .ok_or(Error::Process)?;
        if !observe::launch_agrees(agent, argv, &session.value) {
            return Err(Error::Session.into());
        }
        Ok(Binding {
            pane: pane.to_owned(),
            agent,
            session: session.value,
            pid,
            started,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
