//! Guests: people or agents outside the chat whose words an external tool,
//! such as a session-sharing tool, relays into it.
//!
//! A guest prompt is never the person's: it waits as a permission request
//! until the person allows it, and the agent is told who wrote it, through
//! which tool, and that it is untrusted. See `docs/chat-guests.md`.

use serde::{Deserialize, Serialize};

/// The longest guest name, in bytes.
pub const MAX_GUEST_NAME: usize = 64;
/// The longest tool name, in bytes.
pub const MAX_GUEST_TOOL: usize = 32;
/// The longest prompt a guest may send, in bytes: it is shown in full in the
/// permission request every client draws.
pub const MAX_GUEST_PROMPT_SIZE: usize = 16 * 1024;
/// The most guest prompts waiting for the person at once.
pub const MAX_GUESTS_WAITING: usize = 8;

/// Who wrote a prompt the person did not type, as the relaying tool names
/// them. Neither name is verified by Sheprd: they are labels the tool
/// supplies, shown to the person and the agent as such.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Guest {
    /// The guest's display name, such as `Bob`.
    pub name: String,
    /// The tool that relayed the prompt, such as `tma`.
    pub tool: String,
}

impl Guest {
    pub fn new(name: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tool: tool.into(),
        }
    }

    /// Whether both names are something a client may show and a note may
    /// quote: not blank, within their limits, and free of control
    /// characters and of the characters that end a quoted attribute.
    pub fn is_valid(&self) -> bool {
        label_ok(&self.name, MAX_GUEST_NAME) && label_ok(&self.tool, MAX_GUEST_TOOL)
    }

    /// How clients name the guest, such as `Bob via tma`.
    pub fn label(&self) -> String {
        format!("{} via {}", self.name, self.tool)
    }
}

fn label_ok(text: &str, max: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= max
        && !text
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '&' | '`'))
}

/// Whether every character of a guest's `text` shows as itself: no control
/// characters but line breaks and tabs, and none of the invisible or
/// direction-changing ones that could hide or reorder words the person is
/// asked to allow. The zero-width joiner and emoji presentation selectors
/// stay, as emoji need them and they hide nothing.
pub fn shows_as_written(text: &str) -> bool {
    !text.chars().any(|c| {
        (c.is_control() && !matches!(c, '\n' | '\t'))
            || matches!(c,
                '\u{00ad}' | '\u{034f}' | '\u{061c}' | '\u{115f}' | '\u{1160}' | '\u{17b4}'
                | '\u{17b5}' | '\u{180b}'..='\u{180f}' | '\u{200b}' | '\u{200c}' | '\u{200e}' | '\u{200f}'
                | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{3164}'
                | '\u{fe00}'..='\u{fe0d}' | '\u{feff}' | '\u{ffa0}' | '\u{fff9}'..='\u{fffb}'
                | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0000}'..='\u{e0fff}')
    })
}

/// A guest's prompt waiting for the person, carried by its permission
/// request so a client can show it in full.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GuestPrompt {
    pub guest: Guest,
    pub text: String,
}

/// The option ids of a guest prompt's permission request.
pub const GUEST_ALLOW: &str = "guest_allow";
pub const GUEST_DENY: &str = "guest_deny";
