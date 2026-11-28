//! Where a phone's new worktree may come from, as the desktop's new
//! worktree dialog offers it: a new branch, a local branch no checkout has,
//! a checkout of the repository no workspace has open, or an open pull
//! request. `GET /api/v2/new/sources` lists the local ones and names the
//! repository the phone lists pull requests and issues of itself; a pull
//! request's head is fetched here before its worktree is made, into the
//! same refs the desktop fetches it to.

use crate::native::{
    changes::local::git,
    github::{checkouts, remote::Repository as Remote},
};
use crate::web::{ApiError, Device, Shared, folders, valid_branch};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sheprd_client::herdr::worktree;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Local branches listed at most, most recently committed first.
const MAX_BRANCHES: usize = 300;
/// Checkouts listed at most.
const MAX_CHECKOUTS: usize = 200;
/// How long fetching a pull request's head may take: the network, and a
/// credential helper that answers without asking. The phone waits this
/// long beyond its usual wait for a new workspace (`RemoteClient`'s
/// `createWorkspace`), so it never gives up on a fetch still running.
const FETCH_WAIT: Duration = Duration::from_secs(60);

/// Why a source could not be listed or used.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("Git could not fetch the pull request: {0}")]
    Fetch(#[source] worktree::Error),
    #[error("that repository has no origin or upstream remote to fetch the pull request from")]
    NoRemote,
    #[error("that is not a checkout of this repository")]
    NotACheckout,
    #[error("Git could not read this repository")]
    Read,
    // keep-sorted end
}

