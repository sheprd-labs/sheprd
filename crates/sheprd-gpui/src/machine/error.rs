use super::Name;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Agent(#[from] super::agent::Error),
    #[error("{0} is in use: shut it down first")]
    Busy(Name),
    #[error(
        "No VM slots available: {in_use} of {limit} in use. Wait for a VM to stop or reuse an unreserved running VM."
    )]
    Capacity { in_use: u32, limit: u32 },
    #[error("A machine takes {min} to {max} CPUs on this Mac")]
    Cpus { min: u16, max: u16 },
    #[error("{}: {source}", path.display())]
    Delete {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0} already exists")]
    Exists(Name),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Install(#[from] super::install::Error),
    /// What Virtualization said, as its message: an `NSError` cannot leave
    /// the main thread.
    #[error("Virtualization refused the machine: {0}")]
    Framework(String),
    #[error("This machine was made for hardware this Mac does not have")]
    Hardware,
    #[error("No image is called {0}")]
    ImageNotFound(Name),
    #[error("A display is `fit` or a size such as 1920x1200")]
    InvalidDisplay,
    #[error("{}: {reason}", path.display())]
    InvalidMetadata { path: PathBuf, reason: String },
    #[error(
        "A machine name takes letters, digits, '-', and '_', and starts with a letter or digit"
    )]
    InvalidName,
    #[error("A machine takes {min} to {max} GB of memory on this Mac")]
    Memory { min: u16, max: u16 },
    #[error("Open {installed} instead of {requested} to use the installed machine name")]
    NameMismatch { requested: Name, installed: Name },
    #[error("No state directory to keep machines in")]
    NoStateDir,
    #[error("No machine is installed at {}", .0.display())]
    NotInstalled(PathBuf),
    #[error("{}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Remote(#[from] super::remote::Error),
    #[error("{0} is suspended: shut it down or forget its saved state first")]
    Suspended(Name),
    #[error("This Mac cannot run virtual Macs")]
    Unsupported,
    #[error("{name} does not support {capability}")]
    UnsupportedCapability {
        name: Name,
        capability: &'static str,
    },
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
