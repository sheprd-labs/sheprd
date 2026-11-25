//! How `herdr-acp` finds the command that starts an agent; the agents
//! themselves are `sheprd_chat_protocol::Agent`, shared with its clients.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

pub(crate) use sheprd_chat_protocol::Agent;

/// Where agents install themselves besides `PATH`, relative to the home
/// folder, searched after it: a noninteractive SSH shell, an app started from
/// the desktop, or a user service such as the remote control daemon often
/// runs with a `PATH` that leaves them out. The mise and asdf shim folders
/// hold agents installed through those version managers (`mise use -g
/// opencode`); a shim finds its tool without the user's shell setup.
const USER_DIRS: [&str; 8] = [
    ".local/bin",
    ".cargo/bin",
    ".bun/bin",
    ".npm-global/bin",
    ".opencode/bin",
    ".volta/bin",
    ".local/share/mise/shims",
    ".asdf/shims",
];
/// System folders package managers install into, searched last.
#[cfg(unix)]
const SYSTEM_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
#[cfg(not(unix))]
const SYSTEM_DIRS: [&str; 0] = [];
/// The runner that fetches a package on first use; an agent only it can
/// start is not installed.
const FETCHER: &str = "npx";

/// `path`'s folders, then the user and system folders agents install to.
pub(crate) fn search_path(path: Option<&OsStr>, home: Option<&Path>) -> OsString {
    search_path_with(path, home, local_app_data().as_deref())
}

fn search_path_with(
    path: Option<&OsStr>,
    home: Option<&Path>,
    #[cfg_attr(not(windows), allow(unused_variables))] local_app_data: Option<&Path>,
) -> OsString {
    let mut dirs: Vec<PathBuf> = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    dirs.extend(
        home.into_iter()
            .flat_map(|home| USER_DIRS.iter().map(|dir| home.join(dir))),
    );
    #[cfg(windows)]
    {
        dirs.extend(antigravity_preferred_dirs(home, local_app_data));
        if let Some(local) = local_app_data {
            dirs.push(local.join("Programs").join("muse"));
        }
    }
    dirs.extend(SYSTEM_DIRS.iter().map(PathBuf::from));
    std::env::join_paths(dirs).unwrap_or_default()
}

/// How an agent is reached.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Launch {
    /// The agent's own program, through a translator of its protocol
    /// ([`crate::claude`], [`crate::codex`], [`crate::pi`]).
    Native(PathBuf),
    /// An ACP server: the program and its arguments.
    Acp(OsString, Vec<OsString>),
}

/// How to start `agent`: the agent itself where `herdr-acp` speaks its own
/// protocol and it is on `path`, unless `adapter` asks for ACP; otherwise
/// [`command`].
pub(crate) fn launch(agent: Agent, path: Option<&OsStr>, adapter: bool) -> Launch {
    if !adapter
        && let Some(program) = native(agent)
        && let Some(found) = find_program(program, path)
    {
        return Launch::Native(found);
    }
    let (program, args) = command(agent, path);
    Launch::Acp(program, args)
}

/// The agent's own program, for an agent `herdr-acp` can talk to without an
/// ACP adapter.
fn native(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::Claude => Some(crate::claude::PROGRAM),
        Agent::Codex => Some(crate::codex::PROGRAM),
        Agent::Pi => Some(crate::pi::PROGRAM),
        Agent::Antigravity
        | Agent::Copilot
        | Agent::Gemini
        | Agent::Kimi
        | Agent::Muse
        | Agent::Opencode => None,
    }
}

/// The agents whose own program or ACP command is on `path`, in table
/// order. One only `npx` could start does not count: it would be
/// downloaded, not run.
pub(crate) fn installed(path: Option<&OsStr>) -> Vec<Agent> {
    let home = std::env::home_dir();
    Agent::ALL
        .iter()
        .copied()
        .filter(|agent| {
            if *agent == Agent::Antigravity {
                return antigravity_program(path, home.as_deref()).is_some();
            }
            native(*agent).is_some_and(|program| find_program(program, path).is_some())
                || agent
                    .candidates()
                    .iter()
                    .filter(|(program, _)| *program != FETCHER)
                    .any(|(program, _)| find_program(program, path).is_some())
        })
        .collect()
}

/// The first of `agent`'s commands found on `path`, or the last one by name
/// so that starting it fails with a message naming what to install.
pub(crate) fn command(agent: Agent, path: Option<&OsStr>) -> (OsString, Vec<OsString>) {
    let candidates = agent.candidates();
    let found = if agent == Agent::Antigravity {
        antigravity_program(path, std::env::home_dir().as_deref())
            .map(|found| (found, candidates[0].1))
    } else {
        candidates
            .iter()
            .find_map(|(program, args)| find_program(program, path).map(|found| (found, *args)))
    };
    let (program, args) = match found {
        Some((found, args)) => (found.into_os_string(), args),
        None => {
            let (program, args) = candidates[candidates.len() - 1];
            (OsString::from(program), args)
        }
    };
    (program, args.iter().map(OsString::from).collect())
}

/// Looks `program` up in a `PATH`-style list of directories.
pub(crate) fn find_program(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let path = path?;
    std::env::split_paths(path).find_map(|dir| executable(&dir, program))
}

/// Standalone Antigravity CLI install locations, preferred over an IDE
/// launcher with the same `agy` name on `PATH`.
fn antigravity_preferred_dirs(
    home: Option<&Path>,
    #[cfg_attr(not(windows), allow(unused_variables))] local_app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut preferred = Vec::new();
    #[cfg(windows)]
    if let Some(local) = local_app_data {
        preferred.push(local.join("agy").join("bin"));
    } else if let Some(home) = home {
        preferred.push(home.join("AppData").join("Local").join("agy").join("bin"));
    }
    #[cfg(not(windows))]
    preferred.extend(home.into_iter().map(|home| home.join(".local/bin")));
    #[cfg(target_os = "macos")]
    preferred.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    preferred
}

#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn local_app_data() -> Option<PathBuf> {
    None
}

/// Prefer the standalone Antigravity CLI's documented install locations over
/// an IDE launcher with the same `agy` name on the shell's `PATH`.
///
/// Only reorders directories already present in `path`: [`search_path`] adds
/// every platform's standalone install directory for production lookups, while
/// callers that pass an explicit isolated `path` stay scoped to it.
fn antigravity_program(path: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    antigravity_program_with(path, home, local_app_data().as_deref())
}

fn antigravity_program_with(
    path: Option<&OsStr>,
    home: Option<&Path>,
    local_app_data: Option<&Path>,
) -> Option<PathBuf> {
    let directories: Vec<PathBuf> = std::env::split_paths(path?).collect();
    let preferred = antigravity_preferred_dirs(home, local_app_data);
    let ordered: Vec<PathBuf> = preferred
        .iter()
        .filter(|directory| directories.contains(directory))
        .cloned()
        .chain(
            directories
                .iter()
                .filter(|directory| !preferred.contains(directory))
                .cloned(),
        )
        .collect();
    ordered
        .iter()
        .find_map(|directory| executable(directory, "agy"))
}

#[cfg(unix)]
fn executable(dir: &Path, program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let candidate = dir.join(program);
    let metadata = candidate.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(candidate)
}

#[cfg(windows)]
fn executable(dir: &Path, program: &str) -> Option<PathBuf> {
    ["exe", "cmd", "bat"]
        .iter()
        .map(|extension| dir.join(program).with_extension(extension))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests;
