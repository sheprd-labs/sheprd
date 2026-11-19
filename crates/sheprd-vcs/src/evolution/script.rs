//! The Git side of change history, as POSIX shell scripts that run in the
//! checkout's own host, through the same prelude as checkpoints.
//!
//! A branch's history lives in one hidden ref, named like its checkpoints by
//! the hash of the branch name. It points at a commit whose tree holds the
//! history file and whose parents are the commits it says were replaced, so
//! `git gc` keeps them for as long as the ref names them. Writes move the ref
//! only from the value read, so two observers never lose each other's
//! markers: the loser reads again.

use super::{
    Error,
    log::{self, Commit, Log},
};
use crate::{Result, checkpoint::Checkout};
use sheprd_client::shell_quote;

/// Hidden from branch and tag listings, beside checkpoints.
const NAMESPACE: &str = "refs/herdr-gpui/evolution/";
/// Commits on a branch beyond which it is not a stack being worked on (a
/// long-lived branch, or a base that could not be found), so not tracked.
pub(super) const STACK_LIMIT: usize = 200;
const RECORD: char = '\u{1e}';
const UNIT: char = '\u{1f}';

/// Names the branch's history ref `$evo`, and its base `$base`: where it
/// left the default branch (`origin/HEAD`, else `main` or `master`). The
/// default branch itself has no base, so no stack.
fn locate(checkout: &Checkout) -> String {
    format!(
        r#"{prelude}evo={NAMESPACE}$(printf '%s' "$branch" | g hash-object --stdin)
upstream=$(g symbolic-ref -q refs/remotes/origin/HEAD || true)
base_ref=
for candidate in "$upstream" refs/heads/main refs/heads/master; do
    [ -n "$candidate" ] || continue
    if g rev-parse -q --verify "$candidate^{{commit}}" >/dev/null; then base_ref=$candidate; break; fi
done
case "$branch" in main|master|"${{upstream#refs/remotes/origin/}}") base_ref= ;; esac
base=
[ -z "$base_ref" ] || base=$(g merge-base "$base_ref" "refs/heads/$branch" || true)
"#,
        prelude = crate::checkpoint::prelude(checkout),
    )
}

/// The history ref's commit and file, then the branch's own commits newest
/// first with their patch IDs, or `skip` when it has no stack to track.
pub(super) fn read(checkout: &Checkout) -> String {
    format!(
        r#"{locate}printf '\036head\n'
g rev-parse -q --verify "$evo^{{commit}}" || true
printf '\036log\n'
if g rev-parse -q --verify "$evo:log" >/dev/null; then g cat-file blob "$evo:log" | tr -d '\036'; fi
tip=refs/heads/$branch
if [ -z "$base" ] || [ "$(g rev-list --count "$base..$tip")" -gt {STACK_LIMIT} ]; then
    printf '\036skip\n'
    exit 0
fi
printf '\036stack\n'
# Subjects are untrusted and may hold this output's separators: strip them,
# then mark records by the NULs Git puts between commits, which a commit
# message cannot hold.
g log -z --no-show-signature --format='%H%n%(trailers:key=Change-Id,valueonly,separator=%x20)%n%s' "$base..$tip" \
    | tr -d '\036\037' | tr '\000' '\037'
printf '\n'

printf '\036patches\n'
g log --no-show-signature -p --no-ext-diff --no-textconv --no-color --format='commit %H' "$base..$tip" \
    | g patch-id --stable
"#,
        locate = locate(checkout),
    )
}

/// Drop the history of branches that no longer exist; their stacks are
/// merged or abandoned.
const SWEEP: &str = r#"g for-each-ref --format='%(refname)' refs/herdr-gpui/evolution/ \
    | while IFS= read -r ref; do
        name=$(g log -1 --format=%B "$ref" | sed -n 's/^Herdr-Branch: //p' | head -n 1)
        [ -z "$name" ] || g rev-parse -q --verify "refs/heads/$name" >/dev/null \
            || g update-ref -d "$ref"
    done
"#;

/// Store the history file read from stdin, moving the ref only from `old`
/// (`None`: only if it does not exist yet). `kept` become the commit's
/// parents, less any Git has already lost.
pub(super) fn write(checkout: &Checkout, old: Option<&str>, kept: &[&str]) -> String {
    let kept: Vec<&str> = kept
        .iter()
        .copied()
        .filter(|sha| log::is_sha(sha))
        .collect();
    format!(
        r#"{locate}blob=$(g hash-object -w --stdin)
tree=$(printf '100644 blob %s\tlog\n' "$blob" | g mktree)
set --
for kept in {kept}; do
    if g cat-file -e "$kept^{{commit}}" 2>/dev/null; then set -- "$@" -p "$kept"; fi
done
commit=$(printf 'Sheprd change history\n\nHerdr-Branch: %s\n' "$branch" \
    | GIT_AUTHOR_NAME='Sheprd' GIT_AUTHOR_EMAIL='herdr-gpui@localhost' \
      GIT_COMMITTER_NAME='Sheprd' GIT_COMMITTER_EMAIL='herdr-gpui@localhost' \
      git -c core.fsmonitor=false commit-tree --no-gpg-sign "$@" "$tree")
if g update-ref -m 'herdr-gpui: change history' "$evo" "$commit" {old}; then
    printf 'written\n'
else
    printf 'raced\n'
fi
{SWEEP}"#,
        locate = locate(checkout),
        kept = kept.join(" "),
        old = shell_quote(old.unwrap_or_default()),
    )
}

/// Forget the branch's history, as `write` moves it: only from `old`.
pub(super) fn delete(checkout: &Checkout, old: &str) -> String {
    format!(
        r#"{locate}if g update-ref -d "$evo" {old}; then printf 'written\n'; else printf 'raced\n'; fi
{SWEEP}"#,
        locate = locate(checkout),
        old = shell_quote(old),
    )
}

