//! A chat's conversation across the agents that hold it.
//!
//! A chat may switch agents whenever its agent is idle. The conversation
//! stays in the transcript; the agent taking over gets a handoff
//! ([`sheprd_chat_protocol::handoff`]) with the user's next prompt. An agent
//! switched back to resumes its own session and hears only what happened
//! since it last took part.

use std::collections::HashMap;

use sheprd_chat_protocol::{
    Agent, Entry, EntryId, EntryKind, Guest,
    handoff::{self, Handoff, Scope},
    thread::file::Trimmed,
};

/// How the agent starting now takes over the conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Joining {
    /// It has never seen it: it gets the whole handoff.
    Fresh,
    /// It continues its own session, which saw everything up to `seen`.
    Resume { seen: EntryId },
}

/// Where an agent left the conversation: its session, and the last entry
/// it saw.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Left {
    seen: EntryId,
    session: String,
}

#[derive(Debug, Default)]
pub(super) struct Thread {
    /// The agent that held the conversation before the current one.
    before: Option<Agent>,
    /// The conversation's first prompt, kept for handoffs once the
    /// transcript drops its oldest entries.
    first_prompt: Option<String>,
    /// The guest who wrote the first prompt, if the person did not.
    first_guest: Option<Guest>,
    /// The text sent ahead of the next prompt, for the agent that took over.
    handoff: Option<String>,
    /// The handoff the turn under way carried, until the agent answers: one
    /// it rejects goes with the next prompt again.
    in_flight: Option<String>,
    /// The agents that held the conversation, oldest first, each once.
    held: Vec<Agent>,
    /// How the agent starting now takes over; `None` for the chat's first.
    joining: Option<Joining>,
    /// Where each agent switched away from left the conversation.
    left: HashMap<Agent, Left>,
    /// A resumed session replays its own history, which the transcript
    /// already shows: its updates are ignored until it opens.
    replaying: bool,
    /// What the current agent knows of the conversation while its handoff
    /// has not reached it yet; `None` once a turn carrying it was taken (or
    /// for the chat's first agent), when it has seen the whole transcript.
    undelivered: Option<Joining>,
}

