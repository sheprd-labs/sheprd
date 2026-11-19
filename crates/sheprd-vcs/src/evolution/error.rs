//! Change history failures. Refusals shared with checkpoints (no checkout of
//! the branch, scripts that failed) arrive as `checkpoint::Error`.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("More than one version starts with that; give more of its name.")]
    Ambiguous,
    #[error("The branch's change history was not understood.")]
    Log,
    #[error("Change history output was not understood.")]
    Output,
    #[error("The change history kept changing while it was written.")]
    Raced,
    #[error("No version of this branch's commits starts with that.")]
    Unknown,
    // keep-sorted end
}
