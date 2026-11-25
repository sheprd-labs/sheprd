use std::path::PathBuf;

/// What can go wrong starting a thread.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The folder to start in cannot be read.
    #[error("could not read the checkout at {path}")]
    Checkout {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A linked worktree's `.git` file points nowhere this client can read.
    #[error("{path} is not a Git worktree this client understands")]
    Worktree { path: PathBuf },
    /// No provider the server runs can start a thread.
    #[error("T3 Code has no agent ready to start a thread")]
    NoAgent,
    /// The server named another provider or model than it runs.
    #[error("T3 Code runs no agent {0}")]
    UnknownAgent(String),
    /// An answer this client cannot read, such as a launch with no thread.
    #[error("T3 Code answered {0} without what this client needs")]
    Answer(&'static str),
    /// The thread list ended before its first snapshot.
    #[error("T3 Code ended the thread list: {0}")]
    Shell(String),
}
