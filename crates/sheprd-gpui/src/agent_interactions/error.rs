//! Typed errors for agent interactions and the secret-safe vault.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("secret key name is invalid: must match [A-Za-z_][A-Za-z0-9_]*")]
    InvalidKey,
    #[error("secret destination is invalid: expected env-file:<relative-path> or keychain:<name>")]
    InvalidDestination,
    #[error("secret destination path must stay inside the workspace root")]
    PathEscapesRoot,
    #[error("refusing to write secret through symlink {path}")]
    SymlinkRefused { path: PathBuf },
    #[error("secret value cannot be empty")]
    EmptySecret,
    #[error("secret value is too large (max {limit} bytes)")]
    SecretTooLarge { limit: usize },
    #[error("cannot determine the source workspace for this secret request")]
    WorkspaceNotFound,
    #[error("failed to write secret to {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Credentials(#[from] sheprd_credentials::Error),
}
