//! Edits to the labels shown beside agents on paired phones.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// A fully qualified pane id from the host's layout.
    Pane(String),
    /// A caller's Herdr 1 pane, scoped to its daemon socket.
    LocalPane {
        pane: String,
        socket: String,
    },
    Chat(String),
    Workspace(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edit {
    List,
    Add(String),
    Rename { old: String, new: String },
    Remove(String),
}

/// Both the desktop CLI and the managed SSH shim accept this syntax.
pub const USAGE: &str =
    "labels [list|add TAG|rename OLD NEW|remove TAG] [--worktree] [--pane ID | --workspace ID]";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    pub target: Option<Target>,
    pub worktree: bool,
    pub edit: Edit,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[error("Use {USAGE}")]
pub struct InvalidArguments;

impl Command {
    pub fn parse(
        args: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> Result<Self, InvalidArguments> {
        let mut args = args.into_iter();
        let mut words = Vec::new();
        let mut target = None;
        let mut worktree = false;
        while let Some(word) = args.next() {
            let word = word.into_string().map_err(|_| InvalidArguments)?;
            match word.as_str() {
                "--worktree" => worktree = true,
                "--pane" | "--workspace" if target.is_none() => {
                    let id = args
                        .next()
                        .and_then(|id| id.into_string().ok())
                        .filter(|id| !id.is_empty() && !id.starts_with('-'))
                        .ok_or(InvalidArguments)?;
                    target = Some(if word == "--workspace" {
                        worktree = true;
                        Target::Workspace(id)
                    } else {
                        Target::Pane(id)
                    });
                }
                _ if !word.starts_with('-') && words.len() < 3 => words.push(word),
                _ => return Err(InvalidArguments),
            }
        }
        let edit = match words.as_slice() {
            [] => Edit::List,
            [verb] if verb == "list" => Edit::List,
            [verb, tag] if verb == "add" => Edit::Add(tag.clone()),
            [verb, tag] if verb == "remove" || verb == "delete" => Edit::Remove(tag.clone()),
            [verb, old, new] if verb == "rename" => Edit::Rename {
                old: old.clone(),
                new: new.clone(),
            },
            _ => return Err(InvalidArguments),
        };
        Ok(Self {
            target,
            worktree,
            edit,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Run labels inside an agent, or specify --pane ID or --workspace ID")]
    NoTarget,
    #[error(transparent)]
    Socket(#[from] sheprd_client::Error),
    #[error(transparent)]
    Core(#[from] sheprd_core_client::Error),
    #[error("The Herdr daemon socket path must be UTF-8")]
    SocketPath,
    #[error(transparent)]
    Control(#[from] super::Error),
    #[error("Unexpected daemon answer to a labels request")]
    Unexpected,
}

impl Command {
    pub fn run(self, dir: &std::path::Path) -> Result<Vec<String>, Error> {
        let target = match self.target {
            Some(target) => target,
            None => caller()?,
        };
        let request = super::Request::Labels {
            target,
            worktree: self.worktree,
            edit: self.edit,
        };
        match super::request(&super::address(dir), &request)? {
            super::Response::Labels { tags } => Ok(tags),
            _ => Err(Error::Unexpected),
        }
    }
}

fn caller() -> Result<Target, Error> {
    let id = |key| std::env::var(key).ok().filter(|value| !value.is_empty());
    if let Some(endpoint) = id(sheprd_chat_protocol::AGENT_CHAT_ENV) {
        return Ok(Target::Chat(endpoint));
    }
    if let Some(address) = id("HC_TERMINAL_ID") {
        let socket = std::env::var_os("HC_TERMINAL_SOCKET").filter(|s| !s.is_empty());
        let endpoint = sheprd_core_client::Endpoint::resolve(|name| match name {
            "HC_SOCKET" => socket.clone().or_else(|| std::env::var_os(name)),
            _ => std::env::var_os(name),
        })?;
        let core = sheprd_core_client::Core::connect(&endpoint)?;
        return Ok(Target::Pane(super::favorites::core_pane(
            &core.hello().core.id,
            &address,
        )));
    }
    let pane = id("HERDR_PANE_ID").ok_or(Error::NoTarget)?;
    let socket = sheprd_client::ConnectTarget::Local
        .socket_path()?
        .into_os_string()
        .into_string()
        .map_err(|_| Error::SocketPath)?;
    Ok(Target::LocalPane { pane, socket })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_parse_scope_and_require_exact_operands() {
        let parse = |args: &[&str]| Command::parse(args.iter().map(std::ffi::OsString::from));
        assert_eq!(
            parse(&["rename", "old", "new", "--workspace", "w1"]),
            Ok(Command {
                target: Some(Target::Workspace("w1".into())),
                worktree: true,
                edit: Edit::Rename {
                    old: "old".into(),
                    new: "new".into()
                },
            })
        );
        assert!(parse(&["add"]).is_err());
        assert!(parse(&["--workspace", "w1", "--pane", "p1"]).is_err());
        assert!(parse(&["list", "extra"]).is_err());
    }
}
