//! Change history for a branch before it merges: every version of each of
//! its own commits, which agent's turn rewrote it, and how the rewrite was
//! recognized. The obsolescence markers of Mercurial's changeset evolution,
//! kept for one branch on its own host and forgotten once the branch is
//! merged or deleted; nothing is pushed.
//!
//! Nothing hooks into Git or changes a commit. Each time an agent starts or
//! finishes a turn, the checkpoint worker that snapshots the checkout also
//! compares the branch's commits with those it saw last, and records which
//! replaced which ([`matcher`]). Git keeps every replaced commit through the
//! history ref ([`script`]), so any two versions can still be diffed.

mod error;
pub mod log;
mod matcher;
mod script;
mod stale;
#[cfg(test)]
mod tests;

pub use error::Error;
pub use log::{Commit, Log, Marker, Match, Outcome, new_change_id};
pub use matcher::versions;
pub use stale::{Stale, find as find_stale, stale};

use crate::{Result, checkpoint::Checkout};
use sheprd_client::ScriptLimits;

/// Reads and writes that lose to another observer this many times give up;
/// the next turn tries again.
const ATTEMPTS: usize = 3;

/// Whose turn a look at the branch credits what changed since the last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credit {
    /// A turn is starting: what changed was done outside an agent's turn.
    Outside,
    /// These agents just finished a turn.
    Agents(String),
}

impl Credit {
    /// One look standing for two that were queued for the same checkout.
    #[must_use]
    pub fn merge(self, newer: Self) -> Self {
        match (self, newer) {
            (Self::Agents(older), Self::Agents(newer)) => {
                if older.split(", ").any(|name| name == newer) {
                    Self::Agents(older)
                } else {
                    Self::Agents(log::clean(&format!("{older}, {newer}")))
                }
            }
            (Self::Agents(name), Self::Outside) | (Self::Outside, Self::Agents(name)) => {
                Self::Agents(name)
            }
            (Self::Outside, Self::Outside) => Self::Outside,
        }
    }

    fn by(&self) -> Option<String> {
        match self {
            Self::Outside => None,
            Self::Agents(names) => Some(log::clean(names)).filter(|names| !names.is_empty()),
        }
    }
}

/// What a look at the branch leads to.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Nothing,
    /// Store `log`, moving the ref from `old`.
    Write {
        old: Option<String>,
        log: Log,
    },
    /// The branch has no commits of its own any more: it was merged.
    Delete {
        old: String,
    },
}

fn plan(read: script::Read, credit: &Credit, now: i64) -> Plan {
    let script::Read {
        head,
        mut log,
        stack,
    } = read;
    let Some(stack) = stack else {
        return Plan::Nothing;
    };
    if stack.is_empty() {
        return head.map_or(Plan::Nothing, |old| Plan::Delete { old });
    }
    let changes = matcher::evolve(&log.stack, &stack);
    if changes.is_empty() && log.stack == stack && head.is_some() {
        return Plan::Nothing;
    }
    let by = credit.by();
    log.record(changes.into_iter().map(|(old, outcome)| Marker {
        time: now,
        old,
        outcome,
        by: by.clone(),
    }));
    log.stack = stack;
    Plan::Write { old: head, log }
}

/// Look at `checkout`'s branch and record what replaced what since the last
/// look, crediting `credit`. `run` runs a script on the checkout's host with
/// the given stdin, as [`crate::checkpoint::direct::here_with_input`] does.
/// Blocking: run on a checkpoint worker or the command line.
pub fn observe(
    checkout: &Checkout,
    credit: &Credit,
    now: i64,
    mut run: impl FnMut(&Checkout, &str, &[u8], ScriptLimits) -> Result<String>,
) -> Result<()> {
    let limits = crate::checkpoint::LIMITS;
    for _ in 0..ATTEMPTS {
        let read = script::parse_read(&run(checkout, &script::read(checkout), b"", limits)?)?;
        let output = match plan(read, credit, now) {
            Plan::Nothing => return Ok(()),
            Plan::Write { old, log } => {
                let body = script::write(checkout, old.as_deref(), &log.kept());
                run(checkout, &body, log.render().as_bytes(), limits)?
            }
            Plan::Delete { old } => run(checkout, &script::delete(checkout, &old), b"", limits)?,
        };
        if script::parse_write(&output)? {
            return Ok(());
        }
    }
    Err(Error::Raced.into())
}

