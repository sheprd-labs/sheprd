# Git on the checkouts the app shows: commits, pushes, checkpoints, and the
# code search index. Git's own output stays as it is in $details.

error-vcs-cloud-host-unsupported = This is not available for cloud machines yet.

## Checkpoints, kept as Git refs on each host.

error-vcs-checkpoint-branch = That checkpoint was taken on another branch. Check that branch out to restore it.
error-vcs-checkpoint-busy = A merge, rebase, cherry-pick, or revert is in progress. Finish or abort it first.
error-vcs-checkpoint-checkout = No checkout of this branch was found on its host.
error-vcs-checkpoint-missing = That checkpoint no longer exists.
error-vcs-checkpoint-output = Checkpoint output was not understood.
error-vcs-checkpoint-script = Checkpoint Git commands failed
error-vcs-checkpoint-thread = Could not start the checkpoint worker
error-vcs-checkpoint-unborn = This checkout has no commit yet, so it has no checkpoints.
error-vcs-checkpoint-worker = The checkpoint worker stopped.

## Searching a checkout for files and symbols.

error-vcs-code-index-cancelled = Reading the checkout was cancelled
error-vcs-code-index-root = Only a local Git checkout can be searched for files and symbols

## Git commands. "git commit" and "git push" name Git's own commands.

error-vcs-git-busy = A Git operation is already running.
error-vcs-git-commit-message = Enter a commit message of at most 4096 characters.
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here. $details is Git's own output.
error-vcs-git-failed = { $kind ->
    [check-uncommitted] git check uncommitted files failed: { $details }
    [check-unpushed] git check unpushed commits failed: { $details }
    [commit] git commit failed: { $details }
    [count-changes] git count changes failed: { $details }
    [count-working-tree] git count working tree changes failed: { $details }
    [fetch-pull-request] git fetch the pull request branch failed: { $details }
    [fetch-stack-base] git fetch of the stack's base failed: { $details }
    [find-branch-start] git find where the branch started failed: { $details }
    [find-checkout] git find the checkout failed: { $details }
    [forget-fetched-base] git forgetting the fetched base failed: { $details }
    [list-attribute-files] git list attribute files failed: { $details }
    [list-branches] git list branches failed: { $details }
    [list-files] git list files failed: { $details }
    [list-merges] git listing the branch's merges failed: { $details }
    [list-untracked] git list untracked files failed: { $details }
    [list-working-tree] git list working tree changes failed: { $details }
    [prepare-remote-checkout] git prepare the remote pull request or issue checkout failed: { $details }
    [push] git push failed: { $details }
    [read-attributes] git read attributes failed: { $details }
    [read-branch-history] git read of the branch's history failed: { $details }
    [read-commit-subject] git read the commit subject failed: { $details }
    [read-default-branch] git read the remote's default branch failed: { $details }
    [read-stack-base] git read of the stack's base failed: { $details }
    [read-head] git read HEAD failed: { $details }
    [read-new-commit] git read the new commit failed: { $details }
    [read-working-tree-changes] git read working tree changes failed: { $details }
    [read-working-tree-state] git read working tree state failed: { $details }
    [stage] git stage changes failed: { $details }
   *[other] git { $operation } failed: { $details }
}
error-vcs-git-no-checkout = No local checkout is focused.
error-vcs-git-nothing-to-commit = Nothing to commit: the checkout has no changes.
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here.
error-vcs-git-process = { $kind ->
    [start-worker] Could not start the Git worker.
   *[other] Could not { $operation }.
}
error-vcs-git-pull-request-base = This branch is the repository default branch; no pull request can be opened from it.
error-vcs-git-pull-request-title = The branch has no commit to describe. Commit before opening a pull request.
error-vcs-git-worker = Git worker stopped. Retry the operation.
error-vcs-git-restack-dirty = Commit or stash the checkout's changes before restacking.
error-vcs-git-restack-merges = The branch's own commits include a merge, which a rebase would drop; restack it by hand.
error-vcs-git-restack-moved = The checkout moved to another branch or commit; restack it again from its branch.
# $details is Git's own output.
error-vcs-git-restack-stopped = The rebase stopped: { $details }. Resolve the conflicts and run `git rebase --continue`, or `git rebase --abort` to undo it.
error-vcs-git-restack-target = The stack's base or the merged layer's commit cannot be restacked onto.
