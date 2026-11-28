# Guest prompts in the desktop chat

macOS captures of the native chat renderer for a chat shared through an
external tool. Bob's earlier message, which the person allowed, is named
above or in place of the speaker; his next prompt waits for the person with
every word it would send. The conversation is synthetic; no personal session
is included.

- [Dark theme](dark.png), bubbles layout.
- [Light theme](light.png).
- [Narrow window](narrow.png).
- [Working](working.png): the agent's turn waits on the request.
- [Document layout](document.png): Bob is named where the person's messages
  say "You".
- [Timeline layout](timeline.png), the default.

There is no before capture: chats had no guests. A client older than
revision 6 shows a guest's message as text starting `Bob via tma: `.

Build the fixture with:

```sh
cargo build --locked -p sheprd-gpui --bin sheprd --features integration-test,mockup
```

Run it with `HERDR_TEST_RICH_CHAT=guests`,
`HERDR_TEST_RICH_CHAT_CAPTURE=/absolute/output.png`, and `--sidebar-test`.
Set `HERDR_TEST_RICH_CHAT_THEME=light`, `HERDR_TEST_RICH_CHAT_SIZE=narrow`,
`HERDR_TEST_RICH_CHAT_WORKING=1`, or `HERDR_TEST_RICH_CHAT_LAYOUT=document`
or `timeline` for the other variants. Use private `HOME`, `XDG_CONFIG_HOME`,
`XDG_STATE_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, and `HERDR_CONFIG_PATH`
when capturing. No VM or installed app was changed.