/// A branch's history as it stands now, for reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    /// The branch's own commits, oldest first.
    pub stack: Vec<Commit>,
    /// Recorded markers, then those since the last look.
    pub markers: Vec<Marker>,
    /// How many of the last `markers` happened since the last look, so are
    /// not credited to anyone yet.
    pub pending: usize,
}

impl History {
    /// Whether `marker`, one of [`History::markers`], is not recorded yet.
    pub fn is_pending(&self, marker: &Marker) -> bool {
        let start = self.markers.len().saturating_sub(self.pending);
        self.markers[start..]
            .iter()
            .any(|pending| std::ptr::eq(pending, marker))
    }

    /// The one commit, on the branch now or an earlier version of one,
    /// whose name starts with `prefix`.
    pub fn resolve(&self, prefix: &str) -> Result<&str> {
        let mut found: Vec<&str> = Vec::new();
        let names = self.stack.iter().map(|commit| commit.sha.as_str()).chain(
            self.markers.iter().flat_map(|marker| {
                let new = match &marker.outcome {
                    Outcome::Replaced { new, .. } => Some(new.as_str()),
                    Outcome::Dropped => None,
                };
                std::iter::once(marker.old.as_str()).chain(new)
            }),
        );
        for name in names.filter(|name| name.starts_with(prefix)) {
            if !found.contains(&name) {
                found.push(name);
            }
        }
        match found.as_slice() {
            [one] => Ok(one),
            [] => Err(Error::Unknown.into()),
            _ => Err(Error::Ambiguous.into()),
        }
    }

    /// What `sha` became: the commit on the branch now that its markers
    /// lead to, or `None` when it was dropped on the way.
    pub fn current<'a>(&'a self, sha: &'a str) -> Option<&'a str> {
        let mut current = sha;
        let mut seen = std::collections::HashSet::new();
        loop {
            if self.stack.iter().any(|commit| commit.sha == current) {
                return Some(current);
            }
            if !seen.insert(current) {
                return None;
            }
            let marker = self
                .markers
                .iter()
                .rev()
                .find(|marker| marker.old == current)?;
            match &marker.outcome {
                Outcome::Replaced { new, .. } => current = new,
                Outcome::Dropped => return None,
            }
        }
    }
}

/// How one version's patch differs from another's: `git range-diff` of the
/// two commits, so a rebase onto a moved base shows only what the rewrite
/// itself changed. Both must be full names, as [`History::resolve`] gives.
pub fn diff(
    checkout: &Checkout,
    old: &str,
    new: &str,
    mut run: impl FnMut(&Checkout, &str, &[u8], ScriptLimits) -> Result<String>,
) -> Result<String> {
    if !log::is_sha(old) || !log::is_sha(new) {
        return Err(Error::Unknown.into());
    }
    let limits = ScriptLimits {
        output: 8 * 1024 * 1024,
        idle: crate::checkpoint::LIMITS.idle,
    };
    run(
        checkout,
        &script::range_diff(checkout, old, new),
        b"",
        limits,
    )
}

/// `checkout`'s branch history, with what changed since the last look but
/// without recording it. `None` when the branch has no commits of its own
/// to track: it is the default branch, or has no base. Reads only; `now`
/// stamps the markers not recorded yet.
pub fn history(
    checkout: &Checkout,
    now: i64,
    mut run: impl FnMut(&Checkout, &str, &[u8], ScriptLimits) -> Result<String>,
) -> Result<Option<History>> {
    let limits = crate::checkpoint::LIMITS;
    let read = script::parse_read(&run(checkout, &script::read(checkout), b"", limits)?)?;
    let Some(stack) = read.stack else {
        return Ok(None);
    };
    let mut markers = read.log.markers;
    let pending: Vec<Marker> = matcher::evolve(&read.log.stack, &stack)
        .into_iter()
        .map(|(old, outcome)| Marker {
            time: now,
            old,
            outcome,
            by: None,
        })
        .collect();
    let count = pending.len();
    markers.extend(pending);
    Ok(Some(History {
        stack,
        markers,
        pending: count,
    }))
}
