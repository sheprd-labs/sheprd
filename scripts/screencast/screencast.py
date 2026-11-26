#!/usr/bin/env python3
"""Record a narrated, captioned screencast of one Sheprd feature in a virtual Mac.

  screencast.py FEATURE.json [--machine NAME] [--out DIR] [--langs en,fr,...]
                [--window classic,moonframe]
                [--tts openai,elevenlabs,say] [--captions] [--skip-record]

1. Speaks every segment in every language with each voice provider (`tts.py`:
   OpenAI, ElevenLabs, or macOS `say`) and measures it, so each segment lasts
   as long as its longest take.
2. Drives Sheprd inside the machine through `sheprd machine` while the guest
   records its own screen with `screencapture -v`, logging when each segment
   starts.
3. Pulls the recording back and renders one MP4 per voice and language: the cropped,
   scaled screen, a title, and the voice, plus an SRT for YouTube; --captions
   also burns the narration in.

Recording happens once; languages differ only in voice and captions.
"""

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

# The `sheprd` command of the running app that shows the machine: $SHEPRD,
# else the one on PATH, else the installed app's.
SHEPRD = (
    os.environ.get("SHEPRD")
    or shutil.which("sheprd")
    or "/Applications/Sheprd.app/Contents/MacOS/sheprd"
)

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tts  # noqa: E402

# Narration exists for every language in tts.LANGUAGE_NAMES; videos are made
# in these until each other language has a native voice picked.
LANGS = ["en", "fr"]

LATIN_FONT = "/System/Library/Fonts/SFNS.ttf"
CJK_FONT = "/System/Library/Fonts/Hiragino Sans GB.ttc"

WIDTH, HEIGHT = 1920, 1080
LEAD_IN = 1.0  # seconds of screen before the first segment
# screencapture can take seconds to start recording, so wait this long before
# the first segment and trim what it caught before LEAD_IN.
WARMUP = 12.0
GAP = 0.5  # silence after each segment's voice
TAIL = 1.5  # seconds after the last segment
CHUNK = 45000  # bytes per pulled chunk; base64 of it stays under the exec output cap
APP = "~/Apps/Sheprd.app"  # where the machine keeps the build being recorded

# Each window the app can open, as the arguments that open it and the share of
# the guest screen's height, from the top, the maximized window fills (the
# rest is desktop). A video is recorded once per window asked for, so people
# can pick the one they use. Rune is to come.
WINDOWS = {
    "classic": (["--args", "--classic"], 1.0),
    "moonframe": (["--env", "SHEPRD_MOONFRAME=1"], 1.0),
}


def run(*args, **kw):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kw).stdout


def machine(name, *args):
    return run(SHEPRD, "machine", *args[:1], name, *args[1:])


# --- voice -----------------------------------------------------------------


def duration(path):
    return float(
        run("ffprobe", "-v", "error", "-show_entries", "format=duration",
            "-of", "csv=p=0", str(path)).strip()
    )


def slug(provider):
    return provider.replace(":", "-")


def voice_file(work, provider, lang, i):
    return work / "voice" / slug(provider) / lang / f"{i}.{tts.extension(provider)}"


def speak(spec, providers, langs, work, cache):
    """Voice every segment with every provider; return each segment's longest take."""
    longest = [0.0] * len(spec["segments"])
    for provider in providers:
        for lang in langs:
            for i, seg in enumerate(spec["segments"]):
                path = voice_file(work, provider, lang, i)
                path.parent.mkdir(parents=True, exist_ok=True)
                tts.synthesize(provider, seg["say"][lang], lang, path, cache)
                longest[i] = max(longest[i], duration(path))
    return longest


# --- recording -------------------------------------------------------------


def act(name, action):
    kind, *args = action
    if kind == "wait":
        time.sleep(args[0])
    elif kind == "key":
        machine(name, "key", args[0])
    elif kind == "type":
        machine(name, "type", args[0])
    elif kind == "slowtype":
        for ch in args[0]:
            machine(name, "type", ch)
            time.sleep(args[1])
    elif kind == "click":
        machine(name, "click", str(args[0]), str(args[1]))
    elif kind == "exec":
        # A shell command in the machine, such as reopening the app.
        machine(name, "exec", "--", "sh", "-c", args[0])
    elif kind == "repeat":
        for _ in range(args[0]):
            for inner in args[1]:
                act(name, inner)
    else:
        sys.exit(f"unknown action {kind}")


