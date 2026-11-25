//! Why a linked host's daemon could not be told a phone is near.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("This kind of host cannot be reached")]
    Unsupported,
    // keep-sorted end
}
