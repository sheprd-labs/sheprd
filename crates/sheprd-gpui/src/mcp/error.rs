//! Why a tool's arguments were refused, or `sheprd mcp` could not serve.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{name} must be one of: {choices}")]
    Choice {
        name: &'static str,
        choices: &'static str,
    },
    #[error("Could not tell where this sheprd executable is")]
    Executable,
    #[error("{name} is required")]
    Missing { name: &'static str },
    #[error("The MCP server is off; turn on MCP server in Sheprd's Settings > General")]
    Off,
    #[error("{name} must be from {min} to {max}")]
    Range {
        name: &'static str,
        min: u64,
        max: u64,
    },
    #[error(transparent)]
    Serve(#[from] sheprd_mcp::Error),
    #[error("`sheprd mcp` serves only when an MCP client starts it")]
    ServeOnly,
    #[error("{name} must be at most {max} bytes")]
    TooLong { name: &'static str, max: usize },
    #[error("{name} must be {expected}")]
    Type {
        name: &'static str,
        expected: &'static str,
    },
    #[error("This tool takes no argument named {name}")]
    Unknown { name: String },
    // keep-sorted end
}
