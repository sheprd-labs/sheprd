//! Which agents want the user: the number on the app's badge, and which
//! rows show unread. An agent counts while it needs an answer, and while it
//! has a reply no one read (`control::read`).
//!
//! One set of marks serves every phone and Sheprd reaching this computer:
//! opening a chat on one clears it on all of them, and the agent's next
//! reply brings it back. A phone reads what it shows on screen; Sheprd says
//! what it showed (`Request::Read`). The marks are saved beside the daemon's
//! other state, so they outlive a restart.

use super::{Shared, State};
use crate::{
    control::read::{Change, Finish, Marks, Reply, Unread},
    hub::Layout,
};
use sheprd_client::protocol::AgentStatus;
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::sync::watch;

/// How long marks wait to be saved after they change, so a run of them is
/// written once.
const SAVE_AFTER: Duration = Duration::from_secs(2);

/// What has been read here.
#[derive(Debug)]
pub(crate) struct Opened {
    /// Moves when what counts or shows unread changes, for views to refresh.
    changed: watch::Sender<u64>,
    /// Moves when the marks change, for [`keep`] to save them.
    dirty: watch::Sender<u64>,
    /// Where the marks are saved; none in tests.
    file: Option<PathBuf>,
    /// The file was missing, so these marks start now: saved at once.
    fresh: bool,
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    marks: Marks,
    /// Views open on each pane now: what is replied in one is read.
    watching: HashMap<String, usize>,
}

impl Default for Opened {
    fn default() -> Self {
        Self::new(Marks::default(), None)
    }
}

/// What a chat's transcript says, beyond the layout.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChatFacts {
    pub(crate) finish: Finish,
    pub(crate) questions: usize,
}

/// Who wants the user now, and which of them only to be read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Seen {
    pub(crate) needing: BTreeSet<String>,
    pub(crate) unread: BTreeSet<String>,
}

/// An agent as this module sees it.
#[derive(Clone, Copy)]
struct Agent {
    reply: Reply,
    status: AgentStatus,
}

/// Every agent by pane, in chats and terminals alike.
fn agents(layout: &Layout, chat: impl Fn(&str) -> ChatFacts) -> HashMap<&str, Agent> {
    // A chat's reply is known by its transcript's last finish; one whose
    // transcript is not read yet by Herdr's status alone.
    let chats = layout.chats().map(|row| {
        let finish = chat(&row.pane).finish;
        let status = row.status;
        let news = finish.is_some() || status == AgentStatus::Done;
        (
            row.pane.as_str(),
            Agent {
                reply: Reply { finish, news },
                status,
            },
        )
    });
    // Every pane's agent: one finished in a split counts while another
    // still works. A terminal agent has replied while Herdr says done,
    // which looking at it on the computer clears.
    let terminals = layout
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.terminals.iter())
        .flat_map(|terminal| terminal.agent.iter().chain(&terminal.agents))
        .map(|agent| {
            let reply = Reply {
                finish: agent.changed_ms,
                news: agent.status == AgentStatus::Done,
            };
            (
                agent.pane.as_str(),
                Agent {
                    reply,
                    status: agent.status,
                },
            )
        });
    chats.chain(terminals).collect()
}

impl Opened {
    fn new(marks: Marks, file: Option<PathBuf>) -> Self {
        let fresh = file.as_deref().is_some_and(|file| !file.exists());
        Self {
            fresh,
            changed: watch::channel(0).0,
            dirty: watch::channel(0).0,
            file,
            inner: Mutex::new(Inner {
                marks,
                watching: HashMap::new(),
            }),
        }
    }

