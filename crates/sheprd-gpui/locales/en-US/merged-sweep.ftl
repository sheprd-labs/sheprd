# Removing every merged worktree at once: the sidebar broom and its dialog.


merged-sweep-title = { $count ->
    [one] Remove { $count } merged worktree?
   *[other] Remove { $count } merged worktrees?
}
merged-sweep-more = and { $count } more
merged-sweep-note = Each checkout is deleted and its workspace closed. A checkout with uncommitted files, commits after the merge, a working agent, or an archive script is kept.
merged-sweep-cancel = Cancel
merged-sweep-submit = Remove
merged-sweep-none = No merged worktrees to remove
merged-sweep-kept = { $count ->
    [one] Kept { $count } merged worktree: uncommitted files, newer commits, a working agent, or an archive script
   *[other] Kept { $count } merged worktrees: uncommitted files, newer commits, a working agent, or an archive script
}
merged-sweep-button = Remove merged worktrees
merged-sweep-running = Removing merged worktrees…
# $done of $total merged worktrees are removed or kept.
merged-sweep-count = { $done } of { $total }
merged-sweep-running-note = Other workspace actions wait until Git finishes each one. Hiding this does not stop it.
merged-sweep-hide = Hide
merged-sweep-stopped = Stopped removing merged worktrees: another device or connection is selected
