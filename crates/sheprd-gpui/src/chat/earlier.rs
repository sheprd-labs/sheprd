//! The turns a continued conversation brought with it, such as a terminal
//! agent's moved into this chat, folded under one header until opened. The
//! last of them stays in view, so the chat opens where the agent left off
//! rather than on a wall of history.

use std::collections::HashSet;

use sheprd_chat_protocol::{EntryId, EntryKind, Transcript};

/// Where the folded turns end, and whether they show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Earlier {
    /// The notice under the brought conversation, whose expansion opens the
    /// fold.
    pub(crate) divider: EntryId,
    /// Whether the folded turns show.
    pub(crate) open: bool,
    /// The index of the brought conversation's last prompt: rows before it
    /// fold, and its row carries the header.
    pub(crate) start: usize,
    /// How many prompts the fold holds.
    pub(crate) turns: usize,
}

impl Earlier {
    /// Whether the row at `index` is folded away.
    pub(crate) fn hides(&self, index: usize) -> bool {
        !self.open && index < self.start
    }
}

/// The fold over `transcript`'s brought conversation, opened when its
/// divider is in `expanded`, or `None` when nothing comes before its last
/// prompt.
pub(crate) fn earlier(transcript: &Transcript, expanded: &HashSet<EntryId>) -> Option<Earlier> {
    let divider = transcript.session.as_ref()?.continued?;
    let entries = &transcript.entries;
    let at = entries.iter().position(|entry| entry.id == divider)?;
    let start = entries[..at]
        .iter()
        .rposition(|entry| prompt(&entry.kind))?;
    if start == 0 {
        return None;
    }
    Some(Earlier {
        divider,
        open: expanded.contains(&divider),
        start,
        turns: entries[..start]
            .iter()
            .filter(|entry| prompt(&entry.kind))
            .count(),
    })
}

fn prompt(kind: &EntryKind) -> bool {
    matches!(kind, EntryKind::User { .. })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