    /// The marks saved in `file`. Blocking.
    pub(crate) fn load(file: PathBuf) -> Self {
        let marks = crate::app::read::load(&file, super::now_ms());
        Self::new(marks, Some(file))
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn moved(&self) {
        self.changed
            .send_modify(|moved| *moved = moved.wrapping_add(1));
        self.saved_later();
    }

    fn saved_later(&self) {
        self.dirty
            .send_modify(|moved| *moved = moved.wrapping_add(1));
    }

    /// Records that someone read `pane`.
    pub(crate) fn open(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        let agents = agents(layout, chat);
        let Some(agent) = agents.get(pane) else {
            return;
        };
        if self.inner().marks.read(pane, agent.reply, super::now_ms()) {
            self.moved();
        }
    }

    /// A view of `pane` opened.
    pub(super) fn begin(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        *self.inner().watching.entry(pane.to_owned()).or_default() += 1;
        self.open(layout, pane, chat);
    }

    /// A view of `pane` closed. What was replied in it was read, even if
    /// nothing asked for the count while it was open.
    pub(super) fn end(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        self.open(layout, pane, chat);
        let mut inner = self.inner();
        if let Some(views) = inner.watching.get_mut(pane) {
            *views -= 1;
            if *views == 0 {
                inner.watching.remove(pane);
            }
        }
    }

    /// Whether a view of `pane` is open, which marks what it replies read.
    #[cfg(all(test, unix))]
    pub(crate) fn watched(&self, pane: &str) -> bool {
        self.inner().watching.contains_key(pane)
    }

    /// Whether these marks start now, their file missing.
    #[cfg(test)]
    pub(crate) fn fresh(&self) -> bool {
        self.fresh
    }

    /// Moves whenever what counts or shows unread changes.
    pub(crate) fn changed(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// The marks as they stand, to save.
    fn marks(&self) -> Marks {
        self.inner().marks.clone()
    }

    /// The panes that want the user now; `chat` says what a chat's
    /// transcript holds.
    #[cfg(test)]
    pub(crate) fn needing(
        &self,
        layout: &Layout,
        chat: impl Fn(&str) -> ChatFacts,
    ) -> BTreeSet<String> {
        self.seen(layout, chat).needing
    }

    /// Who wants the user now. What open views show is read first.
    pub(crate) fn seen(&self, layout: &Layout, chat: impl Fn(&str) -> ChatFacts) -> Seen {
        let agents = agents(layout, &chat);
        let now_ms = super::now_ms();
        let mut inner = self.inner();
        let Inner { marks, watching } = &mut *inner;
        let mut dirty = false;
        for pane in watching.keys() {
            if let Some(agent) = agents.get(pane.as_str()) {
                dirty |= marks.read(pane, agent.reply, now_ms);
            }
        }
        let unread: BTreeSet<String> = agents
            .iter()
            .filter(|(pane, agent)| marks.unread(pane, agent.reply))
            .map(|(pane, _)| (*pane).to_owned())
            .collect();
        drop(inner);
        if dirty {
            self.saved_later();
        }
        let chats = layout.chats().map(|row| row.pane.as_str());
        let asking = chats.filter(|pane| chat(pane).questions > 0);
        let blocked = agents
            .iter()
            .filter(|(_, agent)| agent.status == AgentStatus::Blocked)
            .map(|(pane, _)| *pane);
        let needing = asking
            .chain(blocked)
            .map(str::to_owned)
            .chain(unread.iter().cloned())
            .collect();
        Seen { needing, unread }
    }

    /// Marks `change`'s panes read or unread, those this computer has, and
    /// answers with what is unread now.
    pub(crate) fn change(
        &self,
        layout: &Layout,
        change: &Change,
        chat: impl Fn(&str) -> ChatFacts,
    ) -> Unread {
        let agents = agents(layout, &chat);
        let now_ms = super::now_ms();
        let mut moved = false;
        {
            let mut inner = self.inner();
            for pane in &change.panes {
                let Some(agent) = agents.get(pane.as_str()) else {
                    continue;
                };
                moved |= if change.read {
                    inner.marks.read(pane, agent.reply, now_ms)
                } else {
                    inner.marks.mark_unread(pane, agent.reply, now_ms)
                };
            }
        }
        if moved {
            self.moved();
        }
        Unread {
            panes: self.seen(layout, chat).unread,
        }
    }

    /// Follows the primary Herdr 1 session's `boot` (`Marks::follow_boot`).
    pub(crate) fn follow_boot(&self, boot: &str) {
        if self.inner().marks.follow_boot(boot, super::now_ms()) {
            self.moved();
        }
    }
}

/// A phone's view of one pane, for as long as this lives.
pub(crate) struct Watching {
    pane: String,
    state: Shared,
}

impl Watching {
    pub(crate) fn new(state: &Shared, pane: &str) -> Self {
        state
            .opened
            .begin(&state.hub.layout(), pane, |pane| state.chat_facts(pane));
        Self {
            pane: pane.to_owned(),
            state: state.clone(),
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        let state = &self.state;
        state.opened.end(&state.hub.layout(), &self.pane, |pane| {
            state.chat_facts(pane)
        });
    }
}

impl State {
    /// What `pane`'s chat holds; nothing for a pane that is not a chat.
    fn chat_facts(&self, pane: &str) -> ChatFacts {
        self.hub.chat(pane).map_or_else(ChatFacts::default, |link| {
            let summary = link.summary();
            ChatFacts {
                finish: summary.last_turn_ended_ms,
                questions: summary.questions,
            }
        })
    }

    /// Records that a phone showed `pane`.
    pub(crate) fn mark_opened(&self, layout: &Layout, pane: &str) {
        self.opened.open(layout, pane, |pane| self.chat_facts(pane));
    }

    /// Who wants the user now, and which of them only to be read.
    pub(crate) fn seen(&self) -> Seen {
        self.opened
            .seen(&self.hub.layout(), |pane| self.chat_facts(pane))
    }

    /// The panes that want the user now.
    pub(crate) fn attention(&self) -> BTreeSet<String> {
        self.seen().needing
    }

    /// Marks `change`'s panes and answers with what is unread now.
    pub(crate) fn mark_read(&self, change: &Change) -> Unread {
        self.opened
            .change(&self.hub.layout(), change, |pane| self.chat_facts(pane))
    }
}

/// Saves the marks shortly after each change, for as long as the daemon
/// runs. A failed save is logged and tried again on the next change.
pub(crate) async fn keep(state: Shared) {
    let Some(file) = state.opened.file.clone() else {
        return;
    };
    let mut dirty = state.opened.dirty.subscribe();
    // Marks a first start began are saved at once: their cutoff must
    // outlive a restart before any mark changes, or what was replied
    // meanwhile would count as read.
    if state.opened.fresh {
        state.opened.saved_later();
    }
    while dirty.changed().await.is_ok() {
        tokio::time::sleep(SAVE_AFTER).await;
        dirty.mark_unchanged();
        let marks = state.opened.marks();
        let file = file.clone();
        let saved =
            tokio::task::spawn_blocking(move || crate::app::read::save(&file, &marks)).await;
        if let Ok(Err(error)) = saved {
            tracing::warn!(%error, "could not save which chats were read");
        }
    }
}