def launch(name, window, work, fill=True):
    """Quit Sheprd in the machine and open it in `window`, keeping a screenshot
    of how it came up in work/preflight.png. Without `fill`, the window keeps
    the size it reopened at."""
    # Activate it first: Command-Q goes to whichever app is in front.
    machine(name, "exec", "--", "sh", "-c", f"open {APP}")
    time.sleep(1)
    machine(name, "key", "cmd+q")
    for _ in range(20):
        time.sleep(0.5)
        alive = machine(name, "exec", "--", "sh", "-c", "pgrep -x sheprd || true").strip()
        if not alive:
            break
    else:
        sys.exit("Sheprd did not quit in the machine")
    args = " ".join(WINDOWS[window][0])
    machine(name, "exec", "--", "sh", "-c",
            f"open {APP} {args}")
    for _ in range(40):
        time.sleep(0.5)
        if machine(name, "exec", "--", "sh", "-c", "pgrep -x sheprd || true").strip():
            break
    else:
        sys.exit("Sheprd did not start in the machine")
    time.sleep(8)
    # Bring it to the front before any click: one that lands on the desktop
    # while Sheprd is still drawing its window raises Finder instead, and the
    # setup then types into Finder.
    machine(name, "exec", "--", "sh", "-c", f"open {APP}")
    time.sleep(1)
    # Window > Fill (Globe-Control-F): unlike a title bar double-click, it
    # fills the screen whatever the window's size was, never toggling back.
    if fill:
        # Sheprd Agent can leave Control held after this shortcut, turning
        # later clicks into right-clicks until the agent restarts.
        machine(name, "key", "ctrl+fn+f")
        time.sleep(1.5)
    work.mkdir(parents=True, exist_ok=True)
    machine(name, "screenshot", str(work / "preflight.png"))


def record(spec, name, slots, work):
    """Run the feature in the machine while it records; return segment start times."""
    machine(name, "exec", "--", "sh", "-c",
            "nohup caffeinate -du -t 900 >/dev/null 2>&1 &")
    # screencapture writes a frame only when the screen changes, and its
    # movie ends at the last one. A ticking menu bar clock keeps frames coming
    # every second, so the movie ends when it is stopped, which the segment
    # times are measured back from.
    machine(name, "exec", "--", "sh", "-c",
            "defaults write com.apple.menuextra.clock ShowSeconds -bool true; "
            "killall ControlCenter; sleep 2")
    for a in spec.get("setup", []):
        act(name, a)

    movie = f"/tmp/screencast-{spec['id']}-{int(time.time())}.mov"
    recorder = machine(name, "exec", "--", "sh", "-c",
                       f"nohup screencapture -v -x {movie} >/tmp/screencast.log 2>&1 & "
                       "echo $!").strip()
    if not recorder.isdigit():
        sys.exit(f"screencapture did not start in the machine: {recorder}")
    t0 = time.monotonic()

    def stop():
        # SIGINT makes screencapture finish the movie; only this run's recorder.
        machine(name, "exec", "--", "sh", "-c", f"kill -INT {recorder}; sleep 2")

    try:
        time.sleep(WARMUP)
        starts = []
        for seg, slot in zip(spec["segments"], slots):
            begin = time.monotonic()
            starts.append(begin - t0)
            for a in seg["actions"]:
                act(name, a)
            left = slot - (time.monotonic() - begin)
            if left > 0:
                time.sleep(left)
        time.sleep(TAIL)
        stopped = time.monotonic() - t0
    except BaseException:
        # A failed action or Control-C: stop the recorder and put the app back
        # as best we can, then report the original failure.
        for cleanup in [stop] + [lambda a=a: act(name, a) for a in spec.get("teardown", [])]:
            try:
                cleanup()
            except (subprocess.CalledProcessError, OSError) as error:
                print(f"cleanup step failed: {error}", file=sys.stderr)
        raise
    stop()
    for a in spec.get("teardown", []):
        act(name, a)

    raw = work / "screen.mov"
    pull(name, movie, raw)
    # The movie ends when it was stopped; place segment starts on its clock.
    offset = duration(raw) - stopped
    starts = [s + offset for s in starts]
    (work / "timing.json").write_text(json.dumps({"starts": starts, "slots": slots}))
    return starts


