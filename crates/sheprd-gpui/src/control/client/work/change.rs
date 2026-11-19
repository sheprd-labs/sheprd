//! `change log`, run in this process for the checkout of the current
//! directory with the scripts the app's checkpoint workers run, so Sheprd
//! need not run. Reads only: what changed since the last turn is shown as
//! not recorded yet, and recorded by the app at the next turn.
use super::{
    super::Output,
    checkpoint::{here, now},
    rejected,
};
use crate::{
    checkpoint::{Checkout, direct},
    cli::control::ChangeCommand,
    control::ErrorCode,
};
use serde::Serialize;
use sheprd_vcs::evolution::{self, History, Marker, Match, Outcome};
use std::fmt::Write as _;

/// How a version's replacement was recognized, or that it had none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum How {
    ChangeId,
    Patch,
    Subject,
    Dropped,
}

impl From<&Outcome> for How {
    fn from(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::Replaced { how, .. } => match how {
                Match::ChangeId => Self::ChangeId,
                Match::Patch => Self::Patch,
                Match::Subject => Self::Subject,
            },
            Outcome::Dropped => Self::Dropped,
        }
    }
}

/// One earlier version of a commit, as `change log --json` prints it.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct VersionInfo {
    pub sha: String,
    /// Unix seconds when its replacement was seen.
    pub replaced: i64,
    pub how: How,
    /// The agents whose turn it was, if any.
    pub by: Option<String>,
    /// Seen since the last turn, so not recorded or credited yet.
    pub pending: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub change_id: Option<String>,
    /// Newest first.
    pub versions: Vec<VersionInfo>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct ChangeLog {
    /// Newest first.
    pub commits: Vec<CommitInfo>,
    /// Commits that left the branch with nothing recognized as their
    /// successor, newest first.
    pub dropped: Vec<VersionInfo>,
}

fn version(history: &History, marker: &Marker) -> VersionInfo {
    VersionInfo {
        sha: marker.old.clone(),
        replaced: marker.time,
        how: How::from(&marker.outcome),
        by: marker.by.clone(),
        pending: history.is_pending(marker),
    }
}

/// The branch's commits newest first, each with its versions; only those
/// whose name starts with `prefix` when one is given, and then no drops.
pub(super) fn change_log(history: &History, prefix: Option<&str>) -> ChangeLog {
    let commits = history
        .stack
        .iter()
        .rev()
        .filter(|commit| prefix.is_none_or(|prefix| commit.sha.starts_with(prefix)))
        .map(|commit| CommitInfo {
            sha: commit.sha.clone(),
            subject: commit.subject.clone(),
            change_id: commit.change_id.clone(),
            versions: evolution::versions(&history.markers, &commit.sha)
                .into_iter()
                .map(|marker| version(history, marker))
                .collect(),
        })
        .collect();
    let dropped = match prefix {
        Some(_) => Vec::new(),
        None => history
            .markers
            .iter()
            .rev()
            .filter(|marker| marker.outcome == Outcome::Dropped)
            .map(|marker| version(history, marker))
            .collect(),
    };
    ChangeLog { commits, dropped }
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// "amended by Codex, 5 min ago", or "rebased, not recorded yet".
fn describe_version(version: &VersionInfo, now: i64) -> String {
    let how = match version.how {
        How::ChangeId => "rewritten (same Change-Id)",
        How::Patch => "rebased or reworded",
        How::Subject => "amended",
        How::Dropped => "dropped",
    };
    if version.pending {
        return format!("{how}, not recorded yet");
    }
    let by = version.by.as_deref().map_or_else(
        || "outside an agent's turn".to_owned(),
        |by| format!("in {by}'s turn"),
    );
    let ago = crate::ago::english(crate::ago::since(version.replaced, now));
    format!("{how} {by}, {ago}")
}

pub(super) fn describe(log: &ChangeLog, branch: &str, now: i64) -> String {
    if log.commits.is_empty() && log.dropped.is_empty() {
        return format!("{branch} has no commits of its own");
    }
    let mut text = String::new();
    for commit in &log.commits {
        let _ = write!(text, "{}  {}", short(&commit.sha), commit.subject);
        match commit.versions.len() {
            0 => text.push('\n'),
            count => {
                let _ = writeln!(text, "  ({} versions)", count + 1);
            }
        }
        for version in &commit.versions {
            let _ = writeln!(
                text,
                "    was {}  {}",
                short(&version.sha),
                describe_version(version, now)
            );
        }
    }
    if !log.dropped.is_empty() {
        text.push_str("Dropped:\n");
        for version in &log.dropped {
            let _ = writeln!(
                text,
                "    {}  {}",
                short(&version.sha),
                describe_version(version, now)
            );
        }
    }
    text.trim_end().to_owned()
}

pub(in crate::control::client) fn change(command: ChangeCommand) -> crate::Result<Output> {
    match command {
        ChangeCommand::Log { commit } => log(&here()?, commit.as_deref()),
        ChangeCommand::Diff { old, new } => diff(&here()?, &old, new.as_deref()),
        ChangeCommand::Id => id(),
        ChangeCommand::Stale => stale(&here()?),
    }
}

/// One branch as `change stale --json` prints it.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct StaleInfo {
    pub branch: String,
    pub old: String,
    /// `None`: the version it is built on was dropped.
    pub new: Option<String>,
    pub command: Option<String>,
}

