//! Worktrees for work on a core. herdr-core keeps no workspaces or
//! worktrees, so whoever starts work there makes the checkout itself, in the
//! folder Herdr 1 would have used, and starts its terminal in it: the phone
//! daemon, automations, and dispatch.

mod error;

pub use error::Error;

use crate::{local_path::home_dir, new_command};
use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::Duration,
};

/// The most of Git's complaint that is kept for messages and logs.
const MAX_COMPLAINT_CHARS: usize = 300;
/// The folder under home that Herdr's default keeps checkouts in, on this
/// computer and on any host a script makes one on.
pub const ROOT: &str = ".herdr/worktrees";
/// The folder of a branch with no ASCII letters or digits.
const FALLBACK_SLUG: &str = "worktree";
/// The most of Git's output read; its answers here are a line or two.
const MAX_OUTPUT: u64 = 64 * 1024;
/// How often a running Git is asked whether its caller has given up.
const POLL: Duration = Duration::from_millis(50);

/// Where new checkouts go, when this user has a home.
pub fn root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(ROOT))
}

/// Adds a new `branch` from `base` (a commit, branch, or `HEAD`) of
/// `repository`, under `root`, and answers the checkout's folder. Git is
/// killed, and the add fails with `Error::GaveUp`, once `give_up` says so,
/// such as for a stopped run or a hook that hangs.
pub fn add(
    repository: &Path,
    branch: &str,
    base: &str,
    root: &Path,
    give_up: impl Fn() -> bool,
) -> Result<PathBuf, Error> {
    add_checkout(repository, branch, Some(base), root, &give_up)
}

/// A checkout [`add_or_checkout`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Added {
    pub checkout: PathBuf,
    /// Whether the branch is new, and so [`remove`] may delete it again.
    pub new_branch: bool,
}

/// As [`add`], but a `branch` that already exists locally is checked out as
/// it is and `base` is ignored, as Herdr 1's `worktree.create` does.
pub fn add_or_checkout(
    repository: &Path,
    branch: &str,
    base: &str,
    root: &Path,
    give_up: impl Fn() -> bool,
) -> Result<Added, Error> {
    let existing = format!("refs/heads/{branch}");
    // `show-ref` fails for a missing ref; any other failure shows again
    // in the add, with Git's own words.
    let exists = git(
        &give_up,
        repository,
        [
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new(&existing),
        ],
    )
    .is_ok();
    let base = (!exists).then_some(base);
    let checkout = add_checkout(repository, branch, base, root, &give_up)?;
    Ok(Added {
        checkout,
        new_branch: !exists,
    })
}

/// A new checkout of `branch` under `root`: a new branch from `base`, or
/// the existing branch when `base` is `None`.
fn add_checkout(
    repository: &Path,
    branch: &str,
    base: Option<&str>,
    root: &Path,
    give_up: &dyn Fn() -> bool,
) -> Result<PathBuf, Error> {
    let common = git(
        give_up,
        repository,
        [
            OsStr::new("rev-parse"),
            OsStr::new("--path-format=absolute"),
            OsStr::new("--git-common-dir"),
        ],
    )?;
    let checkout = root.join(folder(Path::new(common.trim()), branch)?);
    let mut args = vec![OsStr::new("worktree"), OsStr::new("add")];
    match base {
        Some(base) => args.extend([
            OsStr::new("-b"),
            OsStr::new(branch),
            checkout.as_os_str(),
            // A base that looks like an option is a ref, never a flag.
            OsStr::new("--end-of-options"),
            OsStr::new(base),
        ]),
        None => args.extend([
            checkout.as_os_str(),
            OsStr::new("--end-of-options"),
            OsStr::new(branch),
        ]),
    }
    git(give_up, repository, args)?;
    Ok(checkout)
}

/// Fetches `refspec` from `remote` into `repository`, never asking for
/// credentials, until `give_up` says to stop.
pub fn fetch(
    repository: &Path,
    remote: &str,
    refspec: &str,
    give_up: impl Fn() -> bool,
) -> Result<(), Error> {
    let mut command = new_command("git");
    command
        .current_dir(repository)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "-c",
            "core.fsmonitor=false",
            "fetch",
            "--no-tags",
            "--quiet",
        ])
        .args(["--end-of-options", remote, refspec]);
    match run(&give_up, command) {
        Err(Error::Refused(complaint)) => Err(Error::Fetch(complaint)),
        other => other.map(drop),
    }
}