def pull(name, remote, local):
    size = int(machine(name, "exec", "--", "stat", "-f", "%z", remote).strip())
    with open(local, "wb") as f:
        for i in range((size + CHUNK - 1) // CHUNK):
            out = machine(name, "exec", "--", "sh", "-c",
                          f"dd if={remote} bs={CHUNK} skip={i} count=1 2>/dev/null | base64")
            f.write(base64.b64decode(out))
    if local.stat().st_size != size:
        sys.exit(f"pulled {local.stat().st_size} of {size} bytes")


# --- rendering -------------------------------------------------------------


def font(lang):
    return CJK_FONT if lang in ("ja", "zh-CN") else LATIN_FONT


def caption(text, lang, path, size, width):
    run("magick", "-background", "none", "-fill", "white", "-font", font(lang),
        "-pointsize", str(size), "-size", f"{width}x", "-gravity", "center",
        f"caption:{text}", "-bordercolor", "none", "-border", "28x16",
        "(", "+clone", "-background", "#000000c0", "-alpha", "background",
        "-channel", "A", "-evaluate", "set", "78%", "+channel", ")",
        "+swap", "-composite", str(path))


def title_card(text, lang, path):
    run("magick", "-background", "#000000b0", "-fill", "white", "-font", font(lang),
        "-pointsize", "64", f"label:{text}", "-bordercolor", "#000000b0",
        "-border", "40x24", str(path))


def srt_time(t):
    ms = int(round(t * 1000))
    return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02},{ms % 1000:03}"


def render(spec, provider, lang, starts, work, window, out, captions=False, crop=None):
    """Render one language and voice of the recording in work/WINDOW."""
    screen = work / window / "screen.mov"
    crop = crop or WINDOWS[window][1]
    # Keep LEAD_IN of screen before the first segment; the recorder's warm-up
    # before that is cut.
    head = max(0.0, starts[0] - LEAD_IN)
    starts = [start - head for start in starts]
    d = work / window / "render" / slug(provider) / lang
    d.mkdir(parents=True, exist_ok=True)
    title_card(spec["title"][lang], lang, d / "title.png")

    inputs = ["-i", str(screen), "-loop", "1", "-i", str(d / "title.png")]
    filters = [
        # Keep the top `crop` of the guest screen (the window, not the desktop below).
        # screencapture writes a frame only when the screen changes: make the
        # rate constant before cutting the warm-up, and hold the last frame.
        f"[0:v]fps=30,trim=start={head:.3f},setpts=PTS-STARTPTS,"
        f"tpad=stop_mode=clone:stop_duration=5,crop=iw:ih*{crop}:0:0,scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease,"
        f"pad={WIDTH}:{HEIGHT}:(ow-iw)/2:(oh-ih)/2:color=0x101014[v0]",
        f"[v0][1:v]overlay=(W-w)/2:150:shortest=1:enable='lt(t,{starts[1]:.3f})'[v1]",
    ]
    srt, audio_labels = [], []
    last = "v1"
    burned = 0
    for i, seg in enumerate(spec["segments"]):
        length = duration(voice_file(work, provider, lang, i))
        start, end = starts[i], starts[i] + length + GAP
        srt.append(f"{i + 1}\n{srt_time(start)} --> {srt_time(end)}\n{seg['say'][lang]}\n")
        if not captions:
            continue
        cap = d / f"cap{i}.png"
        caption(seg["say"][lang], lang, cap, 46, WIDTH - 360)
        inputs += ["-loop", "1", "-i", str(cap)]
        n = 2 + burned
        burned += 1
        filters.append(
            f"[{last}][{n}:v]overlay=(W-w)/2:H-h-70:shortest=1:"
            f"enable='between(t,{start:.3f},{end:.3f})'[v{n}]"
        )
        last = f"v{n}"
    base = 2 + burned
    for i in range(len(spec["segments"])):
        inputs += ["-i", str(voice_file(work, provider, lang, i))]
        ms = int(starts[i] * 1000)
        filters.append(f"[{base + i}:a]adelay={ms}|{ms},aresample=48000[a{i}]")
        audio_labels.append(f"[a{i}]")
    filters.append(
        f"{''.join(audio_labels)}amix=inputs={len(audio_labels)}:normalize=0,"
        f"loudnorm=I=-16:TP=-1.5,apad[aout]"
    )

    out.mkdir(parents=True, exist_ok=True)
    # Captioned cuts (Twitter/X autoplays muted) sit beside the plain ones.
    suffix = ".captioned" if captions else ""
    mp4 = out / window / slug(provider) / f"{spec['id']}.{lang}{suffix}.mp4"
    mp4.parent.mkdir(parents=True, exist_ok=True)
    run("ffmpeg", "-y", "-hide_banner", "-loglevel", "error", *inputs,
        "-filter_complex", ";".join(filters), "-map", f"[{last}]", "-map", "[aout]",
        "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-pix_fmt", "yuv420p",
        "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart",
        "-t", f"{duration(screen) - head:.3f}", str(mp4))
    (mp4.parent / f"{spec['id']}.{lang}.srt").write_text("\n".join(srt))
    return mp4


