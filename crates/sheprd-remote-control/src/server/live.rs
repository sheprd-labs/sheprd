//! Live Activities for the iOS companion: each chat change the APNs rules
//! call for is paced per chat, then sent to every device that registered
//! tokens: the push-to-start token to start an activity, the activity's own
//! token to update or end it. A token APNs calls invalid, or a handle the
//! relay calls gone, is forgotten. What each device gets is cut to what the
//! computer and that phone allow just before sending, so a held update
//! follows the phone's latest choice; through the relay it is then sealed.
//! Each update is sent once, on its own task: one the relay cannot take is
//! dropped, never queued.

mod group;
mod host;
mod launches;
pub(crate) use launches::Launches;

use super::notifier::waiting;
use crate::{
    apns::{Delivery, Event, Live, Pacer, Recipient, Registration, Sent, decide, decide_terminal},
    control::DeviceId,
    push::{Chat, Moved, SeenAgent},
    web::{State, now_ms},
};
use group::{Group, Member};
use p256::PublicKey;
use sheprd_chat_protocol::{
    ActivityState, Change, EndpointId, EntryKind, Status, Transcript, brief::Delegation,
};
use std::{collections::HashMap, sync::Arc};

/// How long an update waits for the activity's own token, which the phone
/// registers after the activity starts.
const HOLD_MS: u64 = 10 * 60 * 1000;

#[derive(Default)]
pub(super) struct LiveActivities {
    pacer: Pacer,
    groups: HashMap<EndpointId, Group>,
    cards: HashMap<DeviceId, host::Cards>,
    /// The latest update or end for an activity whose token the device has
    /// not registered yet, by device and pane. A short turn can end before
    /// the token arrives; its end must still reach the phone.
    held: HashMap<(DeviceId, String), Held>,
}

struct Held {
    live: Live,
    since_ms: u64,
}

impl LiveActivities {
    /// Considers `change` to the chat in `pane`, whose transcript was
    /// `before`.
    pub(super) fn change(
        &mut self,
        state: &Arc<State>,
        pane: &str,
        before: &Transcript,
        change: &Change,
    ) {
        if state.apns.is_off() {
            return;
        }
        let layout = state.hub.layout();
        let Some((workspace, row)) = layout.chat(pane) else {
            return;
        };
        if let Change::Activity { activity } = change {
            if activity.kind != sheprd_chat_protocol::ActivityKind::Subagent {
                return;
            }
            let known = self.call_owner(&row.endpoint, activity);
            let group = if let Some(owner) = known {
                self.groups.get_mut(&owner)
            } else if let Some((owner, _)) = before.delegated_brief() {
                self.find_parent(state, owner, &row.endpoint)
            } else {
                self.groups.get_mut(&row.endpoint)
            };
            if let Some(group) = group
                && group.activity(&row.endpoint, activity)
            {
                state.launches.deliver(group, now_ms());
                let live = group.update();
                self.offer(state, live);
            }
            return;
        }
        if let Some(task) = &before.delegation
            && let Some(group) = self.find_parent(state, &task.parent, &row.endpoint)
        {
            let status = match change {
                Change::Turn { turn: Some(_) } => ActivityState::Running,
                Change::Push { entry } => match &entry.kind {
                    EntryKind::Turn(summary) => group::outcome(summary.end),
                    _ => return,
                },
                Change::Status {
                    status: Status::Failed { .. },
                } => ActivityState::Failed,
                Change::Turn { turn: None }
                    if !matches!(
                        before.entries.last().map(|e| &e.kind),
                        Some(EntryKind::Turn(_))
                    ) =>
                {
                    ActivityState::Killed
                }
                _ => return,
            };
            let launched = state.launches.deliver(group, now_ms());
            if group.record(Member::Chat(row.endpoint.clone()), status, &task.label) || launched {
                let live = group.update();
                self.offer(state, live);
            }
            return;
        }
        let agent = if before.agent.is_empty() {
            row.agent.as_deref().unwrap_or("The agent")
        } else {
            &before.agent
        };
        let chat = Chat {
            agent,
            pane,
            title: &row.title,
            workspace: &workspace.label,
        };
        let now = now_ms();
        let Some(mut live) = decide(
            &chat,
            &state.computer,
            &state.fingerprint,
            before,
            change,
            now,
        ) else {
            return;
        };
        if let Change::Turn { turn: Some(turn) } = change {
            let owner = Delegation {
                endpoint: row.endpoint.clone(),
                started_ms: turn.started_ms,
            };
            if self
                .groups
                .get_mut(&row.endpoint)
                .is_some_and(|group| group.resume(turn.started_ms))
            {
                live.event = Event::Update { alert: None };
            } else if self
                .groups
                .get(&row.endpoint)
                .is_none_or(|group| group.owner != owner)
            {
                self.groups.insert(
                    row.endpoint.clone(),
                    Group::new(owner, pane, live.state.clone()),
                );
            }
        }
        if !self.groups.contains_key(&row.endpoint)
            && let Some(group) = group::seed(&row.endpoint, pane, before)
        {
            self.groups.insert(row.endpoint.clone(), group);
        }
        let live = match self.groups.get_mut(&row.endpoint) {
            Some(group) => {
                state.launches.deliver(group, now);
                group.parent(live)
            }
            None => live,
        };
        self.offer(state, live);
    }

