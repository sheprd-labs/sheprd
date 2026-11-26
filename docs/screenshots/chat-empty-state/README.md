# Empty chat state

macOS captures of the native chat renderer for a connected chat whose
transcript has no entries yet. The conversation is synthetic; no personal
session is included.

- [Before](before.png): the same chat drew an empty list.
- [Dark theme](dark.png), idle with a composer.
- [Light theme](light.png).
- [Narrow window](narrow.png).
- [Working](working.png): the agent is on its first turn.

Build the fixture with:

```sh
cargo build --locked -p sheprd-gpui --bin sheprd --features integration-test,mockup
```

Run it with `HERDR_TEST_RICH_CHAT=empty`,
`HERDR_TEST_RICH_CHAT_CAPTURE=/absolute/output.png`, and `--sidebar-test`.
Set `HERDR_TEST_RICH_CHAT_THEME=light`, `HERDR_TEST_RICH_CHAT_SIZE=narrow`, or
`HERDR_TEST_RICH_CHAT_WORKING=1` for the other variants. Use private `HOME`,
`XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `HERDR_CONFIG_PATH` when capturing.
The before capture came from the same fixture with the empty-state branch
disabled. No VM or installed app was changed.