def fits(recorded, wanted, window):
    """Exit unless narration of `wanted` slot lengths fits a recording made for
    `recorded` ones: each segment must have as many and no longer slots, or a
    sentence would run over the next or past the movie's end."""
    if len(recorded) != len(wanted):
        sys.exit(f"{window}: recorded for {len(recorded)} segments, the feature has "
                 f"{len(wanted)}; record again (drop --skip-record)")
    for i, (old, new) in enumerate(zip(recorded, wanted)):
        if new > old + 0.05:
            sys.exit(f"{window}: segment {i + 1} now needs {new:.1f}s but was recorded "
                     f"with {old:.1f}s; record again (drop --skip-record)")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("feature", type=Path)
    p.add_argument("--machine", default="agent-test")
    p.add_argument("--out", type=Path, default=Path("target/screencasts"))
    p.add_argument("--langs", default=",".join(LANGS))
    p.add_argument("--window", default=",".join(WINDOWS),
                   help=f"comma-separated windows to record: {', '.join(WINDOWS)}")
    p.add_argument("--tts", default="elevenlabs:river",
                   help=f"comma-separated voices to render: {', '.join(tts.PROVIDERS)}")
    p.add_argument("--crop", type=float,
                   help="fraction of the guest screen's height to keep, from the top "
                        "(default: the window's own)")
    p.add_argument("--captions", action="store_true",
                   help="burn the narration in as captions (the .srt is always written)")
    p.add_argument("--skip-record", action="store_true",
                   help="re-render from the last recording")
    a = p.parse_args()

    spec = json.loads(a.feature.read_text())
    langs = a.langs.split(",")
    work = a.out / "work" / spec["id"]
    work.mkdir(parents=True, exist_ok=True)

    providers = a.tts.split(",")
    missing = [p for p in providers if not tts.available(p)]
    if missing:
        sys.exit(f"no API key for {', '.join(missing)} (OPENAI_API_KEY, ELEVENLABS_API_KEY)")
    longest = speak(spec, providers, langs, work, a.out / "tts-cache")
    slots = [d + GAP for d in longest]
    windows = a.window.split(",")
    unknown = [w for w in windows if w not in WINDOWS]
    if unknown:
        sys.exit(f"unknown window {', '.join(unknown)}: {', '.join(WINDOWS)}")
    for window in windows:
        if a.skip_record:
            saved = json.loads((work / window / "timing.json").read_text())
            fits(saved["slots"], slots, window)
            starts = saved["starts"]
        else:
            launch(a.machine, window, work / window, spec.get("fill", True))
            starts = record(spec, a.machine, slots, work / window)
        print(window, "segments start at", ", ".join(f"{s:.1f}s" for s in starts))
        for provider in providers:
            for lang in langs:
                print(render(spec, provider, lang, starts, work, window, a.out,
                             a.captions, a.crop))


if __name__ == "__main__":
    if not shutil.which("ffmpeg") or not shutil.which("magick"):
        sys.exit("needs ffmpeg and ImageMagick")
    main()
