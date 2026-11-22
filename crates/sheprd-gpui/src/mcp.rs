//! `sheprd mcp`: Sheprd as a Model Context Protocol server over stdio
//! (`docs/mcp-plan.md`). An MCP client starts it; each tool call becomes
//! the control-socket request the matching `sheprd` command sends, marked
//! as relayed (`Via::Mcp`), so the app applies the same targeting, bounds,
//! and dialogs, and refuses it while `[features] mcp` is off.
//!
//! The switch is read here too, from the config file, before every listing
//! and call: some tools (memory, checkpoints) need no app, so the app's
//! refusal cannot be all that gates them.

mod arguments;
mod error;
mod setup;
mod tools;

use crate::cli::control::{ControlCommand, McpCommand};
use crate::control::protocol::Via;
use crate::control::{Context, Output};
use arguments::Arguments;
pub(crate) use error::Error;
use sheprd_mcp::{Call, Handler, ServerInfo, Tool, ToolResult, UnknownTool};
use std::{io, process::ExitCode};

// A client name `sheprd_mcp` keeps always fits the control protocol.
const _: () = assert!(sheprd_mcp::MAX_CLIENT_NAME <= crate::control::protocol::MAX_CLIENT_NAME);

/// The longest error text a tool returns, as the CLI prints at most.
const MAX_ERROR: usize = 4096;

const ON: &str = "Sheprd is the app this agent's terminals run in. These tools read and drive what only Sheprd shows or keeps: its windows and tabs, browser pages and the user's notes on them, worktrees, pull requests, repository memory, and checkpoints. Search memory before a task and add what the next agent would otherwise have to find out again. checkpoint_restore, worktree_create, fan_out, and teleport only open Sheprd's own dialog, where the user decides; memory_forget removes an entry at once, so use it only for entries that are wrong.";
const OFF: &str = "Sheprd's MCP server is off, so it offers no tools. The user turns it on in Sheprd's Settings > General (MCP server); reconnect afterwards to see the tools.";

/// Serves MCP on standard input and output until the client closes them.
pub(crate) fn serve() -> ExitCode {
    let handler = Sheprd {
        enabled,
        run: crate::control::dispatch,
    };
    let input = io::BufReader::new(io::stdin());
    match sheprd_mcp::serve(server_info(), input, io::stdout(), handler) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Standard output belongs to the protocol.
            eprintln!("{}", crate::Error::from(Error::from(error)));
            ExitCode::from(crate::control::exit::REFUSED)
        }
    }
}

/// `sheprd mcp ...` other than serving, as `dispatch` runs it.
pub(crate) fn command(command: McpCommand) -> crate::Result<Output> {
    match command {
        McpCommand::Serve => Err(Error::ServeOnly.into()),
        McpCommand::Config { client } => setup::config(client),
    }
}

/// The `mcpServers` setting most clients read, for this executable; `None`
/// when the executable's path is unknown.
pub(crate) fn client_config() -> Option<String> {
    let executable = crate::agent_skill::executable()?;
    Some(setup::snippet(
        crate::cli::control::McpClient::ClaudeDesktop,
        &executable,
    ))
}

fn server_info() -> ServerInfo {
    ServerInfo {
        name: "sheprd".into(),
        title: Some("Sheprd".into()),
        version: crate::APP_VERSION.into(),
    }
}

/// Whether `[features] mcp` is on now, as the app would read it on its
/// first frame. A config that cannot be read keeps it off.
fn enabled() -> bool {
    crate::config::Config::load_startup().is_ok_and(|config| config.features.mcp)
}

/// The tools, answered through `run` while `enabled` says the switch is on.
struct Sheprd<E, R> {
    enabled: E,
    run: R,
}

impl<E, R> Handler for Sheprd<E, R>
where
    E: FnMut() -> bool,
    R: FnMut(ControlCommand, &Context) -> crate::Result<Output>,
{
    fn instructions(&mut self) -> Option<String> {
        Some(if (self.enabled)() { ON } else { OFF }.into())
    }

    fn tools(&mut self) -> Vec<Tool> {
        if !(self.enabled)() {
            return Vec::new();
        }
        tools::all().map(tools::Spec::tool).collect()
    }

    fn call(&mut self, call: Call) -> Result<ToolResult, UnknownTool> {
        let spec = tools::find(&call.name).ok_or(UnknownTool)?;
        if !(self.enabled)() {
            return Ok(failure(&Error::Off.into()));
        }
        let mut arguments = Arguments::new(call.arguments);
        let parsed = arguments
            .number("window", 0..=u64::MAX)
            .and_then(|window| Ok((window, (spec.build)(&mut arguments)?)))
            .and_then(|parsed| arguments.finish().map(|()| parsed));
        let (window, command) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => return Ok(failure(&error.into())),
        };
        crate::control::relay_as(Some(Via::Mcp {
            client: call.client.map(|client| client.name),
        }));
        let context = Context { json: true, window };
        Ok(match (self.run)(command, &context) {
            Ok(output) => answer(&output),
            Err(error) => failure(&error),
        })
    }
}

/// An answer as text and, when the command gave an object, as data too.
fn answer(output: &Output) -> ToolResult {
    let (text, data, failed) = output.parts();
    let mut result = match data {
        serde_json::Value::Object(data) => ToolResult::structured(text, data.clone()),
        // Lists and messages travel as an object, so clients reading data
        // always get one.
        other => ToolResult::structured(
            text,
            serde_json::Map::from_iter([("result".into(), other.clone())]),
        ),
    };
    result.is_error = failed;
    result
}

/// A failure the model can act on: its code, as `--json` names it, and the
/// message the CLI prints.
fn failure(error: &crate::Error) -> ToolResult {
    let message = crate::notifications::safe_text(&error.to_string(), MAX_ERROR);
    let code = crate::control::error_code(error);
    let mut result = ToolResult::error(format!("{code}: {message}"));
    result.structured_content = Some(serde_json::Map::from_iter([(
        "error".into(),
        serde_json::json!({ "code": code, "message": message }),
    )]));
    result
}

#[cfg(test)]
mod tests;
