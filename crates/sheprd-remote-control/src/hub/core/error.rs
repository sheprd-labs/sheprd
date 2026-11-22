#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Arrange(#[from] super::arrange::Error),
    #[error(transparent)]
    Client(#[from] sheprd_core_client::Error),
    #[error("The Herdr 2 terminal connection was lost")]
    Disconnected,
    #[error("No random id for a new workspace")]
    Random(#[source] getrandom::Error),
    #[error("This action is unavailable for Herdr 2 terminals")]
    Unsupported,
    #[error(transparent)]
    Worktree(#[from] super::worktree::Error),
    // keep-sorted end
}