    fn offer(&mut self, state: &Arc<State>, live: Live) {
        if matches!(live.event, Event::End { .. }) {
            self.groups.retain(|_, group| group.pane() != live.pane);
        }
        if let Some(live) = self.pacer.offer(live, now_ms()) {
            self.dispatch(state, live);
        }
    }

    fn call_owner(
        &self,
        source: &EndpointId,
        activity: &sheprd_chat_protocol::Activity,
    ) -> Option<EndpointId> {
        self.groups
            .iter()
            .find(|(_, group)| group.tracks_call(source, activity))
            .map(|(owner, _)| owner.clone())
    }

    fn find_parent(
        &mut self,
        state: &State,
        owner: &Delegation,
        child: &EndpointId,
    ) -> Option<&mut Group> {
        if !self.groups.contains_key(&owner.endpoint) {
            let layout = state.hub.layout();
            let row = layout.chats().find(|row| row.endpoint == owner.endpoint)?;
            let link = state.hub.chat(&row.pane)?;
            let group = link.read(|t| group::seed(&row.endpoint, &row.pane, t))?;
            self.groups.insert(row.endpoint.clone(), group);
        }
        self.groups
            .get_mut(&owner.endpoint)
            .filter(|g| g.includes(owner, child))
    }

    /// A fast delegated turn can already be running or finished when the
    /// notifier first subscribes to its chat.
    pub(super) fn snapshot(&mut self, state: &Arc<State>, pane: &str, transcript: &Transcript) {
        if state.apns.is_off() {
            return;
        }
        let layout = state.hub.layout();
        let Some((_, row)) = layout.chat(pane) else {
            return;
        };
        let source = &row.endpoint;
        let delegated = transcript.delegation.as_ref().and_then(|task| {
            self.find_parent(state, &task.parent, source)
                .map(|_| (&task.parent.endpoint, task.label.as_str()))
        });
        if delegated.is_none()
            && let Some(turn) = transcript.turn
        {
            // A snapshot can be our first observation of this turn.
            let resume = self.groups.get_mut(source).is_some_and(|group| {
                group.owner.started_ms == turn.started_ms || group.resume(turn.started_ms)
            });
            if !resume {
                let owner = Delegation {
                    endpoint: source.clone(),
                    started_ms: turn.started_ms,
                };
                self.groups.insert(
                    source.clone(),
                    Group::new(owner, pane, crate::apns::current(transcript)),
                );
            }
        }
        let mut changed = std::collections::HashSet::new();
        for activity in &transcript.activities {
            let owner = self
                .call_owner(source, activity)
                .or_else(|| delegated.map(|(owner, _)| owner.clone()))
                .unwrap_or_else(|| source.clone());
            if let Some(group) = self.groups.get_mut(&owner)
                && group.activity(source, activity)
            {
                changed.insert(owner);
            }
        }
        if let Some((owner, label)) = delegated
            && let Some(group) = self.groups.get_mut(owner)
        {
            let status = transcript
                .entries
                .iter()
                .rev()
                .take_while(|e| !matches!(e.kind, EntryKind::User { .. }))
                .find_map(|e| match &e.kind {
                    EntryKind::Turn(t) => Some(group::outcome(t.end)),
                    _ => None,
                })
                .or_else(|| {
                    matches!(transcript.status, Status::Failed { .. })
                        .then_some(ActivityState::Failed)
                })
                .or_else(|| transcript.turn.map(|_| ActivityState::Running));
            if let Some(status) = status
                && group.record(Member::Chat(source.clone()), status, label)
            {
                changed.insert(owner.clone());
            }
        }
        for owner in changed {
            if let Some(group) = self.groups.get_mut(&owner) {
                state.launches.deliver(group, now_ms());
                let live = group.update();
                self.offer(state, live);
            }
        }
    }

