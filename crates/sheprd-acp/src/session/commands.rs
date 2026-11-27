//! What clients ask for.

use acp::v1 as wire;
use sheprd_chat_protocol::{
    Attachment, Author, Change, Command, CommandId, MAX_ATTACHMENTS, MAX_PROMPT_SIZE, OptionValue,
    Origin, PermissionId, Queued, Rejection, Status, Turn,
};

use crate::project::STOP_METHOD;
use crate::session::{ConnId, Pending, Session, Update};

/// A prompt ready for the agent, and what the transcript shows of it.
#[derive(Debug)]
pub(super) struct Outgoing {
    pub(super) blocks: Vec<wire::ContentBlock>,
    pub(super) files: usize,
    pub(super) handoff: Option<String>,
    pub(super) images: Vec<sheprd_chat_protocol::Image>,
    pub(super) sent: Option<String>,
    pub(super) shown: String,
}

/// Prompts waiting behind a turn, and their text in bytes: the queue is
/// copied into every change and snapshot, so it stays well inside a frame.
const MAX_QUEUED: usize = 32;
const MAX_QUEUED_TEXT: usize = 1024 * 1024;

impl<R: FnMut(Update)> Session<R> {
    pub(super) fn command(&mut self, conn: ConnId, id: CommandId, command: Command) {
        if let Some((_, rejected)) = self.handled.iter().find(|(handled, _)| *handled == id) {
            let rejected = *rejected;
            return self.send_to(
                conn,
                sheprd_chat_protocol::ServerMessage::Ack { id, rejected },
            );
        }
        let rejected = match command {
            Command::Prompt {
                text,
                attachments,
                origin,
            } => self.prompt(id, text, attachments, origin),
            Command::Cancel => self.cancel(),
            Command::Answer { option, permission } => self.answer(permission, &option),
            Command::AnswerQuestion {
                answers,
                permission,
            } => self.answer_question(permission, answers),
            Command::SetMode { id } => self.set_mode(id),
            Command::SetOption { id, value } => self.set_option(id, &value),
            Command::Unqueue { id } => self.unqueue(id),
            Command::SendNow { id } => self.send_now(id),
            Command::SetAutoAccept { on } => {
                self.set_auto_accept(on);
                Ok(())
            }
            Command::StopActivity { call_id } => self.stop_activity(&call_id),
            Command::SetAgent { agent } => self.set_agent(agent),
            Command::AddPullRequest { url } => self.add_pull_request(&url),
        }
        .err();
        self.ack(conn, id, rejected);
    }

    pub(super) fn prompt(
        &mut self,
        id: CommandId,
        text: String,
        attachments: Vec<Attachment>,
        origin: Option<Origin>,
    ) -> Result<(), Rejection> {
        if text.trim().is_empty() && attachments.is_empty() {
            return Err(Rejection::EmptyPrompt);
        }
        if attachments.len() > MAX_ATTACHMENTS {
            return Err(Rejection::TooManyAttachments);
        }
        if !attachments.iter().all(crate::files::readable) {
            return Err(Rejection::UnknownFile);
        }
        if text.len() > MAX_PROMPT_SIZE {
            return Err(Rejection::PromptTooLong);
        }
        if !self.transcript.status.accepts_prompts() {
            return Err(Rejection::NotReady);
        }
        if self.turn.is_some() || !self.ready || self.carrying() {
            let waiting = &self.transcript.queued;
            let text_size: usize = waiting.iter().map(|queued| queued.text.len()).sum();
            if waiting.len() >= MAX_QUEUED || text_size + text.len() > MAX_QUEUED_TEXT {
                return Err(Rejection::QueueFull);
            }
            let mut queued = self.transcript.queued.clone();
            queued.push(Queued {
                attachments,
                id,
                held: false,
                steered: false,
                text,
            });
            if let Some(origin) = origin {
                self.queued_origins.insert(id, origin);
            }
            self.queue_changed(queued);
            return Ok(());
        }
        self.send_prompt(&text, &attachments, origin);
        Ok(())
    }

    /// Sends a prompt with its files, read now, and shows it in the
    /// transcript.
    fn send_prompt(&mut self, text: &str, attachments: &[Attachment], origin: Option<Origin>) {
        if let Some(outgoing) = self.outgoing(text, attachments, origin, true) {
            self.send_outgoing(outgoing);
        }
    }

