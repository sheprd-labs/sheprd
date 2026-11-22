//! Which agents have a reply no one read, which the agents panel marks.
//!
//! Each device's own daemon keeps what was read of its agents, by pane
//! (`sheprd_remote_control::control::read`), so every phone and desktop
//! reaching it shows the same dots: reading on one clears it on all of them.
//! [`AgentRead`] holds what each reachable daemon last answered, by endpoint
//! ID. An agent this app shows focused, in a window in front, for a moment
//! is read, and the change goes at once; it shows at once too, laid over
//! each answer until the daemon takes it.

mod error;
mod sync;

pub use error::Error;

use crate::device_tags::Daemon;
use gpui::{App, BorrowAppContext, Global, WindowId};
use sheprd_remote_control::control::read::{Change, Unread};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// How often a reachable machine is asked over SSH. A reply read on a
/// phone clears here within it; this app's own reads go at once.
const REMOTE_INTERVAL: Duration = Duration::from_secs(30);
/// This computer's daemon is a socket away.
const LOCAL_INTERVAL: Duration = Duration::from_secs(5);
/// How long an agent stays focused before it counts as read, so passing
/// over it on the way to another does not.
const SETTLE: Duration = Duration::from_secs(1);
/// The most changes waiting for one device's daemon; past it, the oldest go.
const MAX_PENDING: usize = 64;

/// An agent as read marks know it: its host's endpoint ID, and its pane as
/// that daemon names it (`control::favorites`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Agent {
    pub(crate) endpoint: String,
    pub(crate) key: String,
}

/// Every device's unread agents as this app shows them, by endpoint ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sets {
    devices: BTreeMap<String, BTreeSet<String>>,
}

impl Sets {
    /// Whether the agent in `pane` on the device `device` has a reply no
    /// one read.
    pub(crate) fn contains(&self, device: &str, pane: &str) -> bool {
        self.devices
            .get(device)
            .is_some_and(|panes| panes.contains(pane))
    }

    /// Whether the device `device` has any unread agent.
    pub(crate) fn any(&self, device: &str) -> bool {
        self.devices
            .get(device)
            .is_some_and(|panes| !panes.is_empty())
    }

    /// `records` as their daemons answered, for tests of what draws them.
    #[cfg(test)]
    pub(crate) fn unread(records: &[(&str, &[&str])]) -> Self {
        let devices = records
            .iter()
            .map(|(device, panes)| {
                let panes = panes.iter().map(|pane| (*pane).to_owned()).collect();
                ((*device).to_owned(), panes)
            })
            .collect();
        Self { devices }
    }

    /// Replaces a device's unread agents. Returns whether anything changed.
    fn replace(&mut self, device: &str, panes: Option<BTreeSet<String>>) -> bool {
        match panes {
            Some(panes) if self.devices.get(device) != Some(&panes) => {
                self.devices.insert(device.to_owned(), panes);
                true
            }
            Some(_) => false,
            None => self.devices.remove(device).is_some(),
        }
    }
}

/// `stood`, what a daemon answered, with the changes it has not taken yet.
pub(crate) fn with_pending<'a>(
    mut stood: BTreeSet<String>,
    pending: impl IntoIterator<Item = &'a Change>,
) -> BTreeSet<String> {
    for change in pending {
        for pane in &change.panes {
            if change.read {
                stood.remove(pane);
            } else {
                stood.insert(pane.clone());
            }
        }
    }
    stood
}

#[derive(Debug, Default)]
struct Link {
    last: Option<Instant>,
    running: bool,
    /// Marks made here that the daemon has not answered, the oldest first;
    /// the front one is on its way while `running`.
    pending: VecDeque<Change>,
    /// The last exchange failed: what waits goes again after the interval,
    /// not on the next poll, until a new change is made.
    failed: bool,
}

impl Link {
    /// Whether an exchange is due: a change waiting goes at once unless the
    /// last exchange failed, and otherwise the daemon is read once each
    /// `interval`.
    fn due(&self, interval: Duration) -> bool {
        if self.running {
            return false;
        }
        let stale = self.last.is_none_or(|last| last.elapsed() >= interval);
        stale || (!self.failed && !self.pending.is_empty())
    }
}

/// The agent a window shows focused, and since when.
#[derive(Debug)]
struct Focus {
    agent: Agent,
    since: Instant,
    /// Marked unread here while focused: left so until focus moves.
    held: bool,
}

/// The unread agents shown, each device's link to its daemon, and what
/// each window shows focused.
#[derive(Default)]
pub(crate) struct AgentRead {
    sets: Arc<Sets>,
    /// The devices windows can reach now, by endpoint ID.
    daemons: BTreeMap<String, Daemon>,
    links: HashMap<String, Link>,
    focus: HashMap<WindowId, Focus>,
}

impl Global for AgentRead {}

impl AgentRead {
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// The unread agents to draw from; none before [`Self::install`], as in
    /// tests.
    pub(crate) fn current(cx: &App) -> Arc<Sets> {
        cx.try_global::<Self>()
            .map(|this| this.sets.clone())
            .unwrap_or_default()
    }

