use super::wire::Refusal;
use crate::machine::Name;
use std::{io, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Lost the connection to Sheprd Agent: {0}")]
    Io(#[source] io::Error),
    /// The bytes the guest sent are not the PNG it said they were. The
    /// guest is not trusted with what lands at the caller's path.
    #[error("Sheprd Agent sent a screenshot that is not a PNG")]
    NotPng,
    #[error("{0} is not running")]
    NotRunning(Name),
    #[error("Sheprd Agent sent a screenshot this app cannot read")]
    Picture(#[source] base64::DecodeError),
    #[error("Sheprd Agent refused: {message}")]
    Refused { refusal: Refusal, message: String },
    #[error("Sheprd Agent sent a reply this app cannot read")]
    Reply(#[source] serde_json::Error),
    #[error("A message of {0} bytes is past the limit")]
    TooLarge(usize),
    /// What Virtualization said, as its message: an `NSError` cannot leave
    /// the main thread.
    #[error("Sheprd Agent does not answer in {name}: {reason}")]
    Unreachable { name: Name, reason: String },
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