    /// A prompt as the agent gets it: the text with a line per file shown
    /// in the transcript, then the pictures. What Sheprd tells the agent
    /// besides goes first, and the transcript leaves it out. A prompt that
    /// starts a turn carries the conversation an agent that took over
    /// hears next; one steered into a running turn does not.
    pub(super) fn outgoing(
        &mut self,
        text: &str,
        attachments: &[Attachment],
        origin: Option<Origin>,
        starts_turn: bool,
    ) -> Option<Outgoing> {
        self.acp_session.as_ref()?;
        let prepared: Vec<_> = attachments
            .iter()
            .map(|attachment| crate::files::prepare(attachment, self.accepts))
            .collect();
        let mut shown = text.to_owned();
        for attachment in attachments {
            if !shown.is_empty() {
                shown.push('\n');
            }
            shown.push_str(sheprd_chat_protocol::ATTACHED);
            shown.push_str(attachment.name());
        }
        let mut blocks = Vec::with_capacity(prepared.len() + 3);
        self.context
            .running(crate::context::Running::of(&self.transcript));
        let note = self.context.before_prompt(origin, text);
        if let Some(note) = &note {
            blocks.push(wire::ContentBlock::Text(wire::TextContent::new(note)));
        }
        let handoff = starts_turn.then(|| self.thread.take_handoff()).flatten();
        if let Some(handoff) = &handoff {
            blocks.push(wire::ContentBlock::Text(wire::TextContent::new(handoff)));
        }
        // The transcript keeps what went ahead of the person's words apart
        // from them, for a client to show when asked.
        let sent = [note.as_deref(), handoff.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let sent = (!sent.is_empty()).then(|| sent.join("\n\n"));
        if !text.trim().is_empty() {
            blocks.push(wire::ContentBlock::Text(wire::TextContent::new(text)));
        }
        let mut images = Vec::new();
        for item in prepared {
            images.extend(item.image);
            blocks.push(item.block);
        }
        Some(Outgoing {
            blocks,
            files: attachments.len(),
            handoff,
            images,
            sent,
            shown,
        })
    }

    /// Starts a turn with `outgoing`, shown in the transcript.
    pub(super) fn send_outgoing(&mut self, outgoing: Outgoing) {
        let Some(session) = self.acp_session.clone() else {
            if let Some(handoff) = outgoing.handoff {
                self.thread.restore_handoff(handoff);
            }
            return;
        };
        let Outgoing {
            blocks,
            files,
            handoff,
            images,
            sent,
            shown,
        } = outgoing;
        self.show_prompt(&shown, files, sent, images);
        let request = wire::PromptRequest::new(session, blocks);
        if self.request("session/prompt", &request, Pending::Prompt) {
            // Only a prompt the agent received names the chat and starts
            // the conversation a later agent is handed.
            self.thread.prompted(&shown, handoff);
            self.keep_started(&shown);
            (self.report)(Update::Prompt(shown));
            self.turn = Some(self.next_request);
            self.watchdog.start(std::time::Instant::now());
            self.emit(Change::Turn {
                turn: Some(Turn {
                    started_ms: crate::session::now_ms(),
                }),
            });
            self.status(Status::Working);
        } else if let Some(handoff) = handoff {
            self.thread.restore_handoff(handoff);
        }
    }

    /// Shows the person's prompt in the transcript: its text, then its
    /// pictures.
    pub(super) fn show_prompt(
        &mut self,
        shown: &str,
        files: usize,
        sent: Option<String>,
        images: Vec<sheprd_chat_protocol::Image>,
    ) {
        let prompt = self.projector.prompt(shown, files, sent);
        self.emit(prompt);
        for image in images {
            let change = self.projector.image(Author::User, image, None);
            self.emit(change);
        }
    }

    /// Sends the oldest queued prompt once the agent is free; those held
    /// when a turn was stopped wait to be sent now.
    pub(super) fn next_queued(&mut self) {
        if self.turn.is_some() || !self.ready || self.carrying() {
            return;
        }
        let mut queued = self.transcript.queued.clone();
        let Some(index) = queued.iter().position(|queued| !queued.held) else {
            return;
        };
        let next = queued.remove(index);
        let origin = self.queued_origins.get(&next.id).cloned();
        // A prompt steered into a turn that ended without it goes as it was
        // sent: what it told the agent besides is not told again, and its
        // files as they were then. Taken before the queue changes, which
        // drops what prompts no longer waiting were prepared with.
        let sent = self.steering.remove(&next.id);
        self.queue_changed(queued);
        match sent {
            Some(outgoing) => self.send_outgoing(outgoing),
            None => self.send_prompt(&next.text, &next.attachments, origin),
        }
    }

    fn cancel(&mut self) -> Result<(), Rejection> {
        let waiting = self.transcript.queued.iter().any(|queued| !queued.held);
        if self.turn.is_none() && !waiting {
            return Err(Rejection::NotWorking);
        }
        self.hold_queue();
        self.stop_turn()
    }

    /// Ends the running turn, declining what it asked.
    pub(super) fn stop_turn(&mut self) -> Result<(), Rejection> {
        self.resolve_all();
        self.watchdog.clear();
        if self.turn.is_some()
            && let Some(session) = self.acp_session.clone()
            && let Err(error) = self
                .outbox
                .notify("session/cancel", &wire::CancelNotification::new(session))
        {
            self.fail(&error.to_string());
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    /// Asks the bridge to end an activity. It reports how it ended like any
    /// other, so nothing here marks it stopped.
    fn stop_activity(&mut self, call_id: &str) -> Result<(), Rejection> {
        let stoppable = self
            .transcript
            .activity(call_id)
            .is_some_and(|activity| activity.is_running() && activity.stoppable);
        if !stoppable {
            return Err(Rejection::UnknownActivity);
        }
        if let Err(error) = self
            .outbox
            .notify(STOP_METHOD, &serde_json::json!({ "callId": call_id }))
        {
            self.fail(&error.to_string());
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    fn answer(&mut self, permission: PermissionId, option: &str) -> Result<(), Rejection> {
        let asked = self
            .asked
            .get(&permission)
            .ok_or(Rejection::UnknownPermission)?;
        if !asked.options.iter().any(|known| known == option) {
            return Err(Rejection::UnknownOption);
        }
        self.resolve(permission, Some(option));
        Ok(())
    }

    fn answer_question(
        &mut self,
        permission: PermissionId,
        answers: Vec<String>,
    ) -> Result<(), Rejection> {
        if !self.asked.contains_key(&permission) {
            return Err(Rejection::UnknownPermission);
        }
        self.transcript
            .permissions
            .iter()
            .find(|asked| asked.id == permission)
            .and_then(|asked| asked.question.as_ref())
            .ok_or(Rejection::UnknownOption)?
            .check(&answers)?;
        self.resolve_answers(permission, answers);
        Ok(())
    }

    pub(super) fn set_mode(&mut self, mode: String) -> Result<(), Rejection> {
        if !self.ready {
            return self.defer_mode(mode);
        }
        let session = self.acp_session.clone().ok_or(Rejection::NotReady)?;
        let known = self
            .transcript
            .modes
            .as_ref()
            .is_some_and(|modes| modes.available.iter().any(|known| known.id == mode));
        if !known {
            return Err(Rejection::UnknownMode);
        }
        let request = wire::SetSessionModeRequest::new(session, mode.clone());
        if !self.request("session/set_mode", &request, Pending::SetMode(mode)) {
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    pub(super) fn set_option(&mut self, id: String, value: &OptionValue) -> Result<(), Rejection> {
        if !self.ready {
            return self.defer_option(id, value.clone());
        }
        let session = self.acp_session.clone().ok_or(Rejection::NotReady)?;
        let known = self
            .transcript
            .options
            .iter()
            .any(|option| option.id == id && option.accepts(value));
        if !known {
            return Err(Rejection::UnknownSetting);
        }
        if self.legacy_model_option && id == crate::project::models::OPTION_ID {
            let OptionValue::Choice { id: model_id } = value else {
                return Err(Rejection::UnknownSetting);
            };
            let request = serde_json::json!({
                "sessionId": session,
                "modelId": model_id,
            });
            if !self.request(
                "session/set_model",
                &request,
                Pending::SetModel(model_id.clone()),
            ) {
                return Err(Rejection::AgentUnavailable);
            }
            return Ok(());
        }
        let request = wire::SetSessionConfigOptionRequest::new(
            session,
            id,
            crate::project::option_value(value),
        );
        if !self.request("session/set_config_option", &request, Pending::SetOption) {
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    /// Turns auto-accepting on or off; turning it on also answers the
    /// requests already waiting.
    pub(super) fn set_auto_accept(&mut self, on: bool) {
        if self.transcript.auto_accept != on {
            self.emit(Change::AutoAccept { on });
        }
        if on {
            let waiting: Vec<PermissionId> = self
                .transcript
                .permissions
                .iter()
                .map(|permission| permission.id)
                .collect();
            for permission in waiting {
                self.auto_answer(permission);
            }
        }
    }

    /// Answers `permission` with its allow option while auto-accepting;
    /// one that offers no way to allow it keeps waiting for a person.
    pub(super) fn auto_answer(&mut self, permission: PermissionId) {
        if !self.transcript.auto_accept {
            return;
        }
        let option = self
            .transcript
            .permissions
            .iter()
            .find(|asked| asked.id == permission)
            .and_then(|asked| asked.allow_option())
            .map(|option| option.id.clone());
        if let Some(option) = option {
            self.resolve(permission, Some(&option));
        }
    }

    fn unqueue(&mut self, id: CommandId) -> Result<(), Rejection> {
        let mut queued = self.transcript.queued.clone();
        // A steered prompt is already with the agent.
        let index = queued
            .iter()
            .position(|queued| queued.id == id && !queued.steered)
            .ok_or(Rejection::UnknownQueued)?;
        queued.remove(index);
        self.queue_changed(queued);
        Ok(())
    }
}
