# sheprd-tmux

Enable the `tmux` Cargo feature on `sheprd-gpui`, `sheprd-acp` and `sheprd-remote-control` to include this adapter.
A plain workspace build skips this crate. To build all other members explicitly,
use `cargo build --workspace --exclude sheprd-tmux` with the feature disabled.

A `sheprd_backend::Backend` that runs one persistent tmux session per Sheprd terminal.
The desktop saves the host and terminal ID. A new control client can attach to that
address after the desktop restarts. An observer can read the screen and history
while the active client controls input and size.

Local commands use the user's login environment. A remote host supplies a
`CommandHost` which keeps its channel or authentication alive until a short command
finishes or a control client acknowledges attachment. The desktop's SSH, WSL and
cloud implementations live in `sheprd-remote`.

The adapter uses the dedicated `tmux -L sheprd` server and recognizes session names
of the form `tmux-<UUID>`. Named spawns reuse a deterministic address. Dropping a view
disconnects its client. Closing the owning pane ends the named session.

After a host reboot, opening a saved driving pane recreates its shell in the
original launch directory or runs `sheprd-acp --resume` for its exact agent
session. The helper stores its record privately on the terminal host. Closing
the pane clears the record. The tmux-enabled remote service also resumes saved chats after reboot. Observing a missing pane cannot launch a process.

## Live test

Choose an installed tmux explicitly. The test starts a private server in an isolated
environment and exercises both direct execution and the remote command contract
through a local POSIX shell.

```sh
HERDR_TEST_BINARY="$(command -v tmux)" HERDR_TEST_TMPDIR=/tmp \
  cargo test -p sheprd-tmux --features integration-test -- --include-ignored
```

See the [session model](../../docs/session-model.md#tmux) for the desktop features
that still depend on a Herdr service.
