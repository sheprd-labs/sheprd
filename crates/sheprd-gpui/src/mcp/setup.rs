//! `sheprd mcp config`: what to paste into an MCP client so it starts this
//! executable's `sheprd mcp`. The path is absolute, since a macOS app
//! bundle puts nothing on `PATH`.

use crate::cli::control::McpClient;
use crate::control::Output;
use serde_json::json;
use std::path::Path;

pub(super) fn config(client: Option<McpClient>) -> crate::Result<Output> {
    let executable = crate::agent_skill::executable().ok_or(super::Error::Executable)?;
    let clients = match client {
        Some(client) => vec![client],
        None => McpClient::ALL.to_vec(),
    };
    let snippets: Vec<_> = clients
        .iter()
        .map(|&client| (client, snippet(client, &executable)))
        .collect();
    let text = snippets
        .iter()
        .map(|(client, snippet)| format!("{}:\n{snippet}", client.label()))
        .collect::<Vec<_>>()
        .join("\n\n");
    let data = json!({
        "command": executable.to_string_lossy(),
        "args": ["mcp"],
        "clients": snippets
            .iter()
            .map(|(client, snippet)| (client.key().to_owned(), json!(snippet)))
            .collect::<serde_json::Map<_, _>>(),
    });
    let note = "Turn on MCP server in Sheprd's Settings > General first.";
    Output::data(format!("{text}\n\n{note}"), &data)
}

/// The shell a printed command is pasted into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shell {
    /// sh, bash, zsh, and the like.
    Posix,
    /// cmd.exe or PowerShell.
    Windows,
}

impl Shell {
    /// The shell people use on this platform.
    pub(super) const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Posix
    };

    /// `text` as one word of this shell.
    fn quote(self, text: &str) -> String {
        match self {
            Self::Posix => posix_quote(text),
            // Both cmd.exe and PowerShell read a double-quoted path as one
            // word, and no Windows file name can hold a `"`.
            Self::Windows => format!("\"{text}\""),
        }
    }
}

/// The configuration `client` reads, starting `executable mcp`, with any
/// command quoted for this platform's shell.
pub(super) fn snippet(client: McpClient, executable: &Path) -> String {
    snippet_for(client, executable, Shell::CURRENT)
}

pub(super) fn snippet_for(client: McpClient, executable: &Path, shell: Shell) -> String {
    let command = executable.to_string_lossy();
    match client {
        McpClient::ClaudeCode => format!("claude mcp add sheprd -- {} mcp", shell.quote(&command)),
        McpClient::ClaudeDesktop | McpClient::Cursor => {
            let config =
                json!({ "mcpServers": { "sheprd": { "command": command, "args": ["mcp"] } } });
            serde_json::to_string_pretty(&config).unwrap_or_default()
        }
        // A TOML basic string takes JSON's escapes for what a path holds.
        McpClient::Codex => format!(
            "[mcp_servers.sheprd]\ncommand = {}\nargs = [\"mcp\"]",
            json!(command)
        ),
    }
}

/// `text` as one POSIX shell word.
fn posix_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+:,@".contains(&byte))
    {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}
