use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why [`crate::serve`] stopped before its input ended. A malformed or
/// refused message is not one: the client gets a JSON-RPC error and the
/// server goes on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not encode an MCP message: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("Could not read MCP input: {0}")]
    Read(#[source] io::Error),
    #[error("The MCP input reader stopped unexpectedly")]
    ReaderPanicked,
    #[error("Could not start the MCP input reader: {0}")]
    Spawn(#[source] io::Error),
    #[error("Could not write MCP output: {0}")]
    Write(#[source] io::Error),
    // keep-sorted end
}
