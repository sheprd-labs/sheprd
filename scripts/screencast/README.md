# Feature screencasts

Short narrated videos, one per feature, one MP4 per language, recorded in a
virtual Mac so nobody's own desktop or sessions show.

```sh
set -a; source .envrc; set +a   # ELEVENLABS_API_KEY (git-ignored)
python3 scripts/screencast/screencast.py scripts/screencast/features/themes.json
```

Each feature is recorded once per window (`--window classic,moonframe`, both
by default), so viewers can pick the look they use: the script quits Sheprd
in the machine and reopens it with that window's arguments (`WINDOWS` in
`screencast.py`) before recording, and keeps `preflight.png` of how it came
up. Add a window there (Rune is planned) with its arguments and the share of
the screen its maximized window fills.

- `features/NAME.json`: the steps Sheprd is driven through (`sheprd machine
  key/type/click`) and the narration of each segment in every language.
- `screencast.py`: voices every segment, records the machine once with each
  segment as long as its longest take, then renders
  `target/screencasts/WINDOW/elevenlabs-VOICE/NAME.LANG.mp4` and an `.srt` per
  language for YouTube's subtitles. `--captions` burns them in instead.
- `tts.py`: ElevenLabs (also OpenAI and macOS `say`, which were rejected).
  Spoken lines are cached in `target/screencasts/tts-cache`, so a re-render
  costs no characters until the narration changes.

Videos committed to the repository (the website's clips, for example) are
stored with Git LFS (`.gitattributes`): run `git lfs install` once per machine
before committing one.

## Voices

Chosen by listening to tryouts on 2026-11-22. The model is `eleven_v4`, with
each language's `language_code`.

| Language | Voice | Notes |
| --- | --- | --- |
| English and the default | River (`SAz9YHcvj6GT2YYXdXww`), or Eric (`cjVigY5qzO86Huf0OWal`) | Both liked; not yet decided. Premade voices, so ElevenLabs publishes no usage for either. |
| French | Julien, friendly tech explainer (`VuE88ydpXAhBTNxFI7WF`) | Native fr-FR. Fallback: Damien Doré, deep narrator (`kSNoLVC4Ox0vkBz1IKux`). |

Videos are made in English and French only for now (`screencast.py`'s
`LANGS`). The feature files keep narration for de, es, pt-BR, ja and zh-CN.

**Rule: every language gets a native voice.** Nobody on the team can judge
the other languages by ear, so before adding one, pick its voice from the
ElevenLabs library by data, not by listening: `locale` of that country
(de-DE, es-ES, pt-BR, ja-JP, zh-CN), `accent` "standard" (or the capital's),
a use case of `informative_educational` or `narrative_story`, and the most
users (`cloned_by_count`) and characters in the last year. Add it to
`ELEVENLABS_VOICES` and `ELEVENLABS_NATIVE` and to the table above, with a
fallback.

The English voices speak French with a Canadian accent, so a language with a
native pick in `tts.ELEVENLABS_NATIVE` uses it whatever voice is named on the
command line. `ELEVENLABS_VOICE_FR=damien` (or any voice ID) overrides it for
one run.

## The machine

- The `agent-test` machine runs a build of `main` at `~/Apps/Sheprd.app` with
  the Dock hidden, built outside a Git worktree (`git archive` into a plain
  folder, then `just bundle`) so it shows no worktree banner. Older builds
  are in `~/Apps/old`. Its display is a fixed 2560×1440 (`sheprd machine set
  agent-test --display 2560x1440`, applied at its next boot), so videos are
  16:9 with no bars; a display that follows its tab would change shape.
- Wake and unlock it first (`admin`), and check no other session is using it.
