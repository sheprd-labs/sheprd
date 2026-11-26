//! `sheprd-acp thread-mcp --thread ID`: the stdio MCP server a chat gives
//! its agent, reading that chat's own thread's transcript file and nothing
//! else (`docs/threads-plan.md`, `thread_read`). It sends no command and
//! changes nothing; it ends when its client, the agent, closes it.

use std::{ffi::OsString, io, path::PathBuf};

use serde_json::{Value, json};
use sheprd_chat_protocol::{
    EntryId,
    thread::{self, ThreadId, file::Contents, read},
};
use sheprd_mcp::{Annotations, Call, Handler, ServerInfo, Tool, ToolResult, UnknownTool};

/// The name agents know the server by.
pub(crate) const NAME: &str = "sheprd-thread";
/// The subcommand that serves it.
pub(crate) const COMMAND: &str = "thread-mcp";

const READ: &str = "thread_read";
const SEARCH: &str = "thread_search";

/// The arguments that start the server for thread `id`, after the program.
pub(crate) fn args(id: ThreadId) -> Vec<String> {
    vec![COMMAND.to_owned(), "--thread".to_owned(), id.to_string()]
}

/// How an agent gets the server for its chat's thread.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Given {
    /// It does not: its harness takes no MCP servers, or the chat keeps no
    /// thread.
    #[default]
    Not,
    /// On its own command line, as a natively reached agent takes it.
    Launched,
    /// In `session/new` and `session/load`, as every ACP agent takes it.
    InSession(Server),
}

impl Given {
    /// Whether the agent can read its thread.
    pub(crate) fn reads(&self) -> bool {
        !matches!(self, Self::Not)
    }
}

/// The program and arguments that serve one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Server {
    pub(crate) args: Vec<String>,
    /// UTF-8, as JSON and TOML carry it.
    pub(crate) program: String,
}

impl Server {
    /// The server for thread `id`, when this program's own path is known
    /// and is UTF-8.
    pub(crate) fn of(id: ThreadId) -> Option<Self> {
        let program = std::env::current_exe()
            .ok()?
            .into_os_string()
            .into_string()
            .ok()?;
        Some(Self {
            args: args(id),
            program,
        })
    }

    /// `claude --mcp-config`'s value: the server as JSON.
    pub(crate) fn claude_config(&self) -> String {
        json!({"mcpServers": {NAME: {
            "type": "stdio",
            "command": self.program,
            "args": self.args,
        }}})
        .to_string()
    }

    /// `codex -c` overrides that add the server. JSON strings and arrays
    /// of strings are TOML values as written.
    pub(crate) fn codex_overrides(&self) -> [String; 4] {
        let program = Value::from(self.program.clone());
        let args = Value::from(self.args.clone());
        [
            "-c".to_owned(),
            format!("mcp_servers.{NAME}.command={program}"),
            "-c".to_owned(),
            format!("mcp_servers.{NAME}.args={args}"),
        ]
    }
}

/// Serves thread `id`'s file on standard input and output.
pub(crate) fn run(id: ThreadId) -> anyhow::Result<()> {
    let dir = thread::threads_dir().ok_or_else(|| anyhow::anyhow!("no home folder"))?;
    let handler = Reader {
        path: thread::transcript_path(&dir, &id),
    };
    let info = ServerInfo {
        name: NAME.to_owned(),
        title: Some("Sheprd thread".to_owned()),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    sheprd_mcp::serve(info, io::BufReader::new(io::stdin()), io::stdout(), handler)?;
    Ok(())
}

/// Reads one thread's transcript file.
pub(crate) struct Reader {
    pub(crate) path: PathBuf,
}

impl Handler for Reader {
    fn instructions(&mut self) -> Option<String> {
        Some(
            "These tools read this chat's own conversation, as Sheprd keeps it across the \
             agents that held it. When Sheprd hands you the conversation and leaves entries \
             out for length, it names them by id: read them with thread_read when you need \
             them, or find ids with thread_search."
                .to_owned(),
        )
    }

    fn tools(&mut self) -> Vec<Tool> {
        let read_only = Some(Annotations {
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: true,
            open_world_hint: false,
        });
        vec![
            Tool {
                name: READ.to_owned(),
                title: Some("Read the conversation".to_owned()),
                description: format!(
                    "Entries of this chat's conversation from id `from` through id `to`, oldest \
                     first, at most {} entries or {} KiB per call; read on from the id it names \
                     when there is more. Trimmed entries are no longer kept.",
                    read::MAX_ENTRIES,
                    read::MAX_BYTES / 1024
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "from": {"type": "integer", "minimum": 0},
                        "to": {"type": "integer", "minimum": 0},
                    },
                    "required": ["from", "to"],
                }),
                annotations: read_only,
            },
            Tool {
                name: SEARCH.to_owned(),
                title: Some("Search the conversation".to_owned()),
                description: format!(
                    "Ids of this chat's entries whose text holds `query`, ignoring case, newest \
                     first, at most {}, each with the line that holds it.",
                    read::MAX_HITS
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {"query": {"type": "string", "minLength": 1}},
                    "required": ["query"],
                }),
                annotations: read_only,
            },
        ]
    }

    fn call(&mut self, call: Call) -> Result<ToolResult, UnknownTool> {
        match call.name.as_str() {
            READ => Ok(self.read(&call)),
            SEARCH => Ok(self.search(&call)),
            _ => Err(UnknownTool),
        }
    }
}

impl Reader {
    fn contents(&self) -> Result<Contents, ToolResult> {
        Contents::read(&self.path)
            .map_err(|error| ToolResult::error(format!("The conversation cannot be read: {error}")))
    }

    fn read(&self, call: &Call) -> ToolResult {
        let id = |name: &str| {
            call.arguments
                .get(name)
                .and_then(Value::as_u64)
                .map(EntryId)
        };
        let (Some(from), Some(to)) = (id("from"), id("to")) else {
            return ToolResult::error("Give `from` and `to` as entry ids, such as 12 and 30.");
        };
        if from > to {
            return ToolResult::error("`from` comes after `to`.");
        }
        match self.contents() {
            Ok(contents) => ToolResult::text(read::read(&contents, from, to).text),
            Err(error) => error,
        }
    }

    fn search(&self, call: &Call) -> ToolResult {
        let Some(query) = call
            .arguments
            .get("query")
            .and_then(Value::as_str)
            .filter(|query| !query.trim().is_empty())
        else {
            return ToolResult::error("Give `query`, the text to look for.");
        };
        let contents = match self.contents() {
            Ok(contents) => contents,
            Err(error) => return error,
        };
        let hits = read::search(&contents, query);
        if hits.is_empty() {
            return ToolResult::text("No entry holds that.");
        }
        let mut text = String::new();
        for (id, line) in hits {
            text.push_str(&format!("#{} {line}\n", id.0));
        }
        ToolResult::text(text)
    }
}

/// `thread-mcp`'s arguments: `--thread ID`.
pub(crate) fn parse(
    mut args: impl Iterator<Item = OsString>,
) -> Result<ThreadId, crate::cli::Error> {
    let mut thread = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--thread") => {
                let id = args
                    .next()
                    .ok_or(crate::cli::Error::MissingValue("--thread"))?;
                let id = id
                    .into_string()
                    .map_err(|_| crate::cli::Error::NotUtf8("--thread"))?;
                thread = Some(
                    id.parse()
                        .map_err(|_| crate::cli::Error::InvalidThread(id))?,
                );
            }
            _ => return Err(crate::cli::Error::UnknownOption(arg)),
        }
    }
    thread.ok_or(crate::cli::Error::MissingValue("--thread"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
