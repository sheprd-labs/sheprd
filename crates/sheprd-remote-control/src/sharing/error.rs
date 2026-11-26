use std::{io, path::PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("sharing state is already in use")]
    Busy,
    #[error("the sharing clock moved backwards; access is closed")]
    Clock,
    #[error("shared-session access is unavailable")]
    Denied,
    #[error("sharing is closed after a state failure")]
    Disabled,
    #[error("could not access sharing state at {}", path.display())]
    File {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the shared-session invitation is invalid")]
    InvalidInvitation,
    #[error("invalid sharing state")]
    InvalidState,
    #[error("shared-session I/O failed")]
    Io(#[from] io::Error),
    #[error("could not encode or decode sharing data")]
    Json(#[from] serde_json::Error),
    #[error("the sharing resource limit was reached")]
    Limit,
    #[error(transparent)]
    Private(#[from] sheprd_client::private_file::Error),
    #[error("could not create an invitation secret")]
    Random(#[source] getrandom::Error),
    #[error(transparent)]
    Relay(#[from] sheprd_relay::client::Error),
    #[error("the source session changed or is not ready to share")]
    Source,
    #[error("shared-session transport timed out")]
    Timeout,
    #[error(transparent)]
    Tunnel(#[from] sheprd_tunnel::Error),
    #[error("the sharing worker is unavailable")]
    Unavailable,
    #[error("unsupported shared-session protocol or record version")]
    Version,
    // keep-sorted end
}
