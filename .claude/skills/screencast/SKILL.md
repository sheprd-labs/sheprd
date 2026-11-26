---
name: screencast
description: Make a short narrated video screencast of a Sheprd feature, recorded in a virtual Mac, voiced by ElevenLabs in English and French, in the classic and Moonframe windows, with a captioned cut for Twitter/X and an .srt for YouTube. Use when the user asks for a screencast, demo video, feature video, or a clip of what a release adds for Twitter/X, YouTube, or the website.
---

# Feature screencasts

One feature, one video of 30 seconds at most, per window and language. The
tooling lives in `scripts/screencast/` (read its `README.md` first): a feature
file says what to do in the app and what to say, and `screencast.py` voices
it, records it once per window in the `agent-test` machine, and renders the
MP4s. Never record the user's own desktop: their display is shared and their
workspaces are private.

## 1. Pick the feature

For a release, read what it adds, then agree on one feature per video with
the user:

```sh
gh release view -R sheprd-labs/sheprd --json tagName,body -q '.tagName, .body'
```

`design/sheprd-website/assets/features.js` lists every feature with a one-line
description, which makes a good first narration line.

## 2. Put the build in the machine

The machine must run the build that has the feature, at
`/Users/admin/Apps/Sheprd.app`. Keep the one it replaces in `~/Apps/old`.

- **A published release:** download `sheprd-VERSION-macos-universal.app.tar.gz`
  with `gh release download`, then push it in as below.
- **Unreleased `main`:** build outside a Git worktree, or the window shows a
  worktree banner: `git archive HEAD | tar -x -C DIR`, symlink a warm
  `target/` into `DIR`, then `just bundle` there.

Push a file in through the agent: `base64 | split -a 4 -b 190000`, then per
chunk `sheprd machine exec agent-test -- sh -c 'printf %s "$1" | base64 -D >>
/tmp/Sheprd.zip' _ "$(cat CHUNK)"`. Compare `shasum` on both sides before
unpacking. Quit Sheprd in the machine first (bring it to the front with `open
~/Apps/Sheprd.app`, then `machine key agent-test cmd+q`).

## 3. Check the machine

- `sheprd machine list`: `agent-test` running at 2560x1440. `sheprd machine
  agent agent-test` answers with both permissions.
- Take a `machine screenshot` and look at it: awake, unlocked (password
  `admin`), no Finder window or dialog in front, and no other session using
  the machine. If another agent's build or tabs are there, ask before
  replacing them.

## 4. Write the feature file

Copy `scripts/screencast/features/themes.json` to `features/NAME.json`:

- `title`: shown over the first segment, per language.
- `setup`: puts the app in a known state before recording: reset what the
  last run changed, open a clean terminal. Not recorded.
- `segments`: 4 to 6, each one sentence in `say` (en and fr, plus the other
  languages' text so they are ready) and the `actions` shown while it plays:
  `["key", "cmd+shift+p"]`, `["type", "text"]`, `["slowtype", "text", 0.05]`,
  `["click", x, y]` in screenshot pixels, `["wait", seconds]`,
  `["repeat", n, [actions]]`, `["exec", "shell command"]` run in the
  machine (to reopen the app mid-take, for example).
- `"fill": false` skips Window > Fill when the window already fills the
  screen (see Gotchas).
- `teardown`: undo what the video changed, such as the theme.

Narration: plain words a new user understands, the action before the result,
no keyboard-shortcut captions on visible buttons. Keep the total of the
longest takes under about 25 seconds; the script prints segment times.

Work out every click position from a screenshot first, and walk the steps by
hand with `sheprd machine key/type/click` and a screenshot after each, before
recording.

## 5. Record and render

```sh
set -a; source .envrc; set +a      # ELEVENLABS_API_KEY; .envrc is git-ignored
python3 scripts/screencast/screencast.py scripts/screencast/features/NAME.json
python3 scripts/screencast/screencast.py scripts/screencast/features/NAME.json \
  --skip-record --captions --langs en  # the Twitter/X cut from the same take
```

The key lives in the `.envrc` of the checkout that made the first videos;
copy that line, never print the key. Defaults: both windows
(`--window classic,moonframe`), English and French, River in English and
Julien in French (`scripts/screencast/README.md`, Voices). Spoken lines are
cached, so re-renders cost no characters.

Outputs, under `target/screencasts/WINDOW/elevenlabs-river/`:

| File | For |
| --- | --- |
| `NAME.LANG.mp4` | YouTube and the website, no captions |
| `NAME.LANG.srt` | YouTube's subtitle upload |
| `NAME.en.captioned.mp4` | Twitter/X, which autoplays muted |

All are 1920x1080 H.264 with AAC, which Twitter/X, YouTube and the site take
as is.

## 6. Check before showing anyone

Tile the video and look at every second against the narration:

```sh
ffmpeg -v error -i OUT.mp4 -vf "fps=1,scale=240:-1,tile=6x5" -frames:v 1 tile.png
```

Each action must appear while its sentence plays, with nothing else on screen
(no Finder, dialog, or error in the status bar). If the app shows a bug, stop
and tell the user rather than recording around it.

## 7. Hand it over

Push the MP4s to the orphan `screencast-previews` branch (under
`FEATURE/WINDOW/`) and link the GitHub pages, which play on a phone. Posting to
Twitter/X or YouTube, or publishing the website, is the user's call: never do
it without their explicit go-ahead for that post.

## Gotchas

- `screencapture -v` writes a frame only when the screen changes and its movie
  ends at the last one; the script turns on the menu bar clock's seconds so it
  keeps going. It can also take about ten seconds to start, which the script
  waits for and trims.
- Command-Q and keys go to the front app: bring Sheprd to the front with
  `open ~/Apps/Sheprd.app` before them. A click on the desktop raises Finder.
- After a dialog closes or the app relaunches, click inside the window
  before typing.
- Window > Fill (`ctrl+fn+f`) maximizes; a title bar double-click toggles.
  Sheprd Agent can leave Control held after it, so every later click opens
  a context menu and typing sends control characters. Restart Sheprd Agent
  in the machine to clear it, and set `"fill": false` once the window fills.
- Click into a terminal pane before typing into it: text typed right after
  a new tab or split can lose its first characters.
- A machine without Apple's command line tools shows an install dialog
  whenever Sheprd runs `git`. Install them once (`softwareupdate -i`, with
  the `.installondemand.in-progress` file in `/tmp`), and kill any dialog
  left over as root; it comes forward whenever Sheprd quits.
- A feature that needs a reboot is two feature files recorded on either side
  of it (`tmux-sessions.json`, `tmux-reboot.json`), joined afterwards.
- A theme set in a run sticks: `setup` must reset it.
- `machine exec` keeps only the last 64 KB of output and refuses `rm` inside
  `sh -c`; truncate with `cp /dev/null FILE`. `osascript` hangs on a hidden
  permission prompt: don't use it.
- Moonframe can draw with Rune only in apps written wholly in Luau. Sheprd's
  terminals and pages are a GPUI view, so there is no Rune version of Sheprd
  to record yet.
