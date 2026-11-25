//! One phone's shared card for overlapping turns on this computer. The
//! immutable routing key belongs to the card. Its mutable focus identifies
//! the actual pane to open or answer, including after the first pane finishes.
use crate::apns::{
    AgentProgress, Attributes, ContentState, Event, Live, Pacer, Phase, Registration,
};
use sheprd_chat_protocol::EndpointId;
use std::collections::{HashMap, HashSet};

const PREFIX: &str = "host-activity/";
pub(super) const SHARED_RECIPIENT: &str = "relay/all-hosts";

/// The optional epoch comes from the phone and survives an offline reset.
fn card_prefix(registration: &Registration) -> String {
    registration
        .activity_epoch
        .map_or_else(|| PREFIX.to_owned(), |epoch| format!("{PREFIX}{epoch}/"))
}

fn card_key(registration: &Registration) -> String {
    format!("{}{}", card_prefix(registration), EndpointId::random())
}

#[derive(Default)]
pub(super) struct Cards {
    separate: HashSet<String>,
    host: Option<Host>,
    /// Only adopt a registered card on initial discovery. Delivery of a previous
    /// end and the phone’s removal of its token can trail the next start.
    saw_host: bool,
    pacer: Pacer,
}

struct Host {
    key: String,
    running: HashMap<String, (Attributes, ContentState)>,
    completed: usize,
    latest: Option<String>,
}

impl Cards {
    /// After a daemon restart the phone still has its card. Rebuild its
    /// live members before processing the first completion.
    pub(super) fn restore(
        &mut self,
        registration: &Registration,
        active: impl FnOnce() -> Vec<(Attributes, ContentState)>,
    ) {
        if self.saw_host {
            return;
        }
        let prefix = card_prefix(registration);
        let existing = registration
            .activities
            .iter()
            .find(|a| a.pane.starts_with(&prefix));
        let restored = existing.map(|a| a.pane.clone()).or_else(|| {
            // The phone binds a fleet card's token directly to the relay.
            // Its reserved recipient is enough to reconstruct this host.
            registration
                .activity(SHARED_RECIPIENT)
                .map(|_| format!("{prefix}restored-{}", EndpointId::random()))
        });
        if let Some(key) = restored {
            let mut host = Host::new(key);
            for (facts, state) in active() {
                if registration.activity(&facts.pane).is_none() {
                    host.remember(facts, state, false);
                }
            }
            self.host = Some(host);
            self.saw_host = true;
        }
    }

    pub(super) fn offer(
        &mut self,
        live: &Live,
        registration: &Registration,
        fallback: impl FnOnce() -> Option<Attributes>,
        now: u64,
    ) -> Option<Live> {
        let start = matches!(live.event, Event::Start { .. });
        let end = matches!(live.event, Event::End { .. });
        let tracked = self
            .host
            .as_ref()
            .is_some_and(|host| host.running.contains_key(&live.pane));
        // A change of preference takes effect on new runs. An existing
        // pane keeps its card through its end, including across token delays.
        if !tracked
            && (self.separate.contains(&live.pane)
                || (!start && registration.activity(&live.pane).is_some())
                || !registration.single_activity)
        {
            if start {
                self.separate.insert(live.pane.clone());
            }
            if end {
                self.separate.remove(&live.pane);
            }
            return Some(live.clone());
        }
        if !start && !tracked && (end || self.host.is_none()) {
            return None;
        }
        let facts = match &live.event {
            Event::Start { attributes, .. } => attributes.clone(),
            _ => self
                .host
                .as_ref()
                .and_then(|host| host.running.get(&live.pane))
                .map(|(facts, _)| facts.clone())
                .or_else(fallback)?,
        };
        let fresh = self.host.is_none();
        self.saw_host = true;
        let host = self
            .host
            .get_or_insert_with(|| Host::new(card_key(registration)));
        let result = host.step(live, facts, fresh);
        if host.running.is_empty() {
            self.host = None;
        }
        self.pacer.offer(result, now)
    }

    /// Private ends the phone's existing cards, even when work is still
    /// running. Give its replacement a new token route and retain progress.
    pub(super) fn replace_reset(
        &mut self,
        registration: &Registration,
        now: u64,
    ) -> Option<(String, Live)> {
        let epoch = registration.activity_epoch?;
        let host = self.host.as_mut()?;
        if host.key.starts_with(&format!("{PREFIX}{epoch}/")) {
            return None;
        }
        let (facts, state) = host
            .running
            .values()
            .min_by_key(|(facts, state)| (state.phase != Phase::Question, &facts.pane))?
            .clone();
        let old = std::mem::replace(&mut host.key, card_key(registration));
        let start = Live {
            pane: facts.pane.clone(),
            state,
            event: Event::Start {
                alert: crate::apns::Alert {
                    title: facts.computer.clone(),
                    body: "Open Sheprd for details".into(),
                },
                attributes: facts.clone(),
            },
        };
        // The new card has its own ActivityKit token and pacing window.
        self.pacer = Pacer::default();
        let replacement = host.step(&start, facts, true);
        self.pacer.offer(replacement, now).map(|live| (old, live))
    }

