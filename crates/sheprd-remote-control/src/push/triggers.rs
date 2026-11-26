//! When a chat change is worth a push, and what it says. Pure: the daemon
//! passes each change with the transcript it applies to, before applying it.

use super::{
    EventId, Message, Urgency,
    directives::{without_directives, wizzes},
    plain::{plain, verdict_first},
};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{
    Change, EntryKind, NoticeLevel, Permission, Status, Transcript, TurnEnd, TurnSummary,
};

/// The most characters of chat text an excerpt shows.
pub(super) const EXCERPT_CHARS: usize = 120;

/// Which chat a change belongs to, as the phone names it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chat<'a> {
    /// The agent's display name, such as `Claude`.
    pub agent: &'a str,
    pub pane: &'a str,
    pub title: &'a str,
    /// The workspace, such as `herdr-gpui`.
    pub workspace: &'a str,
}

/// How much of the conversation a push may carry. Push services relay only
/// ciphertext, but a notification shows on a locked screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    /// Who asked and where, never what was said.
    Generic,
    /// Also a short excerpt: the question, the error, or the answer's end.
    #[default]
    Excerpt,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    // keep-sorted start
    Error,
    Question,
    /// An agent in a plain terminal waits for the user.
    TerminalAsking,
    /// An agent in a plain terminal finished what it was doing.
    TerminalDone,
    /// One the user sent from the phone to see notifications arrive.
    Test,
    TurnEnded,
    // keep-sorted end
}

impl Kind {
    /// Whether it asks the user something, so it may not wait.
    pub(crate) fn asks(self) -> bool {
        matches!(self, Self::Question | Self::TerminalAsking)
    }

    /// Whether it is about a terminal tab rather than a chat.
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::TerminalAsking | Self::TerminalDone)
    }
}

/// A push to send for one chat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Notification {
    pub body: String,
    /// Closes the chat's notification instead of showing one: its last
    /// question was answered, from whichever client.
    pub clear: bool,
    /// This decision's id, the same in every copy of the push.
    pub event: EventId,
    pub kind: Kind,
    pub pane: String,
    /// The permission request a question is, for the native app's answer
    /// buttons; the Web Push message leaves it out.
    pub permission: Option<Permission>,
    /// The pull request a finished turn created or pushed to, for the
    /// native app's Open PR action; the Web Push message leaves it out.
    pub pr: Option<String>,
    /// Where the chat is, `workspace · title`, which the native app shows
    /// under the title and the Web Push message above the body.
    pub subtitle: String,
    /// One per chat, so a newer push replaces the older one.
    pub tag: String,
    pub title: String,
    pub urgency: Urgency,
    /// The agent asked for the person's attention with `::wizz`: the
    /// alert interrupts as a question does, and the phone buzzes.
    pub wizz: bool,
}

impl Notification {
    pub(super) fn new(chat: &Chat<'_>, kind: Kind, title: String, body: String) -> Self {
        Self {
            body,
            clear: false,
            event: EventId::new(),
            kind,
            pane: chat.pane.to_owned(),
            permission: None,
            pr: None,
            subtitle: place(chat),
            tag: tag(chat.pane),
            title,
            urgency: if kind.asks() {
                Urgency::High
            } else {
                Urgency::Normal
            },
            wizz: false,
        }
    }

    fn clear(chat: &Chat<'_>) -> Self {
        Self::cleared(chat.pane)
    }

    /// Closes `pane`'s notification, and moves the badge with it.
    pub(crate) fn cleared(pane: &str) -> Self {
        Self {
            body: String::new(),
            clear: true,
            event: EventId::new(),
            kind: Kind::Question,
            pane: pane.to_owned(),
            permission: None,
            pr: None,
            subtitle: String::new(),
            tag: tag(pane),
            title: String::new(),
            urgency: Urgency::High,
            wizz: false,
        }
    }

    /// With `pr`, the pull request its turn created or pushed to
    /// ([`super::TurnPull`]), when it is a finished turn's.
    pub(crate) fn with_pr(self, pr: Option<String>) -> Self {
        if self.kind != Kind::TurnEnded || self.clear {
            return self;
        }
        Self { pr, ..self }
    }

    /// Moves the badge alone: no pane has this notification to close.
    pub(crate) fn badge() -> Self {
        Self::cleared("")
    }

    /// The payload the service worker receives, with `badge` agents
    /// wanting the user.
    pub(crate) fn message(&self, badge: u32) -> Message {
        Message {
            badge,
            body: [self.subtitle.as_str(), self.body.as_str()]
                .into_iter()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            clear: self.clear,
            kind: self.kind,
            tag: self.tag.clone(),
            title: self.title.clone(),
            url: format!(
                "/{}/{}",
                if self.kind.is_terminal() {
                    "term"
                } else {
                    "chat"
                },
                path_segment(&self.pane)
            ),
        }
    }
}

/// `text` percent-encoded as one URL path segment: only unreserved
/// characters stay as they are.
fn path_segment(text: &str) -> String {
    let mut segment = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            segment.push(char::from(byte));
        } else {
            segment.push_str(&format!("%{byte:02X}"));
        }
    }
    segment
}

/// The notification tag for the chat in `pane`.
pub(crate) fn tag(pane: &str) -> String {
    format!("chat-{pane}")
}

