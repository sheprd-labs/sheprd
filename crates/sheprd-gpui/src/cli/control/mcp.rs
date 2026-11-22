//! `sheprd mcp`: serve MCP on stdio, or print a client's configuration.

use super::{Args, CliError, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpCommand {
    /// Speak MCP on standard input and output, for the client that started it.
    Serve,
    /// What to paste into `client`'s settings, or every client's.
    Config { client: Option<McpClient> },
}

/// MCP clients whose configuration `sheprd mcp config` prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpClient {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
    Cursor,
}

impl McpClient {
    pub const ALL: &[Self] = &[
        Self::ClaudeCode,
        Self::ClaudeDesktop,
        Self::Codex,
        Self::Cursor,
    ];

    /// The `--client` value and JSON key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }

    /// Where the snippet goes, for a person.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code (run in a terminal)",
            Self::ClaudeDesktop => "Claude Desktop (claude_desktop_config.json)",
            Self::Codex => "Codex (~/.codex/config.toml)",
            Self::Cursor => "Cursor (~/.cursor/mcp.json)",
        }
    }
}

pub(super) fn mcp(args: &mut Args) -> Result<McpCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(McpCommand::Serve);
    };
    Ok(match verb.to_str() {
        Some("serve") => McpCommand::Serve,
        Some("config") => {
            let client = args
                .text_option("--client")?
                .map(|key| {
                    McpClient::ALL
                        .iter()
                        .copied()
                        .find(|client| client.key() == key)
                        .ok_or(CliError::InvalidValue(
                            "--client claude-code|claude-desktop|codex|cursor",
                        ))
                })
                .transpose()?;
            McpCommand::Config { client }
        }
        _ => return Err(unknown("mcp", verb)),
    })
}
