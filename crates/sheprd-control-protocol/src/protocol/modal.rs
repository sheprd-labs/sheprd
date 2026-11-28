//! Persistent agent modals, dynamic progress pills, secret-safe vault prompts,
//! runbooks, drop-zones, spotlights, mini-diffs, and semantic cues (`sheprd modal`).

use serde::{Deserialize, Serialize};

/// Longest title, body, or label carried over the control socket.
pub const MAX_TITLE: usize = 200;
pub const MAX_BODY: usize = 4 * 1024;
pub const MAX_CHOICES: usize = 8;
pub const MAX_STEPS: usize = 32;

/// Where a persistent modal or interaction appears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModalScope {
    /// Pinned inside the caller's Sheprd window (survives tab/workspace switches).
    #[default]
    Window,
    /// Always-on-top across every app, Space, and Sheprd window (e.g. YubiKey touch).
    All,
}

/// Semantic audio + Edge Light + iOS CoreHaptics cue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    // keep-sorted start
    Countdown,
    Decision,
    Done,
    Secret,
    Yubikey,
    // keep-sorted end
}

impl CueKind {
    pub const ALL: &'static [Self] = &[
        Self::Countdown,
        Self::Decision,
        Self::Done,
        Self::Secret,
        Self::Yubikey,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Countdown => "countdown",
            Self::Decision => "decision",
            Self::Done => "done",
            Self::Secret => "secret",
            Self::Yubikey => "yubikey",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name.trim() {
            "countdown" => Some(Self::Countdown),
            "decision" => Some(Self::Decision),
            "done" => Some(Self::Done),
            "secret" => Some(Self::Secret),
            "yubikey" => Some(Self::Yubikey),
            _ => None,
        }
    }
}

/// One step in a shared human + agent runbook checklist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunbookStep {
    pub label: String,
    /// `true` when the human must check this step off; `false` for agent steps.
    #[serde(default)]
    pub human: bool,
    #[serde(default)]
    pub done: bool,
}

/// Requests of the `modal` control family (`sheprd modal ...`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModalOp {
    /// Show or replace a persistent modal (window-scoped or global/all).
    Show {
        #[serde(default)]
        scope: ModalScope,
        title: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        preset: Option<String>,
        #[serde(default)]
        choices: Vec<String>,
        #[serde(default)]
        quick_reply: bool,
        #[serde(default)]
        cue: Option<CueKind>,
    },
    /// Show a timed countdown "brake pedal" modal.
    Countdown {
        #[serde(default)]
        scope: ModalScope,
        title: String,
        #[serde(default)]
        body: String,
        seconds: u32,
    },
    /// Request a masked 6-digit OTP/2FA code (redacted from transcript).
    Otp {
        title: String,
        #[serde(default)]
        body: String,
    },
    /// Prompt for a browser OAuth / device-code handoff.
    Oauth {
        title: String,
        #[serde(default)]
        body: String,
        url: String,
        #[serde(default)]
        code: Option<String>,
    },
    /// Request a secret injected directly into an env file (`0600`) or Keychain,
    /// never sent to the LLM or written to chat history.
    Secret {
        title: String,
        key: String,
        /// e.g. `env-file:.env.local` or `keychain:my-service`
        dest: String,
        #[serde(default)]
        reason: String,
        #[serde(default)]
        save_keychain: bool,
    },
    /// Update the floating "Dynamic Island" progress pill.
    Progress {
        title: String,
        #[serde(default)]
        percent: Option<u8>,
        #[serde(default)]
        step: String,
        #[serde(default)]
        done: bool,
    },
    /// Pin or update a shared human + agent runbook checklist.
    Runbook {
        title: String,
        steps: Vec<RunbookStep>,
    },
    /// Open a floating file drop-zone asking the user to attach a file.
    Drop {
        #[serde(default)]
        scope: ModalScope,
        title: String,
        #[serde(default)]
        accept: String,
    },
    /// Open a focused one-item diff approval sheet.
    Diff {
        title: String,
        file: String,
        diff: String,
    },
    /// Point a guided spotlight callout at a pane, line, or sidebar worktree.
    Spotlight {
        title: String,
        target: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        step: Option<String>,
    },
    /// Play a semantic audio + Edge Light cue without opening a modal.
    Cue { cue: CueKind },
    /// Clear the caller's active modal/pill (or all when `all` is true).
    Clear {
        #[serde(default)]
        all: bool,
        #[serde(default)]
        id: Option<String>,
    },
    /// List active persistent modals and progress pills.
    List,
}

/// Summary of one active modal or progress pill returned by `modal list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModalSummary {
    pub id: String,
    pub kind: String,
    pub scope: ModalScope,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Shown { id: String },
    Cleared { count: usize },
    Listed { items: Vec<ModalSummary> },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn cue_kind_round_trips() {
        for &cue in CueKind::ALL {
            assert_eq!(CueKind::parse(cue.name()), Some(cue));
        }
        assert_eq!(CueKind::parse("unknown"), None);
    }

    #[test]
    fn modal_ops_serialize_and_deserialize() {
        let op = ModalOp::Secret {
            title: "Provide STRIPE_TEST_KEY".to_owned(),
            key: "STRIPE_TEST_KEY".to_owned(),
            dest: "env-file:.env.test.local".to_owned(),
            reason: "Run webhook tests".to_owned(),
            save_keychain: true,
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: ModalOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }
}
