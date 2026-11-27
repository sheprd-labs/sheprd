//! Agents driving a paired phone's app (`sheprd phone`): showing it a
//! screen of this computer, reading which one it shows, and putting a note
//! in front of the person. Sheprd asks with [`Ask`]; the daemon sends the
//! phone a [`Drive`] over its live socket and answers with what the phone
//! said ([`Answer`]).
//!
//! A phone is driven only while its live socket is open and its person
//! turned on, or left on, Let agents open screens for this computer there, which the phone says
//! with `agent_control` on the socket. The phone refuses while its app is
//! locked. Nothing here pairs, forgets, signs, or approves: those stay the
//! person's, on the phone or the computer.

use crate::control::DeviceId;

mod settings;
use serde::{Deserialize, Serialize};
pub use settings::{Highlight, SettingsPage};

/// The longest note `show` puts on a phone, in characters.
pub const MAX_TEXT_CHARS: usize = 500;
/// The longest pane or workspace id an agent may name, in bytes.
pub const MAX_ID_BYTES: usize = 256;
/// The most phones a [`Answer::Phones`] lists.
pub const MAX_PHONES: usize = 16;

/// A screen of the phone's app. Panes and workspaces are this computer's,
/// by the ids its home lists.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "screen")]
pub enum Screen {
    /// The tab of chats by what they need.
    Home,
    /// The tab of every computer's workspaces.
    Workspaces,
    /// The tab of pull requests.
    PullRequests,
    /// The phone's Settings tab.
    Settings,
    SettingsPage {
        page: SettingsPage,
    },
    /// The form starting a workspace, on this computer.
    NewWorkspace,
    /// A chat (`sheprd-acp` pane, or a terminal agent shown as one).
    Chat {
        pane: String,
    },
    /// A terminal pane.
    Term {
        pane: String,
    },
    Workspace {
        workspace: String,
    },
    /// A workspace's changes.
    Changes {
        workspace: String,
    },
}

impl Screen {
    /// Whether every id it names is within bounds and not empty.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Chat { pane } | Self::Term { pane } => valid_id(pane),
            Self::Workspace { workspace } | Self::Changes { workspace } => valid_id(workspace),
            Self::Home
            | Self::Workspaces
            | Self::PullRequests
            | Self::Settings
            | Self::SettingsPage { .. }
            | Self::NewWorkspace => true,
        }
    }
}

/// Whether `id` may name a pane or workspace.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.chars().any(char::is_control)
}

/// Whether `text` may be shown on a phone.
pub fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MAX_TEXT_CHARS
}

/// What an agent asks of a phone, as Sheprd sends it to the daemon.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum Command {
    /// Which screen the phone shows.
    State,
    Open {
        to: Screen,
    },
    /// Opens a pane as the phone shows it: its chat when it is one, else
    /// its terminal. The daemon decides from its layout.
    OpenPane {
        pane: String,
    },
    /// Persistent guidance, updated in place; no timeout and at most three lines.
    Guide {
        text: String,
        highlight: Option<Highlight>,
        /// The asking agent's pane, which a tap on the banner opens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    Highlight {
        target: Highlight,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    ClearGuide,
    /// A note the person sees on the phone, from this computer's agent.
    Show {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
}

/// What the daemon tells a phone to do, on the live socket as
/// `{"type": "drive", "id": N, "command": ...}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum Drive {
    State,
    Open {
        to: Screen,
    },
    /// `from` is the asking agent's chat or terminal, which a tap on the
    /// note opens; absent when it asked from outside a pane, or its pane is
    /// not listed.
    Show {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Screen>,
    },
    Guide {
        text: String,
        highlight: Option<Highlight>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Screen>,
    },
    Highlight {
        target: Highlight,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Screen>,
    },
    ClearGuide,
}

/// What the phone shows, as it reports it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PhoneState {
    /// The screen in front, when it is a tab or one of this computer's:
    /// `None` for another computer's, or a sheet no [`Screen`] names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Screen>,
    /// Whether the app is locked, showing nothing until its person unlocks
    /// it; it then opens nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

/// What the phone did with a [`Drive`], on the live socket as
/// `{"type": "driven", "id": N, "outcome": ...}`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum Outcome {
    /// Done; what it shows now.
    Done {
        state: PhoneState,
    },
    Refused {
        reason: Refusal,
    },
}

/// Why a phone did not do what it was told.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// Its app is locked.
    Locked,
    /// It knows no such pane or workspace of this computer yet.
    NotFound,
    /// Its person turned agents off for this computer.
    Off,
    /// A reason this Sheprd does not know, from a newer phone.
    #[serde(other)]
    Other,
}

/// A phone agents may drive, connected now.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Phone {
    pub device: DeviceId,
    pub name: String,
    /// When it last said anything on its socket, in milliseconds since the
    /// Unix epoch: the one in use, when several are connected.
    pub active_ms: u64,
}

/// What Sheprd asks the daemon ([`super::Request::Phone`]).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "ask")]
pub enum Ask {
    /// The phones connected that agents may drive.
    Phones,
    Drive {
        /// The phone, by device id or name; else the one used last.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
        command: Command,
    },
}

/// The daemon's answer ([`super::Response::Phone`]).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "answer")]
pub enum Answer {
    Phones {
        phones: Vec<Phone>,
    },
    /// What the phone said.
    Driven {
        phone: Phone,
        outcome: Outcome,
    },
    /// No phone agents may drive is connected, or none by that name.
    NoPhone,
    /// The name is two phones'.
    Ambiguous,
    /// No pane or workspace of that id on this computer.
    NotFound,
    /// An id or note out of bounds ([`valid_id`], [`valid_text`]).
    Invalid,
    /// The phone said nothing within [`super::ASK_WAIT`].
    NoAnswer {
        phone: Phone,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