    pub(super) fn due(&mut self, now: u64) -> Vec<Live> {
        self.pacer.due(now)
    }

    pub(super) fn missing(&self, present: impl Fn(&str) -> bool, now: u64) -> Vec<Live> {
        self.host
            .iter()
            .flat_map(|host| &host.running)
            .filter(|(pane, _)| !present(pane))
            .map(|(pane, (_, state))| {
                let mut state = state.clone();
                state.phase = Phase::Failed;
                state.permission = None;
                state.options.clear();
                state.question = None;
                state.summary = None;
                Live {
                    pane: pane.clone(),
                    state,
                    event: Event::End {
                        dismissal_s: now / 1000,
                    },
                }
            })
            .collect()
    }

    pub(super) fn forget_missing(&mut self, present: impl Fn(&str) -> bool) {
        self.separate.retain(|pane| present(pane));
    }
}

impl Host {
    fn new(key: String) -> Self {
        Self {
            key,
            running: HashMap::new(),
            completed: 0,
            latest: None,
        }
    }

    fn remember(&mut self, facts: Attributes, mut state: ContentState, asked: bool) {
        if state.phase == Phase::Question {
            let prior = self
                .running
                .get(&facts.pane)
                .map(|(_, state)| state)
                .filter(|old| {
                    old.phase == Phase::Question
                        && (state.permission.is_none() || old.started_at == state.started_at)
                        && old.permission == state.permission
                        && old.question == state.question
                        && (state.permission.is_some() || !asked)
                })
                .and_then(|old| old.detail.question_revision);
            // Generate an opaque revision before redaction. It reveals neither
            // the permission id nor the text, including in the Private style.
            state.detail.question_revision =
                Some(prior.unwrap_or_else(crate::apns::relayed::revision));
        }
        self.running.insert(facts.pane.clone(), (facts, state));
    }

    fn step(&mut self, live: &Live, facts: Attributes, fresh: bool) -> Live {
        let ended = matches!(live.event, Event::End { .. });
        let progress = live.state.detail.agents.as_ref();
        let previous = self
            .running
            .get(&live.pane)
            .and_then(|(_, state)| state.detail.agents.as_ref());
        if let Some(label) = progress.and_then(|p| p.latest.as_ref())
            && previous.and_then(|p| p.latest.as_ref()) != Some(label)
        {
            self.latest = Some(label.clone());
        }
        if ended {
            self.running.remove(&live.pane);
            self.completed += progress.map_or(1, |p| p.total);
            if progress.is_none() {
                let outcome = if live.state.phase == Phase::Failed {
                    "Stopped"
                } else {
                    "Finished"
                };
                let name = if facts.chat_title.is_empty() {
                    &facts.agent
                } else {
                    &facts.chat_title
                };
                self.latest = Some(crate::push::excerpt(&format!("{outcome} · {name}")));
            }
        } else {
            let asked = matches!(
                live.event,
                Event::Start { .. } | Event::Update { alert: Some(_) }
            );
            self.remember(facts.clone(), live.state.clone(), asked);
        }
        // Keep a pending question actionable when a different pane reports
        // ordinary work, and switch away from finished panes when possible.
        let active = self
            .running
            .values()
            .filter(|(_, state)| state.phase == Phase::Question)
            .min_by_key(|(facts, _)| &facts.pane)
            .or_else(|| self.running.get(&live.pane))
            .or_else(|| self.running.values().min_by_key(|(facts, _)| &facts.pane));
        let (focus, mut state) = active
            .cloned()
            .unwrap_or_else(|| (facts.clone(), live.state.clone()));
        state.detail.focus = Some(focus);
        state.waiting = live.state.waiting;
        state.working = live.state.working;
        let mut done = self.completed;
        let mut total = done;
        for (_, state) in self.running.values() {
            let progress = state.detail.agents.as_ref();
            done += progress.map_or(0, |p| p.done);
            total += progress.map_or(1, |p| p.total);
        }
        if total > 1 || progress.is_some() || self.latest.is_some() {
            state.detail.agents = Some(AgentProgress {
                done,
                total,
                latest: self.latest.clone(),
            });
        }
        let event = if fresh && let Event::Start { alert, .. } = &live.event {
            let mut attributes = facts;
            attributes.pane.clone_from(&self.key);
            Event::Start {
                alert: alert.clone(),
                attributes,
            }
        } else if self.running.is_empty() {
            live.event.clone()
        } else {
            let alert = match &live.event {
                Event::Update { alert } => alert.clone(),
                Event::Start { alert, .. } if live.urgent() => Some(alert.clone()),
                _ => None,
            };
            Event::Update { alert }
        };
        Live {
            pane: self.key.clone(),
            event,
            state,
        }
    }
}
