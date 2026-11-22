//! Why a restack did not go ahead or did not finish.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Commit or stash the checkout's changes before restacking.")]
    Dirty,
    #[error(
        "The branch's own commits include a merge, which a rebase would drop; restack it by hand."
    )]
    Merges,
    #[error("The checkout moved to another branch or commit; restack it again from its branch.")]
    Moved,
    #[error(
        "The rebase stopped: {details}. Resolve the conflicts and run `git rebase --continue`, or `git rebase --abort` to undo it."
    )]
    Stopped { details: String },
    #[error("The stack's base or the merged layer's commit cannot be restacked onto.")]
    Target,
    // keep-sorted end
}