impl Thread {
    /// The thread of a chat started with `agent`, continuing the
    /// conversation `entries` already show, if any.
    pub(super) fn new(agent: Option<Agent>, entries: &[Entry]) -> Self {
        let (first_prompt, first_guest) = entries
            .iter()
            .find_map(|entry| match &entry.kind {
                EntryKind::User { text, guest, .. } => Some((Some(text.clone()), guest.clone())),
                _ => None,
            })
            .unwrap_or_default();
        Self {
            first_prompt,
            first_guest,
            held: agent.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Whether a prompt has reached an agent: only then is there a
    /// conversation to hand over.
    pub(super) fn started(&self) -> bool {
        self.first_prompt.is_some()
    }

    /// A prompt carrying `handoff` went to the agent.
    pub(super) fn prompted(&mut self, shown: &str, guest: Option<&Guest>, handoff: Option<String>) {
        self.in_flight = handoff;
        if self.first_prompt.is_none() {
            self.first_prompt = Some(shown.to_owned());
            self.first_guest = guest.cloned();
        }
    }

    /// `from`, whose session is `session`, leaves for `to` with the
    /// transcript ending at `seen`. Returns the session `to` should
    /// continue, if it took part before.
    pub(super) fn switch(
        &mut self,
        from: Option<(Agent, String)>,
        seen: Option<EntryId>,
        to: Agent,
    ) -> Option<String> {
        self.handoff = None;
        self.in_flight = None;
        if !self.started() {
            // Nothing to hand over: the new agent simply starts.
            self.joining = None;
            self.replaying = false;
            return None;
        }
        // An agent whose handoff never reached it saw only what it knew on
        // joining; one that joined fresh saw nothing, so it is handed the
        // whole conversation again rather than resumed.
        let seen = match self.undelivered.take() {
            None => seen,
            Some(Joining::Resume { seen }) => Some(seen),
            Some(Joining::Fresh) => None,
        };
        if let (Some((agent, session)), Some(seen)) = (from, seen)
            && sheprd_chat_protocol::restore::session_id_is_plain(&session)
        {
            self.left.insert(agent, Left { seen, session });
        }
        if !self.held.contains(&to) {
            self.held.push(to);
        }
        // The record stays until the session opens, so a launch that fails
        // can still resume it later.
        let joining = match self.left.get(&to) {
            Some(left) => Joining::Resume { seen: left.seen },
            None => Joining::Fresh,
        };
        self.undelivered = Some(joining);
        match self.left.get(&to) {
            Some(left) => {
                self.joining = Some(joining);
                self.replaying = true;
                Some(left.session.clone())
            }
            None => {
                self.joining = Some(Joining::Fresh);
                self.replaying = false;
                None
            }
        }
    }

    pub(super) fn replaying(&self) -> bool {
        self.replaying
    }

    /// The resumed session could not be continued; the agent starts a new
    /// one and needs the whole conversation.
    pub(super) fn resume_failed(&mut self) {
        if self.joining.is_some() {
            self.joining = Some(Joining::Fresh);
            self.undelivered = Some(Joining::Fresh);
        }
        self.replaying = false;
    }

    /// The agent that took over, `agent`, opened its session (`loaded` when
    /// it continued one). Prepares its handoff from `entries`, and returns
    /// how much it carries, for the chat to say.
    pub(super) fn opened(
        &mut self,
        agent: Agent,
        loaded: bool,
        entries: &[Entry],
        tool: bool,
        trimmed: &[Trimmed],
    ) -> Option<(Joining, Handoff)> {
        self.replaying = false;
        let joining = match self.joining.take()? {
            Joining::Resume { seen } if loaded => Joining::Resume { seen },
            _ => Joining::Fresh,
        };
        // Its earlier session is continued or replaced now.
        self.left.remove(&agent);
        self.undelivered = Some(joining);
        let others: Vec<Agent> = self
            .held
            .iter()
            .copied()
            .filter(|held| *held != agent)
            .collect();
        let built = match joining {
            Joining::Fresh => handoff::build_with(
                entries,
                Scope::Full {
                    first_prompt: self.first_prompt.as_deref(),
                    first_guest: self.first_guest.as_ref(),
                },
                &others,
                handoff::BUDGET,
                handoff::Reading {
                    tool,
                    trimmed: count(trimmed, None),
                },
            ),
            Joining::Resume { seen } => {
                let since = entries.partition_point(|entry| entry.id <= seen);
                // What it saw before it left, it still holds: only what was
                // trimmed since is gone for it.
                let reading = handoff::Reading {
                    tool,
                    trimmed: count(trimmed, Some(seen)),
                };
                handoff::build_with(
                    &entries[since..],
                    Scope::Since,
                    &others,
                    handoff::BUDGET,
                    reading,
                )
            }
        };
        let Some(built) = built else {
            // Nothing to hand over: it already knows the conversation.
            self.undelivered = None;
            return None;
        };
        self.handoff = Some(built.text.clone());
        Some((joining, built))
    }

    /// The agent answered the turn under way; `failed` when it rejected
    /// it, so the handoff the turn carried goes with the next prompt.
    pub(super) fn turn_ended(&mut self, failed: bool) {
        let in_flight = self.in_flight.take();
        if failed {
            if let Some(text) = in_flight {
                self.handoff.get_or_insert(text);
            }
        } else if self.handoff.is_none() {
            // The handoff, if any, reached it: the agent is caught up.
            self.undelivered = None;
        }
    }

    /// The agent that held the conversation before the current one.
    pub(super) fn before(&self) -> Option<Agent> {
        self.before
    }

    /// `agent` leaves the conversation for another.
    pub(super) fn leaving(&mut self, agent: Option<Agent>) {
        self.before = agent;
    }

    /// The handoff to send ahead of the next prompt, once.
    pub(super) fn take_handoff(&mut self) -> Option<String> {
        self.handoff.take()
    }

    /// A prompt carrying `text` did not reach the agent: the next one
    /// carries it instead.
    pub(super) fn restore_handoff(&mut self, text: String) {
        self.handoff = Some(text);
    }
}

/// How many ids `trimmed` holds, only those after `after` when given.
fn count(trimmed: &[Trimmed], after: Option<EntryId>) -> usize {
    trimmed
        .iter()
        .filter_map(|range| {
            let from = after.map_or(range.from.0, |after| range.from.0.max(after.0 + 1));
            (from <= range.to.0).then(|| range.to.0 - from + 1)
        })
        .map(|ids| usize::try_from(ids).unwrap_or(usize::MAX))
        .fold(0, usize::saturating_add)
}

#[cfg(test)]
mod tests;
