//! Which agents' replies the person has read. Each computer's own daemon
//! keeps its agents' marks, by pane as favorites name them
//! (`control::favorites`), so every phone and Sheprd reaching it shows the
//! same unread dots: reading a chat on one device clears it on all of them,
//! and the agent's next reply brings it back everywhere.
//!
//! A reply is known by when its turn ended (`Finish`), which survives the
//! agent restarting where transcript ids start over. A pane is unread while
//! it has a reply other than the one last read, or while someone marked it
//! unread. Replies from before this computer kept marks count as read, so
//! starting to keep them lights up nothing.
//!
//! Herdr 1 numbers panes from one again when it restarts, so the marks
//! remember the primary session's boot, as favorites do, and drop its panes
//! when another boot answers.

use super::favorites::{primary_pane, valid_pane};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The most panes one computer keeps marks for; past it, the oldest go.
pub const MAX_MARKS: usize = 1024;
/// The most panes one change names.
pub const MAX_CHANGE: usize = 1024;

/// Tells one reply of an agent from the next: when its turn ended, if known.
pub type Finish = Option<u64>;

/// A computer's read marks.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Marks {
    /// The primary Herdr 1 session's boot its panes were marked in; none
    /// before one answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
    /// Replies that ended at or before this count as read when their pane
    /// has no mark.
    #[serde(default)]
    pub since_ms: u64,
    /// The same for the primary Herdr 1 session's panes since it last
    /// restarted, whose replies from before are restored, not new.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub primary_since_ms: u64,
    #[serde(default)]
    pub panes: BTreeMap<String, Mark>,
}

fn is_zero(ms: &u64) -> bool {
    *ms == 0
}

/// What was read of one pane.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// The latest reply read; a reply that ended later is news.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Finish,
    /// Marked unread by hand: stays so until read again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
    /// When the mark was made, for keeping the newest.
    #[serde(default)]
    pub at_ms: u64,
}

/// Panes marked read or unread at once.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub panes: Vec<String>,
    pub read: bool,
}

/// The panes unread now, as a daemon answers.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Unread {
    pub panes: BTreeSet<String>,
}

/// An agent's latest reply as marks see it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reply {
    pub finish: Finish,
    /// Whether it has replied since it last worked: a chat once a turn
    /// ended, a terminal agent while Herdr says it is done. A reply with
    /// no known time is not told apart from what was read.
    pub news: bool,
}

impl Marks {
    /// Marks kept from `now_ms` on: earlier replies count as read.
    pub fn starting(now_ms: u64) -> Self {
        Self {
            since_ms: now_ms,
            ..Self::default()
        }
    }

    /// Whether `pane`, whose latest reply is `reply`, is unread: marked so
    /// by hand, or with a reply that ended after the one last read, else
    /// after marks were kept. One with no known time, as when the daemon
    /// found it done on starting, is not news.
    pub fn unread(&self, pane: &str, reply: Reply) -> bool {
        let mark = self.panes.get(pane);
        if mark.is_some_and(|mark| mark.unread) {
            return true;
        }
        let Some(finish) = reply.finish.filter(|_| reply.news) else {
            return false;
        };
        finish
            > mark
                .and_then(|mark| mark.seen)
                .unwrap_or_else(|| self.cutoff(pane))
    }

    /// When replies on `pane` began to count, with no mark there.
    fn cutoff(&self, pane: &str) -> u64 {
        if primary_pane(pane) {
            self.since_ms.max(self.primary_since_ms)
        } else {
            self.since_ms
        }
    }

    /// Marks `pane`, whose latest reply is `reply`, read: up to that reply,
    /// and no longer by hand. What was read stays read when the reply's
    /// time is not known. Returns whether anything changed.
    pub fn read(&mut self, pane: &str, reply: Reply, now_ms: u64) -> bool {
        let finish = reply.finish.filter(|_| reply.news);
        let Some(mark) = self.panes.get_mut(pane) else {
            let Some(finish) = finish else {
                return false;
            };
            self.panes.insert(
                pane.to_owned(),
                Mark {
                    seen: Some(finish),
                    unread: false,
                    at_ms: now_ms,
                },
            );
            self.evict();
            return true;
        };
        let seen = mark.seen.max(finish);
        let changed = mark.unread || seen != mark.seen;
        if changed {
            *mark = Mark {
                seen,
                unread: false,
                at_ms: now_ms,
            };
        }
        changed
    }

    /// Marks `pane` unread by hand, until it is read again. Returns whether
    /// anything changed.
    pub fn mark_unread(&mut self, pane: &str, reply: Reply, now_ms: u64) -> bool {
        let cutoff = self.cutoff(pane);
        let mark = self.panes.entry(pane.to_owned()).or_insert_with(|| Mark {
            // What it would have shown unmarked, so reading it later lands
            // where it was.
            seen: reply.finish.filter(|finish| *finish <= cutoff),
            unread: false,
            at_ms: now_ms,
        });
        let changed = !mark.unread;
        mark.unread = true;
        mark.at_ms = now_ms;
        self.evict();
        changed
    }

    /// Drops the oldest marks past [`MAX_MARKS`], so a long-running daemon
    /// does not keep every pane it ever read.
    fn evict(&mut self) {
        while self.panes.len() > MAX_MARKS {
            let oldest = self
                .panes
                .iter()
                .min_by_key(|(_, mark)| mark.at_ms)
                .map(|(pane, _)| pane.clone());
            let Some(oldest) = oldest else {
                return;
            };
            self.panes.remove(&oldest);
        }
    }

    /// Within bounds: no invalid pane, and at most [`MAX_MARKS`], the
    /// newest kept.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.panes.retain(|pane, _| valid_pane(pane));
        if self.panes.len() > MAX_MARKS {
            let mut marks: Vec<(String, Mark)> =
                std::mem::take(&mut self.panes).into_iter().collect();
            // Newest first; a stable sort keeps ties in pane order.
            marks.sort_by_key(|(_, mark)| std::cmp::Reverse(mark.at_ms));
            marks.truncate(MAX_MARKS);
            self.panes = marks.into_iter().collect();
        }
        self
    }

    /// Follows the primary session's `boot`: when it differs from the one
    /// these were made in, that session's panes are other panes now, and
    /// what they replied before counts as read. Other panes keep their
    /// marks and cutoff. Returns whether anything changed.
    pub fn follow_boot(&mut self, boot: &str, now_ms: u64) -> bool {
        if boot.is_empty() || self.boot.as_deref() == Some(boot) {
            return false;
        }
        if self.boot.is_some() {
            self.panes.retain(|pane, _| !primary_pane(pane));
            self.primary_since_ms = self.primary_since_ms.max(now_ms);
        }
        self.boot = Some(boot.to_owned());
        true
    }
}

impl Change {
    /// Within bounds: at least one pane, at most [`MAX_CHANGE`], each valid.
    pub fn valid(&self) -> bool {
        !self.panes.is_empty()
            && self.panes.len() <= MAX_CHANGE
            && self.panes.iter().all(|pane| valid_pane(pane))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
