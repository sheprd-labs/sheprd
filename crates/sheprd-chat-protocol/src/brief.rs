//! Prompts another agent wrote, such as the brief an agent starts a chat
//! with (`sheprd chat new --prompt`). The person did not type them, so a
//! client may show them as a document to read rather than as their words.
//!
//! The mark lives beside the entries, not in [`EntryKind::User`], so a
//! client that predates it reads the prompt as an ordinary one.

mod error;

pub use error::Error;

use serde::{Deserialize, Serialize};

use crate::{Agent, EntryId, EntryKind, Transcript};

/// The most briefs a transcript keeps; a new one past it is refused.
pub const MAX_BRIEFS: usize = 16;

/// Parent turn whose Live Activity follows a delegated chat's initial brief.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegation {
    pub endpoint: crate::EndpointId,
    pub started_ms: u64,
}

impl Delegation {
    pub const ENV: &str = "SHEPRD_BRIEF_PARENT";
}

/// Launch metadata for the initial brief. It outlives a transcript truncation
/// and is cleared by the next user prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedTask {
    pub parent: Delegation,
    pub label: String,
    pub(crate) prompted: bool,
}

impl DelegatedTask {
    pub fn new(parent: Delegation, brief: &str) -> Self {
        let title = brief
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("Agent");
        Self {
            parent,
            label: crate::excerpt::one_line(title, 80),
            prompted: false,
        }
    }
}

/// A user entry another agent wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    pub entry: EntryId,
    /// The agent that wrote it, when it ran in a chat Sheprd knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Agent>,
}

impl Transcript {
    /// Owner and short name of the initial delegated prompt, even after its
    /// text falls out of the bounded transcript.
    pub fn delegated_brief(&self) -> Option<(&Delegation, &str)> {
        let task = self.delegation.as_ref()?;
        task.prompted.then_some((&task.parent, task.label.as_str()))
    }

    /// The brief `id` is, if another agent wrote it.
    pub fn brief(&self, id: EntryId) -> Option<&Brief> {
        self.briefs.iter().find(|brief| brief.entry == id)
    }

    /// Marks a user entry as a brief; marking one again replaces its sender.
    pub(crate) fn add_brief(&mut self, brief: Brief) -> Result<(), Error> {
        if !matches!(
            self.entry(brief.entry).map(|entry| &entry.kind),
            Some(EntryKind::User { .. })
        ) {
            return Err(Error::NotPrompt(brief.entry));
        }
        if let Some(known) = self
            .briefs
            .iter_mut()
            .find(|known| known.entry == brief.entry)
        {
            *known = brief;
            return Ok(());
        }
        if self.briefs.len() >= MAX_BRIEFS {
            return Err(Error::Limit);
        }
        self.briefs.push(brief);
        Ok(())
    }

    /// Forgets the briefs of entries dropped from the front.
    pub(crate) fn drop_briefs_before(&mut self, before: EntryId) {
        self.briefs.retain(|brief| brief.entry >= before);
    }
}