    /// Considers a settled status change of a terminal agent; `agent` is it
    /// as the layout shows it now, `None` once its pane left.
    pub(super) fn terminal(
        &mut self,
        state: &Arc<State>,
        moved: &Moved,
        agent: Option<&SeenAgent<'_>>,
        now: u64,
    ) {
        if state.apns.is_off() {
            return;
        }
        let Some(live) = decide_terminal(moved, agent, &state.computer, &state.fingerprint, now)
        else {
            return;
        };
        if let Some(live) = self.pacer.offer(live, now) {
            self.dispatch(state, live);
        }
    }

    /// Sends the updates the pacer held back whose time came.
    pub(super) fn tick(&mut self, state: &Arc<State>) {
        if state.apns.is_off() {
            return;
        }
        let layout = state.hub.layout();
        let mut updates = Vec::new();
        self.groups.retain(|endpoint, group| {
            let present = layout
                .chat(group.pane())
                .is_some_and(|(_, row)| &row.endpoint == endpoint);
            if present {
                let launched = state.launches.deliver(group, now_ms());
                if group.close_missing(&layout, now_ms()) || launched {
                    updates.push(group.update());
                }
            }
            present
        });
        for live in updates {
            self.offer(state, live);
        }
        for live in self.pacer.due(now_ms()) {
            self.dispatch(state, live);
        }
        let now = now_ms();
        let devices = registrations(state);
        self.cards
            .retain(|device, _| devices.iter().any(|(id, _)| id == device));
        for (device, registration) in devices {
            let Some(cards) = self.cards.get_mut(&device) else {
                continue;
            };
            let mut updates = Vec::new();
            if let Some((old, start)) = cards.replace_reset(&registration, now) {
                self.held.remove(&(device.clone(), old));
                updates.push(start);
            }
            updates.extend(cards.due(now));
            if layout.connected {
                let present =
                    |pane: &str| layout.chat(pane).is_some() || layout.terminal(pane).is_some();
                cards.forget_missing(present);
                for ended in cards.missing(present, now) {
                    updates.extend(cards.offer(&ended, &registration, || None, now));
                }
            }
            for live in updates {
                self.deliver_to(state, &device, &registration, live, now);
            }
        }
        self.release(state);
    }

    /// Sends `live` to every device with a token for it, and holds it for
    /// devices whose activity token has not arrived.
    fn dispatch(&mut self, state: &Arc<State>, mut live: Live) {
        live.state.waiting = waiting(state);
        live.state.working = working(state);
        let now = now_ms();
        for (device, registration) in registrations(state) {
            if registration.activities.is_empty()
                && !registration
                    .push_to_start
                    .as_ref()
                    .is_some_and(|t| state.apns.reaches(t))
            {
                self.cards.remove(&device);
                continue;
            }
            let cards = self.cards.entry(device.clone()).or_default();
            cards.restore(&registration, || {
                let mut active = current_panes(state, &self.groups, now);
                // The first event after restart may already have removed the
                // finished turn from its transcript (or its terminal from layout).
                if registration.activity(host::SHARED_RECIPIENT).is_some()
                    && !matches!(live.event, Event::Start { .. })
                    && !active.iter().any(|(facts, _)| facts.pane == live.pane)
                {
                    let facts =
                        pane_facts(state, &live.pane).unwrap_or_else(|| crate::apns::Attributes {
                            agent: "Agent".into(),
                            chat_title: String::new(),
                            computer: state.computer.clone(),
                            computer_id: crate::apns::computer_id(&state.fingerprint),
                            workspace: state.computer.clone(),
                            pane: live.pane.clone(),
                            terminal: true,
                        });
                    active.push((facts, live.state.clone()));
                }
                active
            });
            let replacement = cards.replace_reset(&registration, now);
            let next = cards.offer(&live, &registration, || pane_facts(state, &live.pane), now);
            if let Some((old, start)) = replacement {
                self.held.remove(&(device.clone(), old));
                self.deliver_to(state, &device, &registration, start, now);
            }
            if let Some(live) = next {
                self.deliver_to(state, &device, &registration, live, now);
            }
        }
    }

