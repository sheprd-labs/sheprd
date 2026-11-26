//! Prompts steered into the running turn: sent to the harness at once, they
//! wait in the queue marked steered until the agent takes them, and show in
//! the transcript where it did. One the turn ended without goes next, as an
//! ordinary queued prompt. Stopping a turn holds the queue, so nothing typed
//! is lost, and a held or queued prompt can be sent now.

use serde_json::Value;
use sheprd_chat_protocol::{Attachment, CommandId, Origin, Rejection};

use crate::harness::steer;
use crate::session::{Pending, Session, Update};

impl<R: FnMut(Update)> Session<R> {
    /// Whether a prompt typed now can go into the running turn: the harness
    /// steers, a turn runs, and no ordinary prompt waits ahead of it.
    pub(super) fn can_steer(&self) -> bool {
        self.steers_now()
            && self
                .transcript
                .queued
                .iter()
                .all(|queued| queued.steered || queued.held)
    }

    /// Whether the running turn takes a prompt now.
    fn steers_now(&self) -> bool {
        self.can_steer && self.turn.is_some() && self.ready && !self.carrying()
    }

    /// Sends a queued or held prompt ahead of the others: into the running
    /// turn when the agent steers, else in place of it, as the turn stops.
    /// With no turn running, it goes next.
    pub(super) fn send_now(&mut self, id: CommandId) -> Result<(), Rejection> {
        let mut queued = self.transcript.queued.clone();
        let index = queued
            .iter()
            .position(|queued| queued.id == id && !queued.steered)
            .ok_or(Rejection::UnknownQueued)?;
        let mut next = queued.remove(index);
        next.held = false;
        if self.steers_now() {
            let origin = self.queued_origins.get(&id).cloned();
            next.steered = self.steer(id, &next.text, &next.attachments, origin);
        }
        let steered = next.steered;
        queued.insert(0, next);
        self.queue_changed(queued);
        if steered {
            return Ok(());
        }
        if self.turn.is_some() {
            // The turn's end sends it, first.
            return self.stop_turn();
        }
        self.next_queued();
        Ok(())
    }

    /// A stopped turn's queue waits: nothing in it goes until sent now, and
    /// a prompt steered into the turn waits with the rest.
    pub(super) fn hold_queue(&mut self) {
        if self.transcript.queued.iter().all(|queued| queued.held) {
            return;
        }
        let mut queued = self.transcript.queued.clone();
        for waiting in &mut queued {
            waiting.held = true;
            waiting.steered = false;
        }
        self.queue_changed(queued);
    }

    /// Sends the prompt into the running turn; `false` if it could not go.
    pub(super) fn steer(
        &mut self,
        id: CommandId,
        text: &str,
        attachments: &[Attachment],
        origin: Option<Origin>,
    ) -> bool {
        let Some(session) = self.acp_session.as_ref().map(|id| id.0.to_string()) else {
            return false;
        };
        // A prompt steered before goes as it was sent then.
        let outgoing = match self.steering.remove(&id) {
            Some(outgoing) => outgoing,
            None => match self.outgoing(text, attachments, origin, false) {
                Some(outgoing) => outgoing,
                None => return false,
            },
        };
        let request = steer::Steer {
            id: id.0,
            prompt: outgoing.blocks.clone(),
            session_id: session,
        };
        // Kept whether or not it goes: a prompt that could not be steered
        // goes as the next prompt, without telling the agent twice what
        // went ahead of it.
        self.steering.insert(id, outgoing);
        self.request(steer::METHOD, &request, Pending::Steer(id))
    }

    /// The harness's answer to a steered prompt: one it could not take
    /// waits for the next turn instead.
    pub(super) fn steer_answered(&mut self, id: CommandId, result: &Result<Value, acp::v1::Error>) {
        if result.is_err() {
            self.unsteer(id);
        }
    }

    /// The harness says the agent took a steered prompt, or that the turn
    /// ended without it.
    pub(super) fn steer_notified(&mut self, taken: bool, params: Value) {
        let Ok(notified) = serde_json::from_value::<steer::Steered>(params) else {
            return;
        };
        if self
            .acp_session
            .as_ref()
            .is_none_or(|session| *session.0 != *notified.session_id)
        {
            return;
        }
        let id = CommandId(notified.id);
        if taken {
            self.steered(id);
        } else {
            self.unsteer(id);
        }
    }

    /// The agent took the prompt: it leaves the queue and shows in the
    /// conversation, after what the agent said before taking it.
    fn steered(&mut self, id: CommandId) {
        let Some(outgoing) = self.steering.remove(&id) else {
            return;
        };
        let mut queued = self.transcript.queued.clone();
        queued.retain(|queued| queued.id != id);
        if queued.len() != self.transcript.queued.len() {
            self.queue_changed(queued);
        }
        self.show_prompt(
            &outgoing.shown,
            outgoing.files,
            outgoing.sent,
            outgoing.images,
        );
        (self.report)(Update::Prompt(outgoing.shown));
    }

    /// The prompt did not reach the turn: it waits as an ordinary prompt,
    /// sent once the agent is free.
    fn unsteer(&mut self, id: CommandId) {
        let mut queued = self.transcript.queued.clone();
        let Some(waiting) = queued.iter_mut().find(|queued| queued.id == id) else {
            self.steering.remove(&id);
            return;
        };
        if !waiting.steered {
            return;
        }
        waiting.steered = false;
        self.queue_changed(queued);
        self.next_queued();
    }

    /// The turn ended: a prompt still marked steered waits for the next
    /// one.
    pub(super) fn turn_ended_steering(&mut self) {
        if self.transcript.queued.iter().any(|queued| queued.steered) {
            let mut queued = self.transcript.queued.clone();
            for waiting in &mut queued {
                waiting.steered = false;
            }
            self.queue_changed(queued);
        }
    }
}
