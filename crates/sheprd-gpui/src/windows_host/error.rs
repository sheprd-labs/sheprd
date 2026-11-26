//! Why a Windows host's PowerShell gave no answer.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's PowerShell stopped answering.")]
    Closed,
    #[error("The host's PowerShell printed more than expected.")]
    Size,
    #[error("Could not start PowerShell for the host.")]
    Start(#[source] std::io::Error),
    #[error("The host's PowerShell took too long to answer.")]
    Timeout,
    // keep-sorted end
}
