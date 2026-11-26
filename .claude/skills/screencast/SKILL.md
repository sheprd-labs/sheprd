---
name: screencast
description: Make a short narrated video screencast of a Sheprd feature, recorded in a virtual Mac in the Moonframe window, voiced by ElevenLabs in English and French, with smooth pointer moves, click rings, a lower-third title, an .srt for YouTube, a captioned cut for Twitter/X, and a page on sheprd.org/features. Use when the user asks for a screencast, demo video, feature video, the basics tour, or a clip of what a release adds for Twitter/X, YouTube, or the website.
---

# Feature screencasts

One feature, one video of about 30 seconds (the basics tour runs longer), in
the Moonframe window, in English and French. The tooling lives in
`scripts/screencast/` (read its `README.md` first). A feature is a steps file
and a transcript; `screencast.py` voices it, records it in the `screencast`
virtual Mac, and renders the MP4s; `website/tools/features.py` puts them on
the site. Never record or drive the user's own desktop: their display is
shared and their workspaces are private.

## What every video looks like

These are the owner's choices; keep them for every new video.

- **Moonframe window only**, at 1920x1080, 60 fps, 16:9 with no bars.
- **No macOS menu bar**: the machine hides it (auto-hide Always), so the
  window fills the frame. A video about a feature in a menu shows it for itself.
- **Pointer**: one size bigger than normal, and it glides to each target
  (`machine/hover.c`), never jumps.
- **Clicks** show a thin ring that grows and fades, placed on the click
  itself. A click that only gives a pane the keyboard before a shortcut is
  `"quiet"` and shows none, so no ring appears for no visible reason.
- **Title**: a lower third at the bottom, white text with a soft shadow, no
  box, during the first sentence.
- **Narration**: ElevenLabs `eleven_v4`, River in English, Julien in French
  (native accents only), at speed 0.9 with a second's pause after each
  sentence. No burned-in captions in the main cuts; the `.srt` is for YouTube.
- **Shortcuts**: when the narration says a macOS key, the Windows and Linux
  keys show on screen in the narration's language.
- **Tidy window**: close splits, pages and dialogs once they have been shown,
  so the window does not fill up.
- **Cover**: the page's still is a calm frame that shows the feature, picked
  by `poster` (seconds) in `website/features.json`, never a frame mid-click.

## 1. Pick the feature

For a release, read what it adds and agree on one feature per video:

```sh
gh release view -R sheprd-labs/sheprd --json tagName,body -q '.tagName, .body'
```

`design/sheprd-website/assets/features.js` lists every feature with a one-line
description, which makes a good first narration line.

## 2. Prepare the machine

The `screencast` virtual Mac is reserved for this (pinned in repo memory); it
is made from the `screencast-base` image. Check it with `sheprd machine list`
(running, 2560x1440) and `sheprd machine agent screencast` (both permissions),
and take a screenshot: awake, unlocked, nothing else in front. Use the dev
app's `sheprd` (`$SHEPRD`), not an older one on PATH.

```sh
set -a; source .envrc; set +a   # ELEVENLABS_API_KEY; git-ignored, never print it
python3 scripts/screencast/machine/prepare.py --app target/release/Sheprd.app --sign-ins
```

