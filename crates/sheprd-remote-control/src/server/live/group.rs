//! One Live Activity belongs to the turn that delegated the work. Child
//! permissions stay in the child's own chat and ordinary notification flow.
use crate::{
    apns::{self, AgentProgress, ContentState, Event, Live, Phase},
    hub::Layout,
    push::excerpt,
};
use sheprd_chat_protocol::{
    Activity, ActivityKind, ActivityState, EndpointId, Transcript, TurnEnd, brief::Delegation,
};
use std::collections::HashMap;

/// Keep completed agents through the parent's turn even if a child pane closes.
const MAX_MEMBERS: usize = 256;

#[derive(Clone, Eq, Hash, PartialEq)]
pub(super) enum Member {
    Chat(EndpointId),
    Call(EndpointId, String),
}

pub(super) struct Group {
    pub(super) owner: Delegation,
    first_started_ms: u64,
    pending: HashMap<EndpointId, u64>,
    pane: String,
    state: ContentState,
    end: Option<Event>,
    members: HashMap<Member, (ActivityState, String)>,
    latest: Option<String>,
}

impl Group {
    pub(super) fn new(owner: Delegation, pane: &str, state: ContentState) -> Self {
        Self {
            first_started_ms: owner.started_ms,
            pending: HashMap::new(),
            owner,
            pane: pane.to_owned(),
            state,
            end: None,
            members: HashMap::new(),
            latest: None,
        }
    }

    pub(super) fn pane(&self) -> &str {
        &self.pane
    }

    pub(super) fn record(&mut self, key: Member, status: ActivityState, name: &str) -> bool {
        if let Member::Chat(child) = &key {
            self.pending.remove(child);
        }
        if self
            .members
            .get(&key)
            .is_some_and(|(previous, _)| *previous == status)
            || (self.members.len() >= MAX_MEMBERS && !self.members.contains_key(&key))
        {
            return false;
        }
        self.members.insert(key, (status, excerpt(name)));
        let outcome = match status {
            ActivityState::Finished => "Finished",
            ActivityState::Failed => "Failed",
            ActivityState::Killed => "Stopped",
            ActivityState::Running => return true,
        };
        self.latest = Some(excerpt(&format!("{outcome} · {name}")));
        true
    }

    pub(super) fn owns(&self, owner: &Delegation) -> bool {
        self.owner.endpoint == owner.endpoint
            && (self.first_started_ms..=self.owner.started_ms).contains(&owner.started_ms)
    }

    pub(super) fn launch(
        &mut self,
        child: &EndpointId,
        label: Option<&str>,
        deadline: u64,
    ) -> bool {
        let key = Member::Chat(child.clone());
        match label {
            Some(label) if !self.members.contains_key(&key) => {
                if self.record(key, ActivityState::Running, label) {
                    self.pending.insert(child.clone(), deadline);
                    return true;
                }
            }
            None if self.pending.contains_key(child) => {
                if let Some((_, name)) = self.members.get(&key).cloned() {
                    return self.record(key, ActivityState::Failed, &name);
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn includes(&self, owner: &Delegation, child: &EndpointId) -> bool {
        self.owns(owner) || self.members.contains_key(&Member::Chat(child.clone()))
    }

    /// A follow-up on the coordinator reuses its still-open activity while
    /// delegated workers finish. Tokens and reply links are scoped to the pane.
    pub(super) fn resume(&mut self, started_ms: u64) -> bool {
        if self.end.take().is_none() {
            return false;
        }
        self.owner.started_ms = started_ms;
        true
    }

    pub(super) fn tracks_call(&self, source: &EndpointId, activity: &Activity) -> bool {
        self.members
            .contains_key(&Member::Call(source.clone(), activity.call_id.clone()))
    }

    pub(super) fn activity(&mut self, source: &EndpointId, activity: &Activity) -> bool {
        activity.kind == ActivityKind::Subagent
            && (activity.started_ms >= self.first_started_ms || self.tracks_call(source, activity))
            && self.record(
                Member::Call(source.clone(), activity.call_id.clone()),
                activity.state,
                &activity.label,
            )
    }

    pub(super) fn parent(&mut self, live: Live) -> Live {
        self.state = live.state;
        if matches!(live.event, Event::End { .. }) {
            self.end = Some(live.event);
            return self.render(Event::Update { alert: None });
        }
        self.render(live.event)
    }

    pub(super) fn update(&self) -> Live {
        let mut live = self.render(Event::Update { alert: None });
        if let Event::End { dismissal_s } = &mut live.event {
            // The parent ended earlier. Linger from the last worker's update.
            *dismissal_s = crate::web::now_ms() / 1000 + apns::LINGER_S;
        }
        live
    }

    fn render(&self, event: Event) -> Live {
        let mut state = self.state.clone();
        let total = self.members.len();
        let done = self
            .members
            .values()
            .filter(|(status, _)| *status != ActivityState::Running)
            .count();
        if total > 0 {
            state.detail.agents = Some(AgentProgress {
                done,
                total,
                latest: self.latest.clone(),
            });
        }
        let mut event = event;
        if let Some(end) = &self.end {
            if done == total {
                event = end.clone();
            } else {
                state.phase = Phase::Working;
                state.step = "Waiting for agents".into();
                state.detail.activity = Some(state.step.clone());
                state.summary = None;
            }
        }
        Live {
            pane: self.pane.clone(),
            state,
            event,
        }
    }

    /// A closed delegated chat may disappear before its final turn event.
    pub(super) fn close_missing(&mut self, layout: &Layout, now: u64) -> bool {
        let mut changed = false;
        for (member, (status, name)) in &mut self.members {
            let missing = match member {
                Member::Chat(child) if self.pending.contains_key(child) => {
                    self.pending[child] <= now
                }
                Member::Chat(source) | Member::Call(source, _) => {
                    !layout.chats().any(|row| &row.endpoint == source)
                }
            };
            if *status == ActivityState::Running && missing {
                if let Member::Chat(child) = member {
                    self.pending.remove(child);
                }
                *status = ActivityState::Killed;
                self.latest = Some(excerpt(&format!("Stopped · {name}")));
                changed = true;
            }
        }
        changed
    }
}

pub(super) fn outcome(end: TurnEnd) -> ActivityState {
    match end {
        TurnEnd::Done | TurnEnd::Other => ActivityState::Finished,
        TurnEnd::Cancelled | TurnEnd::Limit => ActivityState::Killed,
        TurnEnd::Refused | TurnEnd::Failed => ActivityState::Failed,
    }
}

pub(super) fn seed(endpoint: &EndpointId, pane: &str, transcript: &Transcript) -> Option<Group> {
    let started_ms = transcript.turn?.started_ms;
    let owner = Delegation {
        endpoint: endpoint.clone(),
        started_ms,
    };
    let mut group = Group::new(owner, pane, apns::current(transcript));
    for activity in &transcript.activities {
        group.activity(endpoint, activity);
    }
    Some(group)
}