/// The push `change` calls for, given the transcript `before` it applies.
pub(crate) fn decide(
    chat: &Chat<'_>,
    before: &Transcript,
    change: &Change,
    privacy: Privacy,
) -> Option<Notification> {
    let shown = privacy == Privacy::Excerpt;
    let excerpt = |text: &str| shown.then(|| excerpt(text));
    // What the agent wrote, cut at a sentence's end.
    let said = |text: &str| shown.then(|| sentences(text));
    match change {
        Change::PermissionAsked { permission } => {
            // Answered by itself at once; nobody needs to look.
            if before.auto_accept {
                return None;
            }
            let title = format!("{} needs an answer", chat.agent);
            Some(Notification {
                permission: Some(permission.clone()),
                ..Notification::new(
                    chat,
                    Kind::Question,
                    title,
                    body(excerpt(&permission.title), None),
                )
            })
        }
        Change::PermissionResolved { id } => {
            let last = matches!(before.permissions.as_slice(), [only] if only.id == *id);
            last.then(|| Notification::clear(chat))
        }
        Change::Push { entry } => match &entry.kind {
            EntryKind::Turn(summary) => turn_ended(chat, before, summary, privacy),
            EntryKind::Notice {
                level: NoticeLevel::Error,
                text,
                ..
            } => {
                let title = format!("{} hit an error", chat.agent);
                Some(Notification::new(
                    chat,
                    Kind::Error,
                    title,
                    body(said(&plain(text)), None),
                ))
            }
            _ => None,
        },
        Change::Status { status } if *status != before.status => {
            let (title, message) = match status {
                Status::Failed { message } => (format!("{} failed", chat.agent), message),
                // An agent that quits while it waits on a question leaves it
                // unanswerable; any other exit is the user's doing.
                Status::Exited { message } if before.is_blocked() => {
                    (format!("{} stopped while asking", chat.agent), message)
                }
                _ => return None,
            };
            Some(Notification::new(
                chat,
                Kind::Error,
                title,
                body(said(message), None),
            ))
        }
        _ => None,
    }
}

fn turn_ended(
    chat: &Chat<'_>,
    before: &Transcript,
    summary: &TurnSummary,
    privacy: Privacy,
) -> Option<Notification> {
    // The user stopped it, so they know.
    if summary.end == TurnEnd::Cancelled {
        return None;
    }
    // Token counts mean little on a lock screen; the chat keeps them.
    let facts = summary.took();
    let last = last_answer(before);
    // Only this turn's reply may nudge: an older one already did.
    let wizz = this_turns_answer(before).is_some_and(wizzes);
    let answer = last
        .filter(|_| privacy == Privacy::Excerpt)
        .map(|text| sentences(&verdict_first(&without_directives(text))));
    let title = format!("{} finished", chat.agent);
    let ended = Notification::new(chat, Kind::TurnEnded, title, body(answer, Some(&facts)));
    // Whether it asked is no part of what was said: a generic push buzzes too.
    Some(if wizz {
        Notification {
            urgency: Urgency::High,
            wizz: true,
            ..ended
        }
    } else {
        ended
    })
}

/// The text of the agent's last answer in the transcript.
fn last_answer(transcript: &Transcript) -> Option<&str> {
    transcript
        .entries
        .iter()
        .rev()
        .find_map(|entry| match &entry.kind {
            EntryKind::Agent { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
}

/// The agent's last answer since the user last spoke, if the turn that
/// ends wrote one.
fn this_turns_answer(transcript: &Transcript) -> Option<&str> {
    transcript
        .entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry.kind, EntryKind::User { .. }))
        .find_map(|entry| match &entry.kind {
            EntryKind::Agent { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
}

/// `excerpt` (when allowed) above any `facts`.
pub(super) fn body(excerpt: Option<String>, facts: Option<&str>) -> String {
    let lines = [excerpt.as_deref(), facts];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
}

/// Where the chat is: `workspace · title`, or the workspace alone.
fn place(chat: &Chat<'_>) -> String {
    if chat.title.is_empty() {
        chat.workspace.to_owned()
    } else {
        format!("{} \u{b7} {}", chat.workspace, chat.title)
    }
}

/// `text` on one line, at most [`EXCERPT_CHARS`] characters, ending in `…`
/// when cut. The phone cuts a Live Activity's question the same way
/// (`ActivityAnswer.excerpt`) to match an answer to it, so the two must not
/// drift apart.
pub(crate) fn excerpt(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let line = words.join(" ");
    if line.chars().count() <= EXCERPT_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(EXCERPT_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('\u{2026}');
    cut
}

/// `line`, longer than [`EXCERPT_CHARS`], cut to fit at a word's end where
/// one is near, ending in `…`.
fn word_cut(line: &str) -> String {
    let cut: String = line.chars().take(EXCERPT_CHARS - 1).collect();
    // The cut lands inside a word unless a space follows it.
    let whole = line[cut.len()..].starts_with(' ');
    let mut cut = match cut.rfind(' ') {
        Some(space) if !whole && space >= cut.len() / 2 => cut[..space].to_owned(),
        _ => cut,
    };
    cut.truncate(cut.trim_end().len());
    cut.push('\u{2026}');
    cut
}

/// What an agent wrote on one line, at most [`EXCERPT_CHARS`] characters,
/// ending with a whole sentence when the sentences that fit fill at least
/// half the space: a lock screen then reads complete thoughts. Otherwise
/// cut at a word's end, with `…`.
pub(crate) fn sentences(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= EXCERPT_CHARS {
        return line;
    }
    let room: String = line.chars().take(EXCERPT_CHARS).collect();
    let end = room
        .char_indices()
        .filter(|(at, ch)| {
            matches!(ch, '.' | '!' | '?') && line[at + ch.len_utf8()..].starts_with(' ')
        })
        .map(|(at, ch)| at + ch.len_utf8())
        .next_back();
    match end {
        Some(end) if room[..end].chars().count() >= EXCERPT_CHARS / 2 => room[..end].to_owned(),
        _ => word_cut(&line),
    }
}
