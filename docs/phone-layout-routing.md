# Phone layout ownership

iOS (`RemoteClient.action`, `rename`, and workspace rename) and the web
client use the same authenticated routes: `/api/term/{pane}/action`,
`/api/term/{pane}/rename`, and `/api/v2/workspaces/{workspace}/rename`.
Their handlers live in `sheprd-remote-control/src/web/terminal.rs`.

For Herdr 1 these call its session API. For Herdr 2, `core_keeper` identifies
the owner and `native::changes::arrange` routes the intent:

| Owner | Split, zoom and names | Persistence |
| --- | --- | --- |
| Shared document tab (`sheprd.spaces`, once written) | Daemon writes `spaces::Edit`s while the saved-tabs lock is free; otherwise the desktop answers through `control::layout`, asked by the saved tab holding the pane's terminal, or the daemon writes the document when no saved tab holds it | The core's `sheprd.spaces` document, which every client reads |
| Sheprd saved tab (while the document is at version 0) | Daemon edits while the saved-tabs lock is free; otherwise the desktop answers through `control::layout` | `browser-tabs.json`, read under the same lock when desktop opens |
| Daemon-owned tab | Daemon edits directly, including on a directly paired SSH/cloud host | `core-tabs.json`, scoped to the core boot identity |
| External Herdr desktop `herdr-app` tab | Explicitly refused; split/zoom/rename capabilities are omitted | Owned by the unrelated app; there is no supported mutation contract |

A recent desktop heartbeat is not ownership. It can outlive the desktop by
twelve seconds. The daemon acquires the saved-tabs lock before it edits or
starts a split shell, and queues only when that lock is held. If the desktop
exits while an intent is still queued, the daemon withdraws it and applies it
under the lock. An intent already taken by the desktop is **not replayed**:
its answer may have been lost after the change was saved. The phone receives
an error promptly and should refresh before deciding what to do next.

Desktop startup must also acquire this lock before reading or starting its
saved-tabs writer. If a slow phone operation still holds it after the two-second
startup budget, startup reports an ownership error and exits; opening Sheprd
again after the operation finishes reads the complete updated layout. It never
continues with an unlocked snapshot that could overwrite the phone's split.

Saved-file edits serialize concurrent phones, preserve unrelated fields, and
clean up a shell if recording its split fails. Splitting selects the new pane
and clears zoom; zooming selects its pane. Saved and daemon-owned selections
are projected without reordering the pane tree. Names, selection, and zoom
survive reading the corresponding file again.

## Closing a workspace or deleting its worktree

iOS `closeWorkspace` and `removeWorktree` call
`/api/v2/workspaces/{workspace}/close` and
`/api/v2/workspaces/{workspace}/remove-worktree`, handled in
`sheprd-remote-control/src/web/workspace.rs`. Herdr 1 gets `workspace.close`
for that workspace alone (`close_group: false`) and `worktree.remove`, forced
only for `?discard=true`. A workspace Sheprd keeps for Herdr 2 gets `Op::CloseWorkspace` or
`Op::RemoveWorktree` through `control::layout`, which the desktop makes as its
workspace menu does: closing ends the workspace's tabs, and deleting removes the
linked checkout with Git, forced only for `discard`, then closes the workspace.

With the desktop closed, it depends on where the workspace is kept:

- In the core's written `sheprd.spaces` document, the daemon makes both
  itself. It first refuses a document whose format this version cannot
  write, or a workspace it no longer keeps, so nothing ends or is deleted
  for a change that would not land. Closing ends the terminals of the
  workspace's tabs and of terminals labelled for it, and forgets it only
  once every tab still in it has had its terminals ended, so a tab another
  client adds meanwhile is not dropped while still running. Deleting runs
  `git worktree remove` on the checkout's root, with `--force` only for
  `discard`, keeps the
  branch, then closes the workspace.
- In the saved-tabs file (the document never written), the daemon makes
  neither and says to open Sheprd.

Deleting is refused before it reaches either keeper for a main checkout, and
for a checkout whose `.herdr/worktree.toml` has an archive script or cannot be
read: Sheprd's own delete dialog runs that script first. A checkout with
uncommitted changes stays unless the phone sends `?discard=true`, which it
does only after the person confirms losing them: up front when the phone
counted changes, or after a refusal for changes it had not counted. A Sheprd
that predates `discard` ignores it and keeps the checkout.

## Verification boundaries

Deterministic regressions cover stale/absent desktop routing, exit before and
after an intent is taken, concurrent saved edits, saved-file reopening,
daemon-owned restart/cleanup, selection projection, and failed-split cleanup.
The shared endpoints do not depend on a desktop relay when paired directly
to another host. Live iOS/web operation on SSH/cloud machines remains runtime
QA; mock tests are not evidence that native input or pairing was exercised.

Desktop adoption of formerly daemon-owned terminals is a separate transition:
`core_terminals/adopt.rs` currently creates single-pane tabs for labelled
terminals, and saved tabs then take precedence over daemon-owned groups.
This is not proof that a daemon-owned split is imported intact into the
desktop. A cross-client transfer contract is needed before claiming that
transition preserves grouping and names, especially across hosts. Saved-tab
reopening does preserve the file edited by the phone. Updating the desktop
while it is closed is an independent capability limit.
