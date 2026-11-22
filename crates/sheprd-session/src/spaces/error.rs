use super::{MAX_BYTES, MAX_ENTRIES};

/// Why a core's shared workspaces could not be read or changed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the shared workspaces are not readable")]
    Format(#[from] serde_json::Error),
    #[error("there are already {MAX_ENTRIES} shared workspaces or tabs")]
    Full,
    #[error("terminal {0} is already in another shared tab")]
    Held(sheprd_backend::TerminalId),
    #[error("a shared workspace or tab is not one Sheprd could keep")]
    Invalid,
    #[error("the shared workspaces are larger than {MAX_BYTES} bytes")]
    Large,
    #[error("no shared workspace {0}")]
    Missing(String),
    #[error("the shared workspaces are in format {0}, newer than this Sheprd writes")]
    Newer(u32),
    #[error(transparent)]
    Panes(#[from] crate::panes::Error),
    #[error("shared tab {0} changed its panes since its layout was read")]
    Stale(String),
    #[error("a shared workspace {0} is already kept")]
    Taken(String),
    // keep-sorted end
}
