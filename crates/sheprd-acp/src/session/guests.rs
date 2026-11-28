//! Guest prompts: words someone other than the person wrote, relayed into
//! the chat by an external tool (`docs/chat-guests.md`).
//!
//! A guest prompt waits as a permission request until the person allows or
//! denies it from any client; only then is its command acknowledged. Auto
//! accepting never answers one. A connection that relays a guest prompt is
//! a guest connection from then on and may send nothing else, so a guest
//! can never answer the agent's permission requests, switch modes, cancel,
//! or anything else the person does.

use std::collections::{HashMap, HashSet};

use sheprd_chat_protocol::{
    Change, Command, CommandId, GUEST_ALLOW, GUEST_DENY, Guest, GuestPrompt, MAX_GUEST_PROMPT_SIZE,
    MAX_GUESTS_WAITING, Origin, Permission, PermissionId, PermissionKind, PermissionOption,
    Rejection,
};

use crate::session::{ConnId, Session, Update};

#[derive(Debug, Default)]
pub(super) struct Guests {
    /// Connections that relayed a guest prompt.
    conns: HashSet<ConnId>,
    waiting: HashMap<PermissionId, Waiting>,
}

#[derive(Debug)]
struct Waiting {
    /// The connection the outcome goes to: the latest that sent the command.
    conn: ConnId,
    id: CommandId,
    prompt: GuestPrompt,
}

impl Guests {
    /// Whether `conn` relays guest prompts, and so may send nothing else.
    pub(super) fn is_guest(&self, conn: ConnId) -> bool {
        self.conns.contains(&conn)
    }

    /// Whether `permission` is a guest prompt waiting for the person.
    pub(super) fn waits(&self, permission: PermissionId) -> bool {
        self.waiting.contains_key(&permission)
    }

    /// Marks `conn` as relaying guests from now on.
    pub(super) fn relays(&mut self, conn: ConnId) {
        self.conns.insert(conn);
    }

    pub(super) fn detached(&mut self, conn: ConnId) {
        self.conns.remove(&conn);
    }
}

impl<R: FnMut(Update)> Session<R> {
    /// Runs `command` when it is a guest prompt; `None` leaves it to the
    /// person's commands. `Some(None)` means the outcome waits for the
    /// person.
    pub(super) fn guest_command(
        &mut self,
        conn: ConnId,
        id: CommandId,
        command: &Command,
    ) -> Option<Option<Rejection>> {
        let (text, guest) = relayed(command)?;
        // A resend while it waits: the outcome goes to the newer connection.
        if let Some(waiting) = self
            .guests
            .waiting
            .values_mut()
            .find(|waiting| waiting.id == id)
        {
            waiting.conn = conn;
            return Some(None);
        }
        Some(self.ask_guest(conn, id, text, guest).err())
    }

    /// Puts a guest's prompt before the person as a permission request.
    fn ask_guest(
        &mut self,
        conn: ConnId,
        id: CommandId,
        text: &str,
        guest: &Guest,
    ) -> Result<(), Rejection> {
        if !guest.is_valid() {
            return Err(Rejection::InvalidGuest);
        }
        if text.trim().is_empty() {
            return Err(Rejection::EmptyPrompt);
        }
        if text.len() > MAX_GUEST_PROMPT_SIZE {
            return Err(Rejection::PromptTooLong);
        }
        // What the person reads is what the agent gets.
        if !sheprd_chat_protocol::shows_as_written(text) {
            return Err(Rejection::HiddenText);
        }
        if !self.transcript.status.accepts_prompts() || self.transcript.read_only {
            return Err(Rejection::NotReady);
        }
        if self.guests.waiting.len() >= MAX_GUESTS_WAITING {
            return Err(Rejection::QueueFull);
        }
        self.next_permission += 1;
        let permission = PermissionId(self.next_permission);
        let prompt = GuestPrompt {
            guest: guest.clone(),
            text: text.to_owned(),
        };
        self.guests.waiting.insert(
            permission,
            Waiting {
                conn,
                id,
                prompt: prompt.clone(),
            },
        );
        self.emit(Change::PermissionAsked {
            permission: Permission {
                call_id: None,
                id: permission,
                options: vec![
                    PermissionOption {
                        id: GUEST_ALLOW.to_owned(),
                        kind: PermissionKind::AllowOnce,
                        name: "Send to the agent".to_owned(),
                    },
                    PermissionOption {
                        id: GUEST_DENY.to_owned(),
                        kind: PermissionKind::RejectOnce,
                        name: "Decline".to_owned(),
                    },
                ],
                question: None,
                title: title(&prompt),
                guest: Some(Box::new(prompt)),
            },
        });
        Ok(())
    }

    /// The person's answer to a guest prompt: allowed, it goes to the agent
    /// as any prompt does, now or queued; either way its relay learns how
    /// it ended. `None` when `permission` is not a guest prompt.
    pub(super) fn answer_guest(
        &mut self,
        permission: PermissionId,
        option: &str,
    ) -> Option<Result<(), Rejection>> {
        if !self.guests.waits(permission) {
            return None;
        }
        let allow = match option {
            GUEST_ALLOW => true,
            GUEST_DENY => false,
            _ => return Some(Err(Rejection::UnknownOption)),
        };
        let waiting = self.guests.waiting.remove(&permission)?;
        self.emit(Change::PermissionResolved { id: permission });
        let outcome = if allow {
            let GuestPrompt { guest, text } = waiting.prompt;
            self.prompt(waiting.id, text, Vec::new(), Some(Origin::guest(guest)))
        } else {
            Err(Rejection::Denied)
        };
        self.ack(waiting.conn, waiting.id, outcome.err());
        Some(Ok(()))
    }

    /// Ends every waiting guest prompt with `rejection`: the chat can no
    /// longer take them.
    pub(super) fn drop_guests(&mut self, rejection: Rejection) {
        let mut waiting: Vec<_> = self.guests.waiting.drain().collect();
        waiting.sort_by_key(|(permission, _)| *permission);
        for (permission, waiting) in waiting {
            self.emit(Change::PermissionResolved { id: permission });
            self.ack(waiting.conn, waiting.id, Some(rejection));
        }
    }
}

/// The text and guest of a command that relays a guest's prompt: a guest
/// prompt, or a prompt that names a guest, however it was sent.
pub(super) fn relayed(command: &Command) -> Option<(&str, &Guest)> {
    match command {
        Command::GuestPrompt { text, guest } => Some((text, guest)),
        Command::Prompt {
            text,
            origin: Some(Origin {
                guest: Some(guest), ..
            }),
            ..
        } => Some((text, guest)),
        _ => None,
    }
}

/// The request's title: who asks, then every word they would send, so a
/// client that predates guests shows all that its person allows.
fn title(prompt: &GuestPrompt) -> String {
    format!("{} asks to send:\n{}", prompt.guest.label(), prompt.text)
}
