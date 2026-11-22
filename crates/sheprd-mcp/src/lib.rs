//! A Model Context Protocol server over stdio, without an async runtime
//! (`docs/mcp-plan.md`). It speaks newline-delimited JSON-RPC 2.0: the
//! lifecycle (`initialize`, `ping`) and tools (`tools/list`, `tools/call`),
//! nothing else. What the tools are and do belongs to the [`Handler`] a
//! binary passes to [`serve`]: `sheprd mcp` turns them into control-socket
//! requests, and `sheprd-acp`'s planned servers into memory and thread reads.
//!
//! Every message is bounded by [`MAX_MESSAGE`], calls run one at a time in
//! the order they arrived, and a `ping` is answered while a call runs.

mod error;
mod jsonrpc;
mod line;
mod server;
mod session;
mod tool;

pub use error::{Error, Result};
pub use server::serve;
pub use tool::{
    Annotations, Call, ClientInfo, Content, Handler, MAX_CLIENT_NAME, Tool, ToolResult, UnknownTool,
};

/// The longest message either way, in bytes, newline excluded. It matches
/// the control socket's bound, which every Sheprd tool's answer crosses.
pub const MAX_MESSAGE: usize = 256 * 1024;

/// Protocol revisions this server speaks, newest first. A client asking for
/// one of them gets it; any other gets the newest, and may disconnect.
/// 2025-03-26 clients ignore the fields later revisions added (`title`,
/// `structuredContent`); what this server sends means the same in all three.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// Who the server is, as `initialize` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// A stable machine name, such as `sheprd`.
    pub name: String,
    /// What a client shows people, such as `Sheprd`.
    pub title: Option<String>,
    pub version: String,
}

#[cfg(test)]
mod tests;