pub(super) fn describe_stale(stale: &[StaleInfo], branch: &str) -> String {
    if stale.is_empty() {
        return format!("No other branch is built on an earlier version of {branch}'s commits");
    }
    let mut text = String::new();
    for info in stale {
        match (&info.new, &info.command) {
            (Some(new), Some(command)) => {
                let _ = writeln!(
                    text,
                    "{} is built on {}, now {}\n    {command}",
                    info.branch,
                    short(&info.old),
                    short(new)
                );
            }
            _ => {
                let _ = writeln!(
                    text,
                    "{} is built on {}, which was dropped from {branch}",
                    info.branch,
                    short(&info.old)
                );
            }
        }
    }
    text.trim_end().to_owned()
}

fn stale(checkout: &Checkout) -> crate::Result<Output> {
    let stale: Vec<StaleInfo> = evolution::stale(checkout, now(), direct::here_with_input)
        .map_err(rejected)?
        .into_iter()
        .map(|stale| StaleInfo {
            command: stale.command(),
            branch: stale.branch,
            old: stale.old,
            new: stale.new,
        })
        .collect();
    Output::data(
        describe_stale(&stale, &checkout.branch),
        &serde_json::json!({ "branch": checkout.branch, "stale": stale }),
    )
}

/// A fresh `Change-Id`, for `git commit --trailer "Change-Id: $(sheprd
/// change id)"`; needs no checkout.
fn id() -> crate::Result<Output> {
    let mut random = [0; 20];
    getrandom::fill(&mut random).map_err(|error| crate::control::Error::Rejected {
        code: ErrorCode::Failed,
        message: format!("No randomness for a Change-Id: {error}"),
    })?;
    let id = evolution::new_change_id(random);
    Output::data(id.clone(), &serde_json::json!({ "change_id": id }))
}

/// The range-diff from version `old` to `new`, or to what `old` became.
fn diff(checkout: &Checkout, old: &str, new: Option<&str>) -> crate::Result<Output> {
    let history = evolution::history(checkout, now(), direct::here_with_input)
        .map_err(rejected)?
        .ok_or_else(|| untracked(checkout))?;
    let old = history.resolve(old).map_err(rejected)?;
    let new = match new {
        Some(new) => history.resolve(new).map_err(rejected)?,
        None => history.current(old).ok_or_else(|| {
            crate::Error::from(crate::control::Error::Rejected {
                code: ErrorCode::NotFound,
                message: format!(
                    "{} was dropped from {}; name the version to compare it with",
                    short(old),
                    checkout.branch
                ),
            })
        })?,
    };
    let text = evolution::diff(checkout, old, new, direct::here_with_input).map_err(rejected)?;
    let json = serde_json::json!({ "old": old, "new": new, "range_diff": text });
    let mut output = Output::data(text.trim_end().to_owned(), &json)?;
    if output.text.is_empty() {
        output.text = "No changes".into();
    }
    Ok(output)
}

fn untracked(checkout: &Checkout) -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::InvalidRequest,
        message: format!(
            "{} is the default branch, or has none to leave, so its commits are not tracked",
            checkout.branch
        ),
    }
    .into()
}

fn log(checkout: &Checkout, prefix: Option<&str>) -> crate::Result<Output> {
    let now = now();
    let Some(history) =
        evolution::history(checkout, now, direct::here_with_input).map_err(rejected)?
    else {
        return Output::data(
            format!(
                "{} is the default branch, or has none to leave, so its commits are not tracked",
                checkout.branch
            ),
            &serde_json::json!({
                "repository": checkout.repo_key,
                "branch": checkout.branch,
                "tracked": false,
            }),
        );
    };
    let log = change_log(&history, prefix);
    if let (Some(prefix), true) = (prefix, log.commits.is_empty()) {
        return Err(crate::control::Error::Rejected {
            code: ErrorCode::NotFound,
            message: format!(
                "No commit of {}'s own starts with {prefix}",
                checkout.branch
            ),
        }
        .into());
    }
    Output::data(
        describe(&log, &checkout.branch, now),
        &serde_json::json!({
            "repository": checkout.repo_key,
            "branch": checkout.branch,
            "tracked": true,
            "commits": log.commits,
            "dropped": log.dropped,
        }),
    )
}

#[cfg(test)]
mod tests;
