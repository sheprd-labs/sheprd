use std::path::PathBuf;
use thiserror::Error;

/// Why a new machine could not be made from Apple's macOS.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Stopped before it finished")]
    Cancelled,
    #[error("Apple's server sent {got} of {expected} bytes")]
    Incomplete { got: u64, expected: u64 },
    #[error("Cannot download macOS: {0}")]
    Download(#[source] Box<ureq::Error>),
    /// What Virtualization said, as its message: an `NSError` cannot leave
    /// the thread it arrived on.
    #[error("Cannot find macOS for this Mac: {0}")]
    Fetch(String),
    #[error("{}: {source}", path.display())]
    File {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("macOS did not install: {0}")]
    Install(String),
    #[error("Needs {needed} bytes free, and {free} are")]
    NoRoom { needed: u64, free: u64 },
    #[error("This Mac cannot run the macOS Apple offers")]
    NotSupported,
    #[error("Apple's server answered {0}")]
    Status(u16),
}