    /// Marks `agent` read or unread, shows it at once, and sends it to the
    /// device's daemon.
    pub(crate) fn set(cx: &mut App, agent: &Agent, read: bool) {
        if !cx.has_global::<Self>() {
            return;
        }
        let change = Change {
            panes: vec![agent.key.clone()],
            read,
        };
        if !change.valid() {
            return;
        }
        cx.update_global::<Self, _>(|this, _| {
            if !read {
                for focus in this.focus.values_mut() {
                    focus.held |= focus.agent == *agent;
                }
            }
            let device = agent.endpoint.as_str();
            let current = this.sets.devices.get(device).cloned().unwrap_or_default();
            let next = with_pending(current, [&change]);
            let link = this.links.entry(device.to_owned()).or_default();
            if link.pending.len() == MAX_PENDING {
                link.pending.pop_front();
            }
            link.pending.push_back(change);
            // A new change goes at once, even after a failure.
            link.failed = false;
            let mut sets = (*this.sets).clone();
            sets.replace(device, Some(next));
            this.sets = Arc::new(sets);
        });
        cx.refresh_windows();
        Self::tick(cx);
    }

    /// Tells which devices a window can reach. Windows report every poll, so
    /// this only touches the global when the set changed.
    pub(crate) fn reach(cx: &mut App, reachable: &BTreeMap<String, Daemon>) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if *reachable != this.daemons {
            cx.global_mut::<Self>().daemons = reachable.clone();
        }
        Self::tick(cx);
    }

    /// Tells which agent `window` shows focused now, `None` when none or
    /// when the window is not in front. One focused for [`SETTLE`] is read,
    /// and stays read while it stays focused, its next reply included.
    pub(crate) fn focus(cx: &mut App, window: WindowId, focused: Option<Agent>) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let Some(agent) = focused else {
            if this.focus.contains_key(&window) {
                cx.global_mut::<Self>().focus.remove(&window);
            }
            return;
        };
        let same = this
            .focus
            .get(&window)
            .filter(|focus| focus.agent == agent)
            .map(|focus| !focus.held && focus.since.elapsed() >= SETTLE);
        match same {
            Some(settled) => {
                if settled && this.sets.contains(&agent.endpoint, &agent.key) {
                    Self::set(cx, &agent, true);
                }
            }
            None => {
                let focus = Focus {
                    agent,
                    since: Instant::now(),
                    held: false,
                };
                cx.global_mut::<Self>().focus.insert(window, focus);
            }
        }
    }

    /// Starts the exchanges that are due: a change waiting goes at once, and
    /// otherwise each daemon is read once an interval.
    fn tick(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let due: Vec<(String, Daemon, Option<Change>)> = this
            .daemons
            .iter()
            .filter_map(|(id, daemon)| {
                let interval = match daemon {
                    Daemon::Local => LOCAL_INTERVAL,
                    Daemon::Remote(_) => REMOTE_INTERVAL,
                };
                let link = this.links.get(id);
                if !link.is_none_or(|link| link.due(interval)) {
                    return None;
                }
                let change = link.and_then(|link| link.pending.front().cloned());
                Some((id.clone(), daemon.clone(), change))
            })
            .collect();
        for (id, daemon, change) in due {
            let link = cx.global_mut::<Self>().links.entry(id.clone()).or_default();
            link.running = true;
            link.last = Some(Instant::now());
            let sent = change.clone();
            let task = cx
                .background_executor()
                .spawn(async move { sync::exchange(&daemon, change, &AtomicBool::new(false)) });
            cx.spawn(async move |cx| {
                let answer = task.await;
                cx.update(|cx| Self::answered(cx, &id, sent.as_ref(), answer));
            })
            .detach();
        }
    }

    fn answered(
        cx: &mut App,
        id: &str,
        sent: Option<&Change>,
        answer: Result<Option<Unread>, Error>,
    ) {
        if let Err(error) = &answer {
            tracing::debug!(id, %error, "Could not exchange a device's read marks");
        }
        let (changed, more) = cx.update_global::<Self, _>(|this, _| {
            let link = this.links.entry(id.to_owned()).or_default();
            link.running = false;
            link.failed = answer.is_err();
            let shown = match answer {
                // A failed exchange tries again after the interval, the
                // changes still waiting.
                Err(_) => return (false, false),
                // Nothing keeps marks there, so nothing shows unread.
                Ok(None) => {
                    link.pending.clear();
                    None
                }
                Ok(Some(stood)) => {
                    if sent.is_some() && link.pending.front() == sent {
                        link.pending.pop_front();
                    }
                    Some(with_pending(stood.panes, &link.pending))
                }
            };
            let more = !link.pending.is_empty();
            let mut sets = (*this.sets).clone();
            let changed = sets.replace(id, shown);
            if changed {
                this.sets = Arc::new(sets);
            }
            (changed, more)
        });
        if changed {
            cx.refresh_windows();
        }
        if more {
            Self::tick(cx);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
