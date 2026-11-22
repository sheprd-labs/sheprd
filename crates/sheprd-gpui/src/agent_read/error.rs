//! Why a device's read marks could not be read or changed.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's read marks are not ones this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Daemon(#[from] sheprd_remote_control::app::Error),
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("Read marks cannot be shared with this kind of device")]
    Unsupported,
    // keep-sorted end
}
