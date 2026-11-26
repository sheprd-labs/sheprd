# Terminal backends

What Sheprd needs from whatever runs its terminals, through these traits.

- `Backend` lists, attaches, starts and ends terminals on one host.
- `WatchBackend` adds a snapshot and ordered change stream to backends that support it.
- `Terminal`: one attached terminal: semantic input, size, focus, the
  next surface updates, and its history (`History`): searches, copy-mode
  motions, text between two points, the whole text, scrolling, and clearing.
  History rows count from the top of the retained history, as Herdr 1's
  daemon counts them, so the app's find and copy mode read both alike.

`sheprd-core-client` implements all three traits for herdr-core.
`sheprd-tmux` implements `Backend` and `Terminal` using a dedicated tmux
server and control clients. Both feed `sheprd-terminal` and share surface
updates and history operations. Herdr 1 supplies rendered cells and session
snapshots directly through `sheprd-client`.

Whichever runs a terminal, the app receives `sheprd_protocol` cells (`FrameData`,
`PaneSurfacePatchRow`) and sends `ClientPaneInputEvent`, so painting, hit
testing, and input routing do not depend on the generation.

Every method may block on I/O: call them from a worker thread, never from
the UI thread. This crate has no GPUI, runtime, or transport of its own.
