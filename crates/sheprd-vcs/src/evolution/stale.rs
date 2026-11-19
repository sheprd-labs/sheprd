//! Branches stacked on an earlier version of this branch's commits: Git
//! rebases a commit's descendants on its own branch, but another branch
//! built on it keeps the old version. Found, never moved: each comes with
//! the `git rebase --onto` that would carry it to the current version.

use super::{History, script};
use crate::{Result, checkpoint::Checkout};
use sheprd_client::{ScriptLimits, shell_quote};
use std::collections::HashSet;

/// A local branch whose commits include a replaced version of one of this
/// branch's commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stale {
    pub branch: String,
    /// The nearest replaced version the branch is built on.
    pub old: String,
    /// What it became on this branch; `None` when it was dropped.
    pub new: Option<String>,
}

impl Stale {
    /// The rebase that moves the branch's own commits from `old` onto
    /// `new`, for the caller to run; none for a dropped commit.
    pub fn command(&self) -> Option<String> {
        let new = self.new.as_deref()?;
        let word = |text: &str| {
            if text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
            {
                text.to_owned()
            } else {
                shell_quote(text)
            }
        };
        Some(format!(
            "git rebase --onto {} {} {}",
            &new[..12.min(new.len())],
            &self.old[..12.min(self.old.len())],
            word(&self.branch)
        ))
    }
}

/// Which of `branches` (each with its commits, newest first) are built on
/// a version `history` says was replaced.
pub fn find(history: &History, branches: &[(String, Vec<String>)]) -> Vec<Stale> {
    let current: HashSet<&str> = history
        .stack
        .iter()
        .map(|commit| commit.sha.as_str())
        .collect();
    let replaced: HashSet<&str> = history
        .markers
        .iter()
        .map(|marker| marker.old.as_str())
        .filter(|old| !current.contains(old))
        .collect();
    branches
        .iter()
        .filter_map(|(branch, commits)| {
            let old = commits
                .iter()
                .find(|commit| replaced.contains(commit.as_str()))?;
            Some(Stale {
                branch: branch.clone(),
                old: old.clone(),
                new: history.current(old).map(str::to_owned),
            })
        })
        .collect()
}

/// The local branches stacked on a replaced version of `checkout`'s
/// branch's commits. Reads only.
pub fn stale(
    checkout: &Checkout,
    now: i64,
    mut run: impl FnMut(&Checkout, &str, &[u8], ScriptLimits) -> Result<String>,
) -> Result<Vec<Stale>> {
    let Some(history) = super::history(checkout, now, &mut run)? else {
        return Ok(Vec::new());
    };
    // Nothing replaced: no branch can be built on an old version.
    if history.markers.is_empty() {
        return Ok(Vec::new());
    }
    let output = run(
        checkout,
        &script::branches(checkout),
        b"",
        crate::checkpoint::LIMITS,
    )?;
    Ok(find(&history, &script::parse_branches(&output)?))
}