`prepare.py` runs `machine/setup.sh` in the guest (no sleep or lock, hidden
menu bar and Dock, Claude Code and Codex, the scripted demo agent as
`~/.local/bin/gemini`, the `~/code/forecast` demo repo, Sheprd's settings),
installs the pointer helper, installs a build with `--app`, and copies Claude
and Codex sign-ins without refresh tokens (Claude's last a few hours). Build
the app outside a Git worktree, or it shows a worktree banner: `git archive
HEAD | tar -x -C DIR`, symlink a warm `target/` into `DIR`, `just bundle`.
The pointer size cannot be scripted (macOS protects it): set Accessibility >
Display > Pointer size one step up by hand once, then save a new image.

If the machine is lost, `sheprd machine new screencast --from screencast-base`
brings it back. Another session may run its own VM beside it; macOS allows two.

## 3. Write the feature

Two files in `scripts/screencast/features/`, copied from `themes.*`:

- `NAME.md`, the transcript: `# Title`, a `fr:` line, then one `## N` section
  per segment with `en:` and `fr:` lines. Edit it to change what is said.
- `NAME.json`, the steps: `setup` (reset state, open the demo; not recorded),
  `segments` (one `actions` list per transcript section), `teardown`.

Actions: `["key", "cmd+shift+p"]`, `["type", "text"]`, `["slowtype", "t", 0.05]`,
`["click", x, y]` (add `"right"` and/or `"quiet"`), `["hover", x, y]`,
`["wait", s]`, `["repeat", n, [...]]`, `["exec", "shell in the guest"]`.
Coordinates are the guest screenshot's pixels (2560x1440); a
`{"moonframe": [x, y]}` map stands in when windows differ. A segment saying a
macOS key gets `"shortcut": "ctrl-shift-p"`, looked up in
`crates/sheprd-commands/src/shortcuts/linux.rs` (Windows takes Linux's unless
`windows.rs` overrides); they are rarely just Cmd for Ctrl.

Narrate what the viewer is about to see, in plain words, one idea per
sentence. Walk every step by hand with `machine click/key/type` and a
screenshot after each before recording, and again after any layout change
(the menu bar's height moves everything).

## 4. Record and render

```sh
python3 scripts/screencast/screencast.py scripts/screencast/features/NAME.json
python3 scripts/screencast/screencast.py scripts/screencast/features/NAME.json \
  --skip-record --captions --langs en     # the Twitter/X cut from the same take
```

A take: launch Sheprd fresh and Fill the window, run `setup`, start the
recorder, hide Sheprd for a moment as a sync mark (cut with the warm-up), play
the segments, run `teardown`, copy the movie out. A render takes seconds;
`--skip-record` re-renders after a transcript edit if each sentence still fits
its slot, and refuses otherwise. Outputs land in
`target/screencasts/moonframe/elevenlabs-river/NAME.LANG.mp4` and `.srt`.

## 5. Check before showing anyone

- A contact sheet of the whole video: `ffmpeg -i OUT.mp4 -vf
  "fps=1/2,scale=240:-1,tile=8x9" -frames:v 1 sheet.png`.
- Every click: the frames just before and after it, cropped around the
  point. The ring must sit under the hand pointer as the element reacts,
  never before the target appears or after it has gone.
- Nothing else on screen: no menu bar, Finder, dialog, lock screen, error,
  or the user's data. If the app shows a bug, stop and report it (or hand it
  to another agent) rather than recording around it.

## 6. Publish

```sh
cd website && python3 tools/features.py publish && python3 tools/features.py build
python3 tools/features.py posters        # after changing a `poster` alone
```

Page copy, order and `poster` live in `website/features.json`; the tour is its
`tour` entry. Serve `website/site` locally to look (another session may hold
port 8765). Push previews to the orphan `screencast-previews` branch for the
phone. Videos are committed through Git LFS. Posting to Twitter/X or YouTube,
or deploying the site, is the user's call.

## Gotchas

- `screencapture -v` writes frames only on screen changes, drops its last
  seconds when stopped, and can take seconds to start: the script keeps the
  menu bar clock ticking, waits 16 s, and aligns on the sync mark, never on
  the movie's end.
- Keys and Command-Q go to the front app: the script brings Sheprd forward
  first. A click on the desktop raises Finder; after a dialog or relaunch,
  click in the window before typing.
- Window > Fill (`ctrl+fn+f`) maximizes; a title bar double-click toggles.
- After the pointer rests on the status bar's CPU meter, the next click on
  the device footer is swallowed unless the pointer leaves the meter about
  3 s before (being fixed separately); move away right after pointing.
- Sheprd opens local pages only outside hidden folders, so demo pages come
  from `~/code/forecast`, not a worktree under `~/.herdr`.
- The agent button starts the agent used last: setups run `sheprd chat new
  gemini` first so the scripted agent answers.
- `machine exec` keeps the last 64 KB of output, refuses `rm` in `sh -c`
  (use `cp /dev/null`), and sometimes drops a connection (the copy retries).
  `osascript` hangs on a hidden permission prompt: avoid it.
