//! `sheprd-t3`'s arguments, parsed without touching a server.

use std::{ffi::OsString, path::PathBuf};

use sheprd_t3::launch::{InteractionMode, NewWorktree, Request};

pub const HELP: &str = "\
sheprd-t3: read and steer T3 Code threads as Sheprd chats

Usage:
  sheprd-t3 pair --token-file FILE [--server URL]
  sheprd-t3 threads --token-file FILE [--server URL] [--json]
  sheprd-t3 show --token-file FILE [--server URL] [--json] THREAD
  sheprd-t3 send --token-file FILE [--server URL] [--json] THREAD [--] TEXT
  sheprd-t3 stop --token-file FILE [--server URL] [--json] THREAD
  sheprd-t3 new --token-file FILE [--server URL] [--json] [--cwd PATH]
                [--prompt TEXT] [--title TEXT] [--provider ID] [--model SLUG]
                [--mode MODE]
                [--plan] [--base REF [--branch NAME]]

Commands:
  pair      Sign in with a one-time code from `t3 auth pairing create` on
            the server's machine, read from stdin, and keep the token in
            FILE (mode 0600), usable only with that server
  threads   List the server's threads
  show      Print one thread as a chat transcript
  send      Send TEXT to the thread's agent: now, or after the turn under
            way when one runs
  stop      Stop the thread's turn under way
  new       Start a thread in the checkout at PATH (default: here), in the
            T3 project for its repository, created if there is none; prints
            the thread's id. In a linked worktree, the thread works there.