/// Undoes [`add`] for a checkout nothing started in, so asking again finds
/// the branch free. Only for what `add` just made: the checkout and branch
/// are removed whatever they hold.
pub fn remove(repository: &Path, checkout: &Path, branch: &str) -> Result<(), Error> {
    remove_checkout(repository, checkout)?;
    git(
        &|| false,
        repository,
        [OsStr::new("branch"), OsStr::new("-D"), OsStr::new(branch)],
    )?;
    Ok(())
}

/// Undoes [`add_or_checkout`] for a checkout nothing started in: the
/// checkout goes whatever it holds, and its branch too only when the add
/// made it.
pub fn remove_added(repository: &Path, added: &Added, branch: &str) -> Result<(), Error> {
    if added.new_branch {
        remove(repository, &added.checkout, branch)
    } else {
        remove_checkout(repository, &added.checkout)
    }
}

fn remove_checkout(repository: &Path, checkout: &Path) -> Result<(), Error> {
    git(
        &|| false,
        repository,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--force"),
            checkout.as_os_str(),
        ],
    )?;
    Ok(())
}

/// Where under the worktree root a new checkout of `branch` goes, as Herdr
/// files it: the repository's folder name, from its common Git directory
/// `common`, then the branch's slug. For callers that make the checkout on
/// another host, where the root is that host's own.
pub fn folder(common: &Path, branch: &str) -> Result<PathBuf, Error> {
    let name = repository_name(common).ok_or(Error::NoRepositoryName)?;
    Ok(Path::new(name).join(path_slug(branch)))
}

/// The repository's folder name, from its common Git directory: the folder
/// holding `.git`, or a bare repository's own name without `.git`.
fn repository_name(common: &Path) -> Option<&OsStr> {
    let name = common.file_name()?;
    if name == ".git" {
        return common.parent()?.file_name();
    }
    Some(common.file_stem().unwrap_or(name))
}

/// Herdr's folder name for `branch`: lowercase ASCII letters and digits,
/// with every other run of characters one dash and none at either end.
fn path_slug(branch: &str) -> String {
    let lower = branch.to_ascii_lowercase();
    let words = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty());
    let slug = words.collect::<Vec<_>>().join("-");
    if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug
    }
}

fn git<'a>(
    give_up: &dyn Fn() -> bool,
    repository: &Path,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> Result<String, Error> {
    let mut command = new_command("git");
    command.current_dir(repository).args(args);
    run(give_up, command)
}

/// Runs Git as `command` says, answering what it wrote.
fn run(give_up: &dyn Fn() -> bool, mut command: std::process::Command) -> Result<String, Error> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Start)?;
    // Read both pipes while Git runs, so a full one never blocks it.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = loop {
        if let Some(status) = child.try_wait().map_err(Error::Start)? {
            break status;
        }
        if give_up() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::GaveUp);
        }
        thread::sleep(POLL);
    };
    let (stdout, stderr) = (joined(stdout), joined(stderr));
    if !status.success() {
        let complaint = String::from_utf8_lossy(&stderr);
        return Err(Error::Refused(
            complaint.trim().chars().take(MAX_COMPLAINT_CHARS).collect(),
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// Reads `pipe` to its end, so a chatty hook never meets a closed pipe,
/// keeping only the first [`MAX_OUTPUT`] bytes.
fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || pipe.map(bounded).unwrap_or_default())
}

/// Everything `reader` gives until its end, of which the first
/// [`MAX_OUTPUT`] bytes are kept.
fn bounded(mut reader: impl Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => return kept,
            Ok(read) => {
                let room = usize::try_from(MAX_OUTPUT)
                    .unwrap_or(usize::MAX)
                    .saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..read.min(room)]);
            }
        }
    }
}

fn joined(reader: thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    reader.join().unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