    fn deliver_to(
        &mut self,
        state: &Arc<State>,
        device: &DeviceId,
        registration: &Registration,
        live: Live,
        now: u64,
    ) {
        let key = (device.clone(), live.pane.clone());
        self.held.remove(&key);
        // Older hosts ignore this reserved activity recipient and use their
        // ordinary push-to-start handle. No shared update token leaves iOS.
        if live.pane.starts_with("host-activity/")
            && let Some(token) = registration.activity(host::SHARED_RECIPIENT)
        {
            let live = shown(state, live, registration);
            let token = token.clone();
            let key = notify_key(registration);
            let state = state.clone();
            let revision = crate::apns::relayed::revision();
            tokio::spawn(async move {
                let sent = state
                    .apns
                    .shared(&token, key.as_ref(), &live, revision, now)
                    .await;
                state
                    .pushes
                    .record(crate::apns::Kind::LiveActivity, &sent, now_ms());
            });
            return;
        }
        match route(&live, registration, &state.apns) {
            Route::Send(token) => send(
                state,
                key.0,
                token.clone(),
                notify_key(registration),
                shown(state, live, registration),
            ),
            Route::Hold => {
                self.held.insert(
                    key,
                    Held {
                        live,
                        since_ms: now,
                    },
                );
            }
            Route::Skip => {}
        }
    }

    /// Sends what was held to the devices whose activity token arrived, and
    /// drops what waited too long.
    fn release(&mut self, state: &Arc<State>) {
        if self.held.is_empty() {
            return;
        }
        let now = now_ms();
        let registrations = registrations(state);
        let (waiting, working) = (waiting(state), working(state));
        self.held.retain(|(device, pane), held| {
            if now.saturating_sub(held.since_ms) > HOLD_MS {
                return false;
            }
            let found = registrations
                .iter()
                .find(|(registered, _)| registered == device)
                .and_then(|(_, registration)| {
                    let token = registration.activity(pane)?;
                    state.apns.reaches(token).then_some((registration, token))
                });
            let Some((registration, token)) = found else {
                return true;
            };
            let mut live = held.live.clone();
            live.state.waiting = waiting;
            live.state.working = working;
            send(
                state,
                device.clone(),
                token.clone(),
                notify_key(registration),
                shown(state, live, registration),
            );
            false
        });
    }
}

/// The currently running coordinators and terminal agents on a restarted host.
fn current_panes(
    state: &State,
    groups: &HashMap<EndpointId, Group>,
    now: u64,
) -> Vec<(crate::apns::Attributes, crate::apns::ContentState)> {
    let layout = state.hub.layout();
    let mut active: Vec<_> = layout
        .chats()
        .filter_map(|chat| {
            let content = groups
                .get(&chat.endpoint)
                .map(|g| g.update().state)
                .or_else(|| {
                    state.hub.chat(&chat.pane)?.read(|t| {
                        if t.delegation.is_some() {
                            return None;
                        }
                        group::seed(&chat.endpoint, &chat.pane, t).map(|g| g.update().state)
                    })
                })?;
            Some((pane_facts(state, &chat.pane)?, content))
        })
        .collect();
    let names = super::notifier::agent_names(&layout);
    for agent in super::notifier::terminal_agents(&layout, &names) {
        if !crate::push::is_running(agent.status) {
            continue;
        }
        let moved = Moved {
            pane: agent.chat.pane.into(),
            from: sheprd_client::protocol::AgentStatus::Idle,
            to: Some(agent.status),
            started_ms: None,
        };
        if let Some(live) = decide_terminal(
            &moved,
            Some(&agent),
            &state.computer,
            &state.fingerprint,
            now,
        ) && let Event::Start { attributes, .. } = live.event
        {
            active.push((attributes, live.state));
        }
    }
    active
}