Options:
  --server URL       The server's address; by default the T3 Code server
                     running on this computer ($T3CODE_HOME or ~/.t3)
  --token-file FILE  Where the access token is kept
  --json             Print JSON
  --cwd PATH         The checkout a new thread works in
  --prompt TEXT      The new thread's first message
  --title TEXT       Its title; by default T3 names it from the prompt
  --provider ID      The agent to run, by T3 provider instance; by default
                     the project's, else the first ready one
  --model SLUG       Its model; by default the agent's default
  --mode MODE        approval-required, auto-accept-edits, auto, or
                     full-access (T3's default)
  --plan             Plan before acting
  --base REF         Work in a new worktree T3 makes from REF
  --branch NAME      The new worktree's branch
  -h, --help         Show this help
";

#[derive(Debug, Eq, PartialEq)]
pub enum Command {
    Help,
    Pair,
    Threads,
    Show { thread: String },
    Send { text: String, thread: String },
    Stop { thread: String },
    New(Request),
}

#[derive(Debug, Eq, PartialEq)]
pub struct Options {
    pub command: Command,
    pub json: bool,
    pub server: Option<String>,
    pub token_file: Option<PathBuf>,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("{0} needs a value")]
    Missing(&'static str),
    #[error("unknown option {0}")]
    Unknown(String),
    #[error("arguments must be valid UTF-8, except file names")]
    Utf8,
    #[error("no command; see --help")]
    NoCommand,
    #[error("{0} takes one argument")]
    Arguments(&'static str),
    #[error("send takes a thread and the text to send")]
    SendArguments,
    #[error("--token-file is required")]
    TokenFile,
    #[error("{0} is only for `new`")]
    NewOnly(&'static str),
    #[error("--branch needs --base")]
    BranchWithoutBase,
    #[error(transparent)]
    Mode(#[from] sheprd_t3::launch::UnknownMode),
}

/// `text` from the server, safe for a terminal: control characters, which
/// could move the cursor or start an escape sequence, become `\u{fffd}`.
pub fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

/// Parses the arguments after the program's name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, Error> {
    let mut args = args.into_iter();
    let mut command = None;
    let mut positional = Vec::new();
    let mut json = false;
    let mut server = None;
    let mut token_file = None;
    let mut new = Request::default();
    let mut base = None;
    let mut branch = None;
    // The first option only `new` takes, to refuse it elsewhere.
    let mut new_only = None;
    let value = |name: &'static str,
                 new_only: &mut Option<&'static str>,
                 args: &mut dyn Iterator<Item = OsString>| {
        new_only.get_or_insert(name);
        args.next()
            .ok_or(Error::Missing(name))?
            .into_string()
            .map_err(|_| Error::Utf8)
    };
    while let Some(arg) = args.next() {
        match arg.to_str() {
            // What follows is text, such as a prompt starting with `-`.
            Some("--") => {
                for rest in args.by_ref() {
                    positional.push(rest.into_string().map_err(|_| Error::Utf8)?);
                }
            }
            Some("-h" | "--help") => {
                return Ok(Options {
                    command: Command::Help,
                    json,
                    server,
                    token_file,
                });
            }
            Some("--json") => json = true,
            Some("--server") => {
                let value = args.next().ok_or(Error::Missing("--server"))?;
                server = Some(value.into_string().map_err(|_| Error::Utf8)?);
            }
            Some("--cwd") => {
                new_only.get_or_insert("--cwd");
                new.cwd = PathBuf::from(args.next().ok_or(Error::Missing("--cwd"))?);
            }
            Some("--prompt") => new.prompt = Some(value("--prompt", &mut new_only, &mut args)?),
            Some("--provider") => {
                new.provider = Some(value("--provider", &mut new_only, &mut args)?)
            }
            Some("--model") => new.model = Some(value("--model", &mut new_only, &mut args)?),
            Some("--title") => new.title = Some(value("--title", &mut new_only, &mut args)?),
            Some("--mode") => {
                new.runtime_mode = value("--mode", &mut new_only, &mut args)?.parse()?
            }
            Some("--plan") => {
                new_only.get_or_insert("--plan");
                new.interaction_mode = InteractionMode::Plan;
            }
            Some("--base") => base = Some(value("--base", &mut new_only, &mut args)?),
            Some("--branch") => branch = Some(value("--branch", &mut new_only, &mut args)?),
            Some("--token-file") => {
                token_file = Some(PathBuf::from(
                    args.next().ok_or(Error::Missing("--token-file"))?,
                ));
            }
            Some(option) if option.starts_with('-') => {
                return Err(Error::Unknown(option.to_owned()));
            }
            Some(word) if command.is_none() => command = Some(word.to_owned()),
            Some(word) => positional.push(word.to_owned()),
            None => return Err(Error::Utf8),
        }
    }
    let one = |name: &'static str, mut values: Vec<String>| {
        (values.len() == 1)
            .then(|| values.remove(0))
            .ok_or(Error::Arguments(name))
    };
    let command = match command.as_deref() {
        None => return Err(Error::NoCommand),
        // The code comes on stdin: arguments are visible to other users.
        Some("pair") if positional.is_empty() => Command::Pair,
        Some("pair") => return Err(Error::Arguments("pair")),
        Some("threads") if positional.is_empty() => Command::Threads,
        Some("threads") => return Err(Error::Arguments("threads")),
        Some("show") => Command::Show {
            thread: one("show", positional)?,
        },
        Some("send") => match <[String; 2]>::try_from(positional) {
            Ok([thread, text]) => Command::Send { text, thread },
            Err(_) => return Err(Error::SendArguments),
        },
        Some("stop") => Command::Stop {
            thread: one("stop", positional)?,
        },
        Some("new") if positional.is_empty() => {
            new.worktree = match (base, branch) {
                (Some(base), branch) => Some(NewWorktree { base, branch }),
                (None, Some(_)) => return Err(Error::BranchWithoutBase),
                (None, None) => None,
            };
            if new.cwd.as_os_str().is_empty() {
                new.cwd = PathBuf::from(".");
            }
            Command::New(new)
        }
        Some("new") => return Err(Error::Arguments("new")),
        Some(other) => return Err(Error::Unknown(other.to_owned())),
    };
    if let Some(option) = new_only
        && !matches!(command, Command::New(_))
    {
        return Err(Error::NewOnly(option));
    }
    if token_file.is_none() {
        return Err(Error::TokenFile);
    }
    Ok(Options {
        command,
        json,
        server,
        token_file,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
