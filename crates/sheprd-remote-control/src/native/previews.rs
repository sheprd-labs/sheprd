//! What each terminal agent last said, for its row on a phone. A terminal
//! has no transcript, so its screen is read and its words picked out
//! ([`screen::preview`]). Read only while a phone is connected, never while
//! a home is drawn, which only looks the pane up here: when an agent first
//! shows up, whenever it changes state, and every [`WORKING`] while it
//! works. A Herdr 2 screen is read by its core ([`crate::hub::Hub::screen_text`]),
//! so these reads never take a screen a phone opens a terminal with.

mod screen;

use crate::web::Shared;
use sheprd_client::protocol::AgentStatus;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;

/// Agents followed; more terminal tabs than this go without.
const MAX_AGENTS: usize = 64;
/// Screens read in one pass, so a burst of changes waits its turn.
const MAX_READS: usize = 8;
/// How often a working agent's screen is read again.
const WORKING: Duration = Duration::from_secs(10);
/// How long after a failed read the screen is tried again.
const RETRY: Duration = Duration::from_secs(10);
/// How often the agents are looked at, to find working ones due a read.
const TICK: Duration = Duration::from_secs(2);
/// How long a burst of layout changes settles into one pass.
const SETTLE: Duration = Duration::from_millis(500);

/// The previews read, by the pane their agent runs in.
pub(crate) struct Previews {
    known: watch::Sender<Arc<HashMap<String, String>>>,
}

impl Default for Previews {
    fn default() -> Self {
        Self {
            known: watch::Sender::new(Arc::default()),
        }
    }
}

impl Previews {
    /// The previews as last read.
    pub(crate) fn current(&self) -> Arc<HashMap<String, String>> {
        self.known.borrow().clone()
    }

    /// Changes whenever a read finds a new preview.
    pub(crate) fn watch(&self) -> watch::Receiver<Arc<HashMap<String, String>>> {
        self.known.subscribe()
    }

    #[cfg(test)]
    pub(crate) fn set_for_tests(&self, known: HashMap<String, String>) {
        self.known.send_replace(Arc::new(known));
    }
}

/// An agent as one pass sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Agent {
    pane: String,
    status: AgentStatus,
    changed_ms: Option<u64>,
}

/// An agent's state at its last read, when that was, and whether it
/// failed.
#[derive(Clone, Copy, Debug)]
struct Read {
    status: AgentStatus,
    changed_ms: Option<u64>,
    at: Instant,
    failed: bool,
}

/// Whether `agent`'s screen is due a read at `now`, last read as `read`:
/// a failed read is tried again after [`RETRY`] whatever the agent does.
fn due(agent: &Agent, read: Option<&Read>, now: Instant) -> bool {
    let Some(read) = read else { return true };
    let since = now.saturating_duration_since(read.at);
    if read.failed {
        return since >= RETRY;
    }
    read.status != agent.status
        || read.changed_ms != agent.changed_ms
        || (agent.status == AgentStatus::Working && since >= WORKING)
}

/// The agents due a read, at most [`MAX_READS`]: never read first, then
/// the longest unread.
fn plan<'a>(agents: &'a [Agent], reads: &HashMap<String, Read>, now: Instant) -> Vec<&'a Agent> {
    let mut due: Vec<&Agent> = agents
        .iter()
        .filter(|agent| due(agent, reads.get(&agent.pane), now))
        .collect();
    due.sort_by_key(|agent| reads.get(&agent.pane).map(|read| read.at));
    due.truncate(MAX_READS);
    due
}

/// The agent each terminal tab lists, the one its row shows.
fn agents(state: &Shared) -> Vec<Agent> {
    state
        .hub
        .layout()
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.terminals)
        .filter_map(|terminal| terminal.agent.as_ref())
        .take(MAX_AGENTS)
        .map(|agent| Agent {
            pane: agent.pane.clone(),
            status: agent.status,
            changed_ms: agent.changed_ms,
        })
        .collect()
}

/// Reads the agents' screens while a phone is connected, when the layout
/// changes and every [`TICK`].
pub(crate) async fn follow(state: Shared) {
    let mut layout = state.hub.watch_layout();
    let mut reads: HashMap<String, Read> = HashMap::new();
    loop {
        if state.connected().is_empty() {
            // Agents go on without a phone looking, and a Herdr 2 agent
            // finishing another turn leaves its state and time as they
            // were: a phone that comes back has every screen read again.
            reads.clear();
        } else {
            let agents = agents(&state);
            let listed = |pane: &String| agents.iter().any(|agent| &agent.pane == pane);
            reads.retain(|pane, _| listed(pane));
            let mut found = state.previews.current().as_ref().clone();
            found.retain(|pane, _| listed(pane));
            for agent in plan(&agents, &reads, Instant::now()) {
                // A failed read keeps the last preview, and is tried again
                // after `RETRY`.
                let read = state.hub.screen_text(&agent.pane).await;
                let failed = read.is_err();
                match read.map(|text| screen::preview(&text)) {
                    Ok(Some(preview)) => {
                        found.insert(agent.pane.clone(), preview);
                    }
                    Ok(None) => {
                        found.remove(&agent.pane);
                    }
                    Err(error) => {
                        tracing::debug!(%error, "Could not read a terminal agent's screen");
                    }
                }
                let read = Read {
                    status: agent.status,
                    changed_ms: agent.changed_ms,
                    at: Instant::now(),
                    failed,
                };
                reads.insert(agent.pane.clone(), read);
            }
            state.previews.known.send_if_modified(|known| {
                if **known == found {
                    return false;
                }
                *known = Arc::new(found);
                true
            });
        }
        tokio::select! {
            changed = layout.changed() => if changed.is_err() { return },
            () = tokio::time::sleep(TICK) => {}
        }
        tokio::time::sleep(SETTLE).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