/// Rebuild the focused pane after reconnecting to an already registered card.
fn pane_facts(state: &State, pane: &str) -> Option<crate::apns::Attributes> {
    use crate::push::excerpt;
    let layout = state.hub.layout();
    let (workspace, title, agent, terminal) = if let Some((ws, row)) = layout.chat(pane) {
        (&ws.label, &row.title, row.agent.as_deref(), false)
    } else {
        let (ws, tab) = layout.terminal(pane)?;
        (
            &ws.label,
            &tab.label,
            tab.agents
                .iter()
                .find(|a| a.pane == pane)
                .and_then(|a| a.name.as_deref()),
            true,
        )
    };
    Some(crate::apns::Attributes {
        workspace: excerpt(workspace),
        chat_title: excerpt(title),
        agent: excerpt(agent.unwrap_or("Agent")),
        pane: pane.to_owned(),
        terminal,
        computer: excerpt(&state.computer),
        computer_id: crate::apns::computer_id(&state.fingerprint),
    })
}

/// `live` as the device behind `registration` may see it.
fn shown(state: &State, live: Live, registration: &Registration) -> Live {
    let detail = state.live_activities.stricter(registration.activity_detail);
    live.redacted(state.privacy, detail)
}

/// The key the phone behind `registration` opens sealed content with.
fn notify_key(registration: &Registration) -> Option<PublicKey> {
    registration.notify_key.as_ref().map(|key| *key.key())
}

/// What to do with an update for one device.
#[derive(Debug, Eq, PartialEq)]
enum Route<'a> {
    Send(&'a Recipient),
    /// Wait for the activity's own token.
    Hold,
    Skip,
}

/// A start goes to the push-to-start token; an update or end to the
/// activity's token, or waits for it on a device that can start
/// activities, which may be showing one whose token is on its way. Only
/// recipients `delivery` reaches count: the push relay's handles, never a
/// plain token.
fn route<'a>(live: &Live, registration: &'a Registration, delivery: &Delivery) -> Route<'a> {
    let reached = |token: Option<&'a Recipient>| token.filter(|token| delivery.reaches(token));
    let starts = reached(registration.push_to_start.as_ref()).is_some();
    let token = match live.event {
        Event::Start { .. } => reached(registration.push_to_start.as_ref()),
        _ => reached(registration.activity(&live.pane)),
    };
    match token {
        Some(token) => Route::Send(token),
        None if starts && !matches!(live.event, Event::Start { .. }) => Route::Hold,
        None => Route::Skip,
    }
}

/// Chats with a turn running now, and terminal agents working, for the
/// Dynamic Island's count.
fn working(state: &State) -> u32 {
    let layout = state.hub.layout();
    let chats = layout
        .chats()
        .filter_map(|chat| state.hub.chat(&chat.pane))
        .filter(|link| link.summary().status == Status::Working)
        .count();
    let terminals = layout
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.terminals)
        .flat_map(|terminal| &terminal.agents)
        .filter(|agent| agent.status == sheprd_client::protocol::AgentStatus::Working)
        .count();
    u32::try_from(chats + terminals).unwrap_or(u32::MAX)
}

/// Every device's non-empty APNs registration, read under one lock: the
/// auth mutex is not reentrant.
pub(super) fn registrations(state: &State) -> Vec<(DeviceId, Registration)> {
    let auth = state.auth();
    let devices = auth.devices();
    devices
        .list(|_| false)
        .into_iter()
        .filter_map(|device| {
            let registration = Registration::from_stored(
                devices
                    .get(&device.id)
                    .and_then(|record| record.apns.as_ref()),
            );
            (!registration.is_empty()).then_some((device.id, registration))
        })
        .collect()
}

/// Sends `live` to `device` with `token`, sealed to `key` through the
/// relay; forgets a token or handle that is gone, and an activity that
/// ended.
fn send(
    state: &Arc<State>,
    device: DeviceId,
    token: Recipient,
    key: Option<PublicKey>,
    live: Live,
) {
    let state = state.clone();
    tokio::spawn(async move {
        let sent = state.apns.live(&token, key.as_ref(), &live, now_ms()).await;
        state
            .pushes
            .record(crate::apns::Kind::LiveActivity, &sent, now_ms());
        let ended = matches!(live.event, Event::End { .. });
        let gone = sent == Sent::Gone;
        if !gone && !ended {
            return;
        }
        let pane = live.pane.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut auth = state.auth();
            let devices = auth.devices_mut();
            let mut registration = Registration::from_stored(
                devices.get(&device).and_then(|record| record.apns.as_ref()),
            );
            let changed = if gone {
                registration.forget(&token)
            } else {
                registration.end(&pane)
            };
            if changed && let Ok(value) = registration.to_stored() {
                let _ = devices.set_apns(&device, value);
            }
        })
        .await;
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
