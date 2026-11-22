//! Restacking: once the layer below a branch merged into the stack's base
//! (squashed or rebased into new commits), the branch still carries that
//! layer's old commits. Restacking replays only the branch's own commits, the
//! ones after the merged layer's head, onto the base as its remote has it now.
//!
//! Local only: it never pushes. A dirty checkout is refused rather than
//! stashed, and a conflict leaves the rebase stopped for the user or an agent
//! to resolve, never aborted or retried here.

mod error;

pub use error::Error;

use super::{Outcome, command, git};
use sheprd_github::pull_request::{base_repository, clean, run};
use std::time::Instant;

/// What a restack did, for the chrome to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Restacked {
    /// `branch` now sits on `remote`/`base` with only its own commits.
    Onto {
        branch: String,
        remote: String,
        base: String,
    },
    /// `branch` no longer carried the merged commits; nothing changed.
    Unchanged { branch: String },
}

impl Restacked {
    /// The English the worker reports, for logs and tests.
    fn message(&self) -> String {
        match self {
            Self::Onto {
                branch,
                remote,
                base,
            } => format!(
                "Restacked {branch} onto {remote}/{base}; push it with --force-with-lease when ready"
            ),
            Self::Unchanged { branch } => {
                format!("Nothing to restack: {branch} no longer carries the merged commits")
            }
        }
    }
}

impl From<Restacked> for Outcome {
    fn from(restacked: Restacked) -> Self {
        Outcome {
            message: restacked.message(),
            url: None,
            restacked: Some(restacked),
        }
    }
}

/// Where restacks fetch the stack's base, each into a ref of its own.
const ONTO_PREFIX: &str = "refs/sheprd/restack";

/// A ref name no other restack uses: refs are shared by every worktree of a
/// repository, and each window runs its own Git worker.
fn onto_ref() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{ONTO_PREFIX}/{}-{nanos}-{count}", std::process::id())
}

pub(super) fn restack(
    checkout: &str,
    branch: &str,
    below: &str,
    base: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    if !commit_id(below) || !branch_name(base) {
        return Err(super::Error::from(Error::Target).into());
    }
    if super::checkout_dirty(checkout, deadline, cancelled)? {
        return Err(super::Error::from(Error::Dirty).into());
    }
    // The branch and commit confirmed; a terminal or agent may move them
    // while the fetch runs, so they are read again before rewriting.
    let head = checked_out(checkout, branch, deadline, cancelled)?;
    if !carries(checkout, below, deadline, cancelled)? {
        return Ok(Restacked::Unchanged {
            branch: branch.to_owned(),
        }
        .into());
    }
    let remote = base_repository(checkout, deadline, cancelled)?
        .remote
        .name();
    // The base lands in a ref of this action's own, and the rebase names the
    // commit it captured: a fetch run elsewhere in this checkout meanwhile
    // rewrites `FETCH_HEAD`, and a restack in another worktree its own ref,
    // never this one.
    let onto_ref = onto_ref();
    let refspec = format!("+refs/heads/{base}:{onto_ref}");
    git(
        checkout,
        &["fetch", "--no-tags", remote, &refspec],
        "fetch the stack's base",
        deadline,
        cancelled,
    )?;
    let onto = git(
        checkout,
        &["rev-parse", "--verify", &format!("{onto_ref}^{{commit}}")],
        "read the stack's base",
        deadline,
        cancelled,
    );
    let _ = git(
        checkout,
        &["update-ref", "-d", &onto_ref],
        "forget the fetched base",
        deadline,
        cancelled,
    );
    let onto = onto?;
    if !commit_id(&onto) {
        return Err(super::Error::from(Error::Target).into());
    }
    // The fetch may take a while: the branch, its commit, and the working
    // tree are read again right before history is rewritten.
    if checked_out(checkout, branch, deadline, cancelled)? != head {
        return Err(super::Error::from(Error::Moved).into());
    }
    if super::checkout_dirty(checkout, deadline, cancelled)? {
        return Err(super::Error::from(Error::Dirty).into());
    }
    // A rebase replays a merge commit's parents' changes but drops the merge
    // itself, and with it a resolution made there, so a branch whose own
    // commits include one is left for a person to rebase.
    let merges = git(
        checkout,
        &[
            "rev-list",
            "--merges",
            "--count",
            &format!("{below}..{head}"),
        ],
        "list the branch's merges",
        deadline,
        cancelled,
    )?;
    if merges.trim() != "0" {
        return Err(super::Error::from(Error::Merges).into());
    }
    // A rebase that is not interactive asks for no message, but a hook or a
    // setting could still start an editor nobody would see. `autoStash`
    // would hide edits instead of refusing them, and `updateRefs` would
    // move other local branches on the replayed commits; this changes the
    // checkout's own branch only. (Settings, not options, so an older Git
    // ignores the one it lacks.)
    let mut rebase = command(
        checkout,
        &[
            "-c",
            "rebase.autoStash=false",
            "-c",
            "rebase.updateRefs=false",
            "rebase",
            "--onto",
            &onto,
            below,
        ],
    );
    rebase.env("GIT_EDITOR", "true");
    let (ok, output) = run(&mut rebase, deadline, cancelled)?;
    if !ok {
        let stopped = Error::Stopped {
            details: clean(output.trim()),
        };
        return Err(super::Error::from(stopped).into());
    }
    Ok(Restacked::Onto {
        branch: branch.to_owned(),
        remote: remote.to_owned(),
        base: base.to_owned(),
    }
    .into())
}

/// HEAD's commit, once `branch` is checked out; else the checkout moved.
fn checked_out(
    checkout: &str,
    branch: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<String> {
    let (on_branch, current) = run(
        &mut command(checkout, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
        deadline,
        cancelled,
    )?;
    if !on_branch || current.trim() != branch {
        return Err(super::Error::from(Error::Moved).into());
    }
    git(
        checkout,
        &["rev-parse", "HEAD"],
        "read HEAD",
        deadline,
        cancelled,
    )
}

/// Whether HEAD still has `below` in its history. Git exits 1 with nothing
/// to say when it does not, and otherwise fails with a reason.
fn carries(
    checkout: &str,
    below: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<bool> {
    let (ok, output) = run(
        &mut command(checkout, &["merge-base", "--is-ancestor", below, "HEAD"]),
        deadline,
        cancelled,
    )?;
    if ok {
        return Ok(true);
    }
    if output.trim().is_empty() {
        return Ok(false);
    }
    Err(super::Error::Failed {
        operation: "read the branch's history",
        details: clean(output.trim()),
    }
    .into())
}

/// A full commit ID in lowercase hex, as GitHub reports one.
pub(super) fn commit_id(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A branch name safe to name in a refspec: no option, range, revision
/// syntax, or control character.
pub(super) fn branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("@{")
        && !name.ends_with(".lock")
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}

#[cfg(test)]
mod tests;