/// `git range-diff` of commit `old` against commit `new`; refuses (as a
/// missing checkpoint does) when Git no longer has either.
pub(super) fn range_diff(checkout: &Checkout, old: &str, new: &str) -> String {
    format!(
        r#"{prelude}for commit in {old} {new}; do
    g cat-file -e "$commit^{{commit}}" 2>/dev/null || exit {missing}
done
# One commit each side: always pair them, however much the rewrite changed.
g range-diff --no-color --creation-factor=999 {old}^..{old} {new}^..{new}
"#,
        prelude = crate::checkpoint::prelude(checkout),
        missing = crate::checkpoint::EXIT_MISSING,
    )
}

/// Other local branches, most recently committed first, each with its own
/// commits since the default branch, newest first.
pub(super) fn branches(checkout: &Checkout) -> String {
    format!(
        r#"{locate}[ -n "$base_ref" ] || exit 0
g for-each-ref --count={BRANCH_LIMIT} --sort=-committerdate --format='%(refname:lstrip=2)' refs/heads/ \
    | while IFS= read -r other; do
        [ "$other" != "$branch" ] || continue
        printf '\036%s\n' "$other"
        g rev-list --max-count={STACK_LIMIT} "$base_ref..refs/heads/$other"
    done
"#,
        locate = locate(checkout),
    )
}

/// Branches [`branches`] looks at; the rest are older.
const BRANCH_LIMIT: usize = 100;

/// [`branches`]' output: each branch and its commits. Git refuses control
/// characters in branch names, so a name is one line.
pub(super) fn parse_branches(output: &str) -> Result<Vec<(String, Vec<String>)>> {
    let mut branches = Vec::new();
    for record in output.split(RECORD).skip(1) {
        let mut lines = record.lines();
        let name = lines
            .next()
            .filter(|name| !name.is_empty())
            .ok_or(Error::Output)?;
        let commits: Vec<String> = lines
            .filter(|line| !line.is_empty())
            .map(|line| {
                log::is_sha(line)
                    .then(|| line.to_owned())
                    .ok_or(Error::Output)
            })
            .collect::<std::result::Result<_, _>>()?;
        branches.push((name.to_owned(), commits));
        if branches.len() > BRANCH_LIMIT {
            return Err(Error::Output.into());
        }
    }
    Ok(branches)
}

/// What [`read`] found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Read {
    /// The history ref's commit, when the branch has one.
    pub head: Option<String>,
    pub log: Log,
    /// The branch's own commits, oldest first; `None` when it has no stack.
    pub stack: Option<Vec<Commit>>,
}

pub(super) fn parse_read(output: &str) -> Result<Read> {
    let mut read = Read::default();
    let mut patches = Vec::new();
    let mut seen = Vec::new();
    for section in output.split(RECORD).skip(1) {
        let (name, body) = section.split_once('\n').unwrap_or((section, ""));
        // Each section once: a repeat could only be forged output.
        if seen.contains(&name) {
            return Err(Error::Output.into());
        }
        seen.push(name);
        match name {
            "head" => {
                let head = body.trim();
                if !head.is_empty() && !log::is_sha(head) {
                    return Err(Error::Output.into());
                }
                read.head = Some(head.to_owned()).filter(|head| !head.is_empty());
            }
            // A history this client cannot read starts again rather than
            // stopping the branch's history for good.
            "log" => {
                read.log = Log::parse(body).unwrap_or_else(|error| {
                    tracing::warn!(category = "evolution", %error, "history unreadable; restarted");
                    Log::default()
                });
            }
            "skip" => read.stack = None,
            "stack" => read.stack = Some(parse_stack(body)?),
            "patches" => patches = parse_patches(body)?,
            _ => return Err(Error::Output.into()),
        }
    }
    if let Some(stack) = &mut read.stack {
        for commit in stack.iter_mut() {
            commit.patch_id = patches
                .iter()
                .find(|(_, sha)| *sha == commit.sha)
                .map(|(id, _)| (*id).to_owned());
        }
    }
    Ok(read)
}

/// `git log` records, newest first, returned oldest first: each the name,
/// the `Change-Id` trailers, and the subject, one line each.
fn parse_stack(body: &str) -> Result<Vec<Commit>> {
    let mut stack = Vec::new();
    for record in body.split(UNIT).map(|record| record.trim_matches('\n')) {
        if record.is_empty() {
            continue;
        }
        if record.lines().count() > 3 {
            return Err(Error::Output.into());
        }
        let mut lines = record.lines();
        let sha = lines
            .next()
            .filter(|sha| log::is_sha(sha))
            .ok_or(Error::Output)?;
        let change_id = lines.next().and_then(log::change_id_of);
        let subject = log::clean(lines.next().unwrap_or_default());
        stack.push(Commit {
            sha: sha.to_owned(),
            patch_id: None,
            change_id,
            subject,
        });
        if stack.len() > STACK_LIMIT {
            return Err(Error::Output.into());
        }
    }
    stack.reverse();
    Ok(stack)
}

/// `git patch-id` lines: the patch ID, then the commit.
fn parse_patches(body: &str) -> Result<Vec<(&str, &str)>> {
    body.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split_once(' ')
                .filter(|(id, sha)| log::is_sha(id) && log::is_sha(sha))
                .ok_or_else(|| Error::Output.into())
        })
        .collect()
}

/// Whether a write moved the ref, or lost to another observer.
pub(super) fn parse_write(output: &str) -> Result<bool> {
    match output
        .lines()
        .find(|line| matches!(*line, "written" | "raced"))
    {
        Some("written") => Ok(true),
        Some(_) => Ok(false),
        None => Err(Error::Output.into()),
    }
}
