# Shared spaces

The workspaces Sheprd keeps for Herdr 2, and their tabs, move from each
desktop's own saved tabs into a document of the herdr-core that runs their
terminals, so every client of that core shows the same ones.

## The problem

herdr-core has no workspaces. The app keeps them, with their tabs, in its
own `browser-tabs.json` (`docs/session-model.md`), under the scope of the
host whose core runs them. Another computer that shows the same host over
SSH reads its own file, never that host's, so it lists only what it kept
there itself. A person who made a workspace on their desktop or phone does
not see it from their laptop, though its agents run on the same core.

`docs/session-model.md` chose the file and named when to revisit: "when a
second client on another machine needs a core's layout". That is now.

## The document

`sheprd_session::spaces` (format) and `sheprd_core_client::spaces` (I/O),
added with this plan:

- One document per core, named `sheprd.spaces` (`spaces::DOC`), JSON,
  at most 256 KiB (`spaces::MAX_BYTES`), at most 256 workspaces and 256
  tabs. A record that fails to read costs only itself.
- `Spaces { format, workspaces: [Workspace], tabs: [Tab] }`.
  - `Workspace { id, label, custom, cwd }`, in the user's order. `cwd` is a
    path on the core's computer.
  - `Tab { id, workspace_id, title, name?, names, panes, zoomed? }`: a tab's
    terminals and split layout. Ids are strings, unique in the document and
    never reused.
- Shared: which workspaces exist, their names, folders and order; each
  tab's terminals, layout, names, and zoom. Per window, never in the
  document: which tab or workspace is in front, the selected pane
  (`active`), mirror tabs (`observe`), folded groups.
- Changes are `spaces::Edit`s (`KeepWorkspace`, `RenameWorkspace`,
  `MoveWorkspace`, `ForgetWorkspace`, `OpenTab`, `CloseTab`, and `Tab`
  with one `TabEdit`: `Split`, `RemovePane`, `SetPanes`, `Zoom`, `Rename`,
  `RenamePane`, `Retitle`, `Move`). Each names only what it changes, so a
  rename on one computer and a zoom on another both stay, and an edit to a
  tab another client closed changes nothing. `SetPanes`, for resizing or
  swapping, is refused as `Stale` once the tab holds other terminals, so
  re-read and redo it. They are applied with
  `sheprd_core_client::spaces::update(core, &edits)`, or any change with
  `spaces::change(core, |spaces| ...)`. Both write over the version they
  read; when another client wrote first, they apply the edit again to its
  document, so both changes stay. Blocking: call them on a worker thread.
- A newer `format` is read for what this version knows and never written
  (`Spaces::writable`).
- New workspace ids are `space-` and 16 random hex digits; new tab ids
  likewise with `tab-`. The `space-N` counter the app uses today collides
  across clients.
- `Spaces::adopt(workspaces, tabs, new_id)` merges what one client kept
  before the document existed: a folder already kept joins that workspace,
  a taken id takes a new one, a tab whose terminals another tab holds is
  left out. It answers the ids taken, to rename the client's own records.

### Contracts between clients

- A terminal some document tab holds is never adopted again by a desktop
  (`core_terminals::adopt`), whoever put it there.
- Whoever starts a terminal for a new kept workspace (a phone, fan-out,
  teleport) may write the workspace and its tab to the document itself,
  rather than only labelling the terminal (`NEW_WORKSPACE_LABEL`) for a
  desktop to adopt. The labels stay for desktops of earlier versions.
- `browser-tabs.json` keeps being written by the app as today: it is the
  offline copy, and the remote control daemon falls back to it while a
  core's document has never been written (version 0).

## Work, in parallel

Each part is its own pull request on top of this one. Parts touch separate
crates; anything either needs in `sheprd-session` or `sheprd-core-client`
goes in new functions or files, not edits to shared lines.

### Desktop (`sheprd-gpui`)

- Read: for each core a window reaches, this computer's and each saved
  host's (`core_terminals::Host`), read `sheprd.spaces` on that host's
  existing worker, again when it may have changed (a cheap `doc.get` every
  couple of seconds while connected, compared by version). Mirror the
  document into the `browser::Store`'s kept workspaces and core tabs of
  that scope, keeping per-window state. A remote host then lists what that
  host's desktop and phones made.
- Write: every change the app makes to a kept workspace or a core tab of a
  scope (keep, rename, move, close, open or close a tab, split, zoom,
  rename a tab or pane, move a pane) also sends the matching `Edit` for
  that core, on the worker, applied locally at once as today. A refused
  edit (bounds, newer format) shows the existing flash and leaves the
  document as it is.
- Map document tab ids to the store's `TabId`s, kept with the tab.
- Migrate: the first time the app reads a core's document, `adopt` what it
  keeps for that scope, write it, and rename its own records by the ids
  answered.
- Adoption: skip a terminal a document tab holds; a phone's new workspace
  kept by adoption is written to the document.
- Tests: headless, with a fake core document, including two windows
  standing for two computers, a conflict, migration twice, and a refused
  newer format. `just test-core` against a real `herdr-core` for the size
  bound, when one is available.

### Remote control daemon (`sheprd-remote-control`)

- Read kept workspaces and saved core tabs from the core's `sheprd.spaces`
  (`hub/core/poller.rs`, `spaces.rs`, `saved.rs`), falling back to
  `browser-tabs.json` while the document's version is 0.
- Tab targets: a document tab's id in the layout (`core:tab:` and the id),
  still accepting the file's numeric ids while the fallback is read.
- While Sheprd runs, phone layout changes still go through it
  (`control::layout`), which writes the document. While it is closed,
  write `Edit`s to the document rather than editing the file
  (`hub/core/arrange.rs`, `saved::edit`): split, zoom, renames, close; and
  closing a kept workspace or deleting its worktree.
- A phone's new workspace or worktree on the core (`hub/new_workspace.rs`)
  writes the workspace and its first tab to the document itself, with the
  title the phone gave, and still labels the terminal for older desktops.
- Tests: the existing fake core (`hub/core/tests.rs`, `socket`) answering
  `doc.get`/`doc.set` for `sheprd.spaces`, including the fallback and a
  conflict.

### After both

- `docs/session-model.md`: the Workspaces row and the "Layout stays in the
  app's saved tabs" decision describe the document.
- Manual: two computers showing one host see the same kept workspaces and
  tabs; a rename on one shows on the other; a phone's new workspace shows
  on both with no desktop running on the host.
