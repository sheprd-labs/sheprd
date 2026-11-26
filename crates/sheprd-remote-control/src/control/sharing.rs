//! Control-socket types for read-only shared sessions, shared between
//! `sheprd-gpui` and `sheprd-remote-control` without pulling Tokio into the GUI.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// How much prior history a share includes. Defaults to `FromNow` so existing
/// conversation turns stay excluded unless the owner explicitly selects `All`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareHistory {
    All,
    #[default]
    FromNow,
}

/// A one-time HTTPS fragment invitation link returned to the owner. Redacted in
/// `Debug` so logs never record bearer credentials.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShareLink(String);

impl ShareLink {
    pub fn new(link: String) -> Self {
        Self(link)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ShareLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted share link]")
    }
}

/// Speaker/kind of one row in the owner's guest preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewRole {
    Agent,
    Tool,
    User,
}

/// One filtered entry from the host's allowlist projection, for Preview as guest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewEntry {
    pub role: PreviewRole,
    pub text: String,
}

/// Exact guest-visible projection of a chat under a chosen policy, before any
/// share or grant is created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SharePreview {
    pub entries: Vec<PreviewEntry>,
    pub truncated: bool,
    pub total_entries: usize,
}

/// Non-secret summary of one active host share. Never carries capabilities.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShareSummary {
    pub share: Uuid,
    pub pane: String,
    pub history: ShareHistory,
    pub include_tools: bool,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub grants: usize,
    pub relay: String,
}

/// Returned once when the owner confirms creating a share and its first link.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShareCreated {
    pub share: ShareSummary,
    pub grant: Uuid,
    pub link: ShareLink,
}