impl Error {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::NoRemote | Self::NotACheckout => StatusCode::BAD_REQUEST,
            Self::Fetch(_) => StatusCode::BAD_GATEWAY,
            Self::Read => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

/// Where a new worktree comes from, besides a new branch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Source {
    /// An open pull request of [`Sources::repository`], by its number, its
    /// head branch, and who owns the repository that branch lives in
    /// (`None` when GitHub no longer says, as for a deleted fork).
    Pull {
        number: u64,
        head: String,
        #[serde(default)]
        head_owner: Option<String>,
    },
    /// A checkout [`Sources::checkouts`] listed.
    Checkout { path: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct SourcesQuery {
    cwd: String,
}

/// What `GET /api/v2/new/sources` answers for a folder.
#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Sources {
    /// Local branches no checkout has, most recently committed first.
    pub(crate) branches: Vec<String>,
    /// Linked checkouts of the repository no workspace has open.
    pub(crate) checkouts: Vec<Listed>,
    /// The repository pull requests are listed from and fetched from:
    /// `upstream`, else `origin`. `None` when neither names one.
    pub(crate) repository: Option<Repository>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Listed {
    pub(crate) path: String,
    /// `None` on a detached HEAD.
    pub(crate) branch: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Repository {
    pub(crate) host: String,
    pub(crate) owner: String,
    pub(crate) repo: String,
}

/// `GET /api/v2/new/sources?cwd=FOLDER`.
pub(crate) async fn new_sources(
    State(state): State<Shared>,
    _device: Device,
    Query(query): Query<SourcesQuery>,
) -> Result<Json<Sources>, ApiError> {
    let layout = state.hub.layout();
    let listed = tokio::task::spawn_blocking(move || {
        if !folders(&layout).offers(&query.cwd) {
            return Err(ApiError::Folder);
        }
        let open: Vec<&str> = layout
            .workspaces
            .iter()
            .map(|workspace| workspace.cwd.as_str())
            .collect();
        read_sources(Path::new(&query.cwd), &open).map_err(ApiError::from)
    })
    .await
    .map_err(|_| ApiError::Folder)??;
    Ok(Json(listed))
}

/// The sources of the repository at `cwd`, leaving out checkouts whose
/// folder is one of `open`. Blocking.
fn read_sources(cwd: &Path, open: &[&str]) -> Result<Sources, Error> {
    let worktrees = list_worktrees(cwd)?;
    let refs = git(
        cwd,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--count=2000",
            "--format=%(refname)",
            "refs/heads",
        ],
    )
    .map_err(|_| Error::Read)?;
    let open: HashSet<PathBuf> = open.iter().map(|path| canonical(Path::new(path))).collect();
    Ok(Sources {
        branches: free_branches(&refs, &worktrees.branches),
        checkouts: worktrees
            .linked
            .into_iter()
            .filter(|entry| !open.contains(&canonical(Path::new(&entry.path))))
            .take(MAX_CHECKOUTS)
            .collect(),
        repository: checkouts::read(cwd).and_then(|checkout| {
            let repository = checkout.upstream.or(checkout.origin)?;
            Some(Repository {
                host: repository.host,
                owner: repository.owner,
                repo: repository.repo,
            })
        }),
    })
}

/// What `git worktree list` says of a repository.
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct Worktrees {
    /// The branch of every checkout, the repository's own included.
    pub(super) branches: HashSet<String>,
    /// Its linked checkouts that still exist, in Git's order.
    pub(super) linked: Vec<Listed>,
}

fn list_worktrees(cwd: &Path) -> Result<Worktrees, Error> {
    let listing = git(cwd, &["worktree", "list", "--porcelain", "-z"]).map_err(|_| Error::Read)?;
    Ok(parse_worktrees(&listing))
}

/// `git worktree list --porcelain -z`: fields end in NUL, and an empty
/// field ends each checkout, so paths come as they are, never quoted. The
/// first checkout is the repository's own, bare or not.
pub(super) fn parse_worktrees(listing: &str) -> Worktrees {
    let mut worktrees = Worktrees::default();
    let records = listing
        .split("\0\0")
        .filter(|record| !record.trim_matches('\0').is_empty());
    for (index, record) in records.enumerate() {
        let mut path = None;
        let mut branch = None;
        let mut gone = false;
        for field in record.split('\0') {
            if let Some(rest) = field.strip_prefix("worktree ") {
                path = Some(rest.to_owned());
            } else if let Some(rest) = field.strip_prefix("branch refs/heads/") {
                branch = Some(rest.to_owned());
            } else if field == "bare" || field.starts_with("prunable") {
                gone = true;
            }
        }
        if let Some(branch) = &branch {
            worktrees.branches.insert(branch.clone());
        }
        if let (Some(path), false, true) = (path, gone, index > 0) {
            worktrees.linked.push(Listed { path, branch });
        }
    }
    worktrees
}

/// The local branches in `refs` (full names, one a line) that no checkout
/// has (`taken`), that a request may name.
pub(super) fn free_branches(refs: &str, taken: &HashSet<String>) -> Vec<String> {
    refs.lines()
        .filter_map(|line| line.trim().strip_prefix("refs/heads/"))
        .filter(|branch| !taken.contains(*branch) && valid_branch(branch))
        .take(MAX_BRANCHES)
        .map(str::to_owned)
        .collect()
}

/// The checkout `path` names among the repository at `cwd`'s linked ones,
/// as Git lists it, so a phone can only open one that exists. Blocking.
pub(super) fn checkout_of(cwd: &Path, path: &str) -> Result<String, Error> {
    let wanted = canonical(Path::new(path));
    list_worktrees(cwd)?
        .linked
        .into_iter()
        .find(|entry| canonical(Path::new(&entry.path)) == wanted)
        .map(|entry| entry.path)
        .ok_or(Error::NotACheckout)
}

/// Whether `name` is a branch Git itself allows (`git check-ref-format
/// --branch`), wider than the names a phone may type for a new branch: a
/// pull request's head may be any of them.
pub(super) fn git_branch(name: &str) -> bool {
    const REFUSED: [char; 8] = [' ', '~', '^', ':', '?', '*', '[', '\\'];
    !name.is_empty()
        && name.len() <= 255
        && name != "@"
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("@{")
        && !name.contains("//")
        && !name
            .chars()
            .any(|c| c.is_ascii_control() || REFUSED.contains(&c))
        && name
            .split('/')
            .all(|part| !part.starts_with('.') && !part.ends_with(".lock"))
}

/// A pull request's worktree: its branch, and the ref its head was just
/// fetched to, which the branch starts from when it is new.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Fetched {
    pub(super) branch: String,
    pub(super) base: String,
}

/// The branch and refs a pull request's head is fetched with from
/// `remote`, as the desktop names them: a fork's head under GitHub's pull
/// ref, on a `pr/NUMBER` branch, else the head branch itself.
pub(super) fn pull_refs(
    remote: &str,
    number: u64,
    head: &str,
    fork: bool,
) -> Option<(Fetched, String)> {
    let (branch, source, base) = if fork {
        (
            format!("pr/{number}"),
            format!("refs/pull/{number}/head"),
            format!("refs/herdr/pull/{number}/head"),
        )
    } else {
        if !git_branch(head) {
            return None;
        }
        (
            head.to_owned(),
            format!("refs/heads/{head}"),
            format!("refs/remotes/{remote}/{head}"),
        )
    };
    let refspec = format!("+{source}:{base}");
    Some((Fetched { branch, base }, refspec))
}

/// Where a pull request's head is fetched from, as the desktop decides: a
/// head on the listed repository, or on this checkout's own fork, is an
/// ordinary branch of that remote; any other owner's is a fork's, fetched
/// through the listed repository's pull ref. Answers the remote and
/// whether the head is a fork's.
pub(super) fn pull_remote(
    checkout: &checkouts::Checkout,
    head_owner: Option<&str>,
) -> Option<(&'static str, bool)> {
    let (listed, remote) = match (&checkout.upstream, &checkout.origin) {
        (Some(upstream), _) => (upstream, "upstream"),
        (None, Some(origin)) => (origin, "origin"),
        (None, None) => return None,
    };
    let owns = |repository: &Option<Remote>, owner: &str| {
        repository
            .as_ref()
            .is_some_and(|repository| repository.owner.eq_ignore_ascii_case(owner))
    };
    Some(match head_owner {
        Some(owner) if listed.owner.eq_ignore_ascii_case(owner) => (remote, false),
        Some(owner) if remote == "upstream" && owns(&checkout.origin, owner) => ("origin", false),
        _ => (remote, true),
    })
}

/// Fetches pull request `number`'s head into the repository at `cwd`,
/// from where [`pull_remote`] says. Blocking.
pub(super) fn fetch_pull(
    cwd: &Path,
    number: u64,
    head: &str,
    head_owner: Option<&str>,
) -> Result<Fetched, ApiError> {
    let checkout = checkouts::read(cwd).ok_or(Error::NoRemote)?;
    let (remote, fork) = pull_remote(&checkout, head_owner).ok_or(Error::NoRemote)?;
    let (fetched, refspec) = pull_refs(remote, number, head, fork).ok_or(ApiError::Invalid(
        "that pull request's branch is not one this computer checks out",
    ))?;
    let deadline = Instant::now() + FETCH_WAIT;
    worktree::fetch(cwd, remote, &refspec, || Instant::now() >= deadline).map_err(Error::Fetch)?;
    Ok(fetched)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
