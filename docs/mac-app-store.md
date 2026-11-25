# Mac App Store build

How Sheprd is built for the Mac App Store. The Developer ID release
([`updating.md`](updating.md)) is unchanged and remains the full app; the store
build is a flavor of it, selected by `sheprd-gpui`'s `mas` cargo feature.

## Decision: join a daemon, never start one

The store requires the App Sandbox, and every process a sandboxed app starts
inherits its sandbox, bundled helpers included. A herdr or herdr-core started
by the app would hold every shell, agent, `git`, and `ssh` in its terminals in
the app's container, unable to reach the user's checkouts, `~/.ssh`, or their
tools' settings. So the store build only joins a daemon the user started
outside the app (for example a Homebrew service), whose terminals are not
sandboxed, and reaches its socket through temporary file exceptions.

App Review may refuse the exceptions, or the need to install Herdr separately
(guideline 4.2.3). Plan for a rejection round and explain both in the review
notes.

## What `mas` changes

| Area | Store build | Where |
| --- | --- | --- |
| Self-update | Never checks, downloads, or installs; App Updates says the store updates the app, and the app never runs as the install helper | `sheprd-updater`'s `mas` feature, `Disabled::AppStore` |
| Starting herdr | Joins a running daemon only | `daemon::may_start` |
| Starting herdr-core | Joins a running core only | `core_terminals::hub::join` |
| Backdrop blur | GPUI's public `Blurred` appearance instead of the window server's private blur | `window/backdrop/macos.rs` |

## Entitlements

[`assets/macos/Sheprd-mas.entitlements`](../assets/macos/Sheprd-mas.entitlements)
replaces the Developer ID entitlements. It drops the camera, microphone,
contacts, calendars, location, photos, and Apple events, which only programs
in the terminals asked for and which a sandboxed terminal cannot pass on. Its
temporary exceptions name:

- `~/.config/herdr/` and `~/.local/state/herdr-core/`: the daemons' sockets and
  configuration.
- `~/.local/state/herdr/gpui/`: the app's control socket, so the `sheprd`
  command and `sheprd mcp` in terminals reach the app.
- `~/.claude/`, `~/.codex/` (read only): agent sessions for Traces.
- `~/.ssh/config`, `~/.ssh/known_hosts` (read only): SSH devices.

## Still to do

1. Resolve the excepted paths from the real home directory: under the sandbox
   `HOME` is the container. Explain in the window how to start Herdr when no
   daemon answers.
2. Security-scoped bookmarks for checkouts the user opens, so `git` reaches them.
3. Leave out what the store refuses: launch agents (`sheprd-remote-control`,
   `sheprd-automate`), installing builds on other hosts, reading other apps'
   cookies and keychain items, writing agent skills and hooks into other tools'
   settings, and virtual Macs.
4. Trim `Info.plist` usage strings, add `LSApplicationCategoryType` and a
   privacy manifest (`PrivacyInfo.xcprivacy`).
5. Sign helpers with `app-sandbox` and `inherit`, embed the provisioning
   profile, build the installer package with `productbuild`, and upload it to
   TestFlight for Mac from a protected CI job.

`lb-wry` sets WebKit's `allowsPictureInPictureMediaPlayback` preference by key;
apps built on wry are accepted, but it is the one other private-looking call
left.

## Checking it

```sh
cargo check -p sheprd-gpui --features mas
cargo test -p sheprd-updater --features mas app_store
just bundle release mas   # a local, ad-hoc signed, unsandboxed app with the flavor's code
```

CI's all-features legs build and test with `mas`, and the default legs
without it. No release job signs with the store entitlements yet.
