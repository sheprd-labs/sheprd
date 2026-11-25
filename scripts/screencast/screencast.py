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
import transcript  # noqa: E402
import tts  # noqa: E402

# Narration exists for every language in tts.LANGUAGE_NAMES; videos are made
# in these until each other language has a native voice picked.
LANGS = ["en", "fr"]

LATIN_FONT = "/System/Library/Fonts/SFNS.ttf"
CJK_FONT = "/System/Library/Fonts/Hiragino Sans GB.ttc"

WIDTH, HEIGHT = 1920, 1080
# The guest screen refreshes at 60 Hz, which YouTube and X also play at most.
FPS = 60
LEAD_IN = 1.0  # seconds of screen before the first segment
# screencapture can take seconds to start recording, so wait this long before
# the first segment and trim what it caught before LEAD_IN.
WARMUP = 16.0
GAP = 1.0  # silence after each segment's voice, to take in what was shown
TAIL = 1.5  # seconds after the last segment
CHUNK = 45000  # bytes per pulled chunk; base64 of it stays under the exec output cap
APP = "~/Apps/Sheprd.app"
# Moves the pointer in the machine (machine/hover.c, installed by prepare.py).
HOVER = "/Users/admin/.local/bin/screencast-hover"  # where the machine keeps the build being recorded

# Each window the app can open, as the arguments that open it and the share of
# the guest screen's height, from the top, the maximized window fills (the
# rest is desktop). A video is recorded once per window asked for, so people
# can pick the one they use. Rune is to come.
WINDOWS = {
    "moonframe": (["--env", "SHEPRD_MOONFRAME=1"], 1.0),
    "classic": (["--args", "--classic"], 1.0),
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


# The window being recorded, for steps that differ between windows.
WINDOW = None
# Where a click's ring starts against the click's logged time: the click
# command returns some time after the press lands, and the pointer is always
# on its target 0.2 s before pressing, so a little early never precedes it.
RECORDER_LAG = -0.15
# Clicks made while recording, as (monotonic time, x, y) in screenshot pixels,
# so the render can mark where each lands.
CLICKS = []


def act(name, action):
    """Run one step of a feature file in the machine.

    `click` and `hover` take x, y, or a {window: [x, y]} map when the windows lay the
    target out differently; `exec` runs a shell command in the machine as the
    logged-in user, for setup the interface cannot do reliably."""
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
        # ["click", x, y] plus "right" and/or "quiet"; a {window: [x, y]} map
        # stands for x, y where the windows differ.
        point, extra = (args[0][WINDOW], args[1:]) if isinstance(args[0], dict) else (args[:2], args[2:])
        x, y = point
        # Point first and let the app redraw under the pointer: Sheprd
        # Agent presses 8 ms after moving, which a busy machine has not
        # redrawn in, so the press would land on the old hover.
        machine(name, "exec", "--", HOVER, str(x / 2), str(y / 2))
        time.sleep(0.2)
        sent = time.monotonic()
        machine(name, "click", str(x), str(y), *(["--right"] if "right" in extra else []))
        # The press lands while the command runs: take its middle. A "quiet"
        # click only gives a pane the keyboard before a shortcut, which shows
        # nothing to point at, so it gets no ring.
        if "quiet" not in extra:
            CLICKS.append(((sent + time.monotonic()) / 2, x, y))
    elif kind == "hover":
        # Point at something without clicking: a theme previews, a tooltip
        # shows, or the viewer's eye follows the pointer there.
        x, y = args[0][WINDOW] if isinstance(args[0], dict) else args
        machine(name, "exec", "--", HOVER, str(x / 2), str(y / 2))
    elif kind == "exec":
        machine(name, "exec", "--", "sh", "-c", args[0])
    elif kind == "repeat":
        for _ in range(args[0]):
            for inner in args[1]:
                act(name, inner)
    else:
        sys.exit(f"unknown action {kind}")


def launch(name, window, work):
    """Quit Sheprd in the machine and open it in `window`, keeping a screenshot
    of how it came up in work/preflight.png."""
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
    CLICKS.clear()

    def stop():
        # SIGINT makes screencapture finish the movie; only this run's recorder.
        machine(name, "exec", "--", "sh", "-c", f"kill -INT {recorder}; sleep 2")

    try:
        # A sync mark the render finds: Sheprd hides at a known moment, the
        # screen turns to the bright desktop, and it comes back. It falls in
        # the warm-up, which the render cuts.
        time.sleep(WARMUP - 4)
        mark = time.monotonic() - t0
        machine(name, "key", "cmd+h")
        time.sleep(1.2)
        machine(name, "exec", "--", "sh", "-c", f"open {APP}")
        time.sleep(2.8)
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
    # Place the segment starts and clicks on the movie's clock by the sync
    # mark; the movie's end is not reliable, as stopping the recorder drops
    # its last seconds.
    seen = mark_time(raw, mark)
    if seen is None:
        print("no sync mark found; aligning on the movie's end", file=sys.stderr)
        offset = duration(raw) - stopped
    else:
        offset = seen - mark
    starts = [s + offset for s in starts]
    clicks = [[at - t0 + offset, x, y] for at, x, y in CLICKS if at - t0 < stopped]
    (work / "timing.json").write_text(json.dumps({"starts": starts, "slots": slots,
                                                  "clicks": clicks}))
    return starts


def mark_time(movie, around):
    """When the screen first turns bright (Sheprd hidden, the desktop showing)
    in the movie, searching from 4 s before to 8 s after `around` seconds."""
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-ss", f"{max(0.0, around - 4):.2f}", "-t", "12", "-i", str(movie),
         "-vf", "scale=320:-1,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-",
         "-f", "null", "-"], capture_output=True, text=True, check=True).stdout
    frames, at = [], None
    for line in out.splitlines():
        if line.startswith("frame:"):
            at = float(line.split("pts_time:")[1]) + max(0.0, around - 4)
        elif "YAVG=" in line and at is not None:
            frames.append((at, float(line.split("=")[1])))
    if len(frames) < 2:
        return None
    dark = min(y for _, y in frames)
    bright = max(y for _, y in frames)
    if bright - dark < 25:
        return None
    return next(t for t, y in frames if y > (dark + bright) / 2)


def pull(name, remote, local):
    size = int(machine(name, "exec", "--", "stat", "-f", "%z", remote).strip())
    with open(local, "wb") as f:
        for i in range((size + CHUNK - 1) // CHUNK):
            # The agent's connection drops now and then; a chunk is asked again
            # rather than losing a whole take.
            for attempt in range(4):
                try:
                    out = machine(name, "exec", "--", "sh", "-c",
                                  f"dd if={remote} bs={CHUNK} skip={i} count=1 2>/dev/null | base64")
                    break
                except subprocess.CalledProcessError:
                    if attempt == 3:
                        raise
                    time.sleep(2)
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
    """The video's title as a lower third: white text on no box, lifted off
    the screen behind it by a soft shadow rather than a black band."""
    run("magick", "-background", "none", "-fill", "white", "-font", font(lang),
        "-pointsize", "58", f"label:{text}", "-bordercolor", "none", "-border", "40x30",
        "(", "+clone", "-background", "black", "-shadow", "85x10+0+3", ")",
        "+swap", "-background", "none", "-layers", "merge", "+repage",
        "(", "+clone", "-background", "black", "-shadow", "55x26+0+0", ")",
        "+swap", "-background", "none", "-layers", "merge", "+repage", f"PNG32:{path}")


# How a segment's `shortcut` (the other platforms' keys, as the command catalog
# names them: "ctrl-shift-p") is labelled while the narration says the macOS
# one. Windows takes Linux's keys (crates/sheprd-commands/src/shortcuts).
SHORTCUT_PREFIX = {"en": "On Windows and Linux:", "fr": "Sous Windows et Linux :"}
KEY_NAMES = {
    "en": {"ctrl": "Ctrl", "shift": "Shift", "alt": "Alt"},
    "fr": {"ctrl": "Ctrl", "shift": "Maj", "alt": "Alt"},
}


def shortcut_label(keys, lang, path):
    names = KEY_NAMES.get(lang, KEY_NAMES["en"])
    keys = " + ".join(names.get(k, k.upper()) for k in keys.split("-"))
    prefix = SHORTCUT_PREFIX.get(lang, SHORTCUT_PREFIX["en"])
    run("magick", "-background", "none", "-font", font(lang),
        "(", "-fill", "#c8c8d0", "-pointsize", "34", f"label:{prefix}  ", ")",
        "(", "-fill", "white", "-pointsize", "44", f"label:{keys}", ")",
        "-gravity", "center", "+append",
        "-bordercolor", "none", "-border", "34x18",
        "(", "+clone", "-alpha", "off", "-fill", "#15161ce6", "-colorize", "100", ")",
        "+swap", "-compose", "over", "-composite", str(path))


def srt_time(t):
    ms = int(round(t * 1000))
    return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02},{ms % 1000:03}"


# A click shows as a thin ring that grows from the pointer and fades, as
# Screen Studio and similar apps mark clicks: RING_FRAMES images, each shown
# for one frame in turn, from a small opaque ring to a wide faint one.
RING_FRAMES = 14
RING_BOX = 96


def ring_frames(folder):
    paths = []
    for i in range(RING_FRAMES):
        t = i / (RING_FRAMES - 1)
        eased = 1 - (1 - t) ** 3
        radius = 9 + eased * 34
        alpha = (1 - t) ** 1.4
        c = RING_BOX / 2
        path = folder / f"ring{i:02d}.png"
        run("magick", "-size", f"{RING_BOX}x{RING_BOX}", "xc:none", "-fill", "none",
            "-stroke", f"rgba(255,255,255,{alpha * 0.95:.3f})", "-strokewidth", "3",
            "-draw", f"circle {c},{c} {c + radius:.1f},{c}",
            "-stroke", f"rgba(0,0,0,{alpha * 0.35:.3f})", "-strokewidth", "1",
            "-draw", f"circle {c},{c} {c + radius + 2:.1f},{c}", f"PNG32:{path}")
        paths.append(path)
    return paths


def render(spec, provider, lang, starts, work, window, out, captions=False, crop=None,
           clicks=()):
    """Render one language and voice of the recording in work/WINDOW."""
    screen = work / window / "screen.mov"
    crop = crop or WINDOWS[window][1]
    # Keep LEAD_IN of screen before the first segment; the recorder's warm-up
    # before that is cut.
    head = max(0.0, starts[0] - LEAD_IN)
    starts = [start - head for start in starts]
    click_offset = spec.get("click_offset", RECORDER_LAG)
    clicks = [(at - head + click_offset, x, y) for at, x, y in clicks if at - head > 0]
    d = work / window / "render" / slug(provider) / lang
    d.mkdir(parents=True, exist_ok=True)
    title_card(spec["title"][lang], lang, d / "title.png")

    inputs = ["-i", str(screen), "-loop", "1", "-i", str(d / "title.png")]
    filters = [
        # Keep the top `crop` of the guest screen (the window, not the desktop below).
        # screencapture writes a frame only when the screen changes: make the
        # rate constant before cutting the warm-up, and hold the last frame.
        f"[0:v]fps={FPS},trim=start={head:.3f},setpts=PTS-STARTPTS,"
        f"tpad=stop_mode=clone:stop=-1,crop=iw:ih*{crop}:0:0,scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease,"
        f"pad={WIDTH}:{HEIGHT}:(ow-iw)/2:(oh-ih)/2:color=0x101014[v0]",
        f"[v0][1:v]overlay=(W-w)/2:H-h-40:shortest=1:enable='lt(t,{starts[1]:.3f})'[v1]",
    ]
    srt, audio_labels = [], []
    end_time = duration(screen) - head
    last = "v1"
    burned = 0
    for i, seg in enumerate(spec["segments"]):
        length = duration(voice_file(work, provider, lang, i))
        start, end = starts[i], starts[i] + length + GAP
        # A static guest may emit its last frame before the voice finishes.
        end_time = max(end_time, end)
        srt.append(f"{i + 1}\n{srt_time(start)} --> {srt_time(end)}\n{seg['say'][lang]}\n")
        overlays = []
        if captions:
            cap = d / f"cap{i}.png"
            caption(seg["say"][lang], lang, cap, 46, WIDTH - 360)
            overlays.append((cap, "H-h-70"))
        if seg.get("shortcut"):
            # Always shown: viewers on Windows and Linux need their own keys
            # whether or not the narration is captioned.
            key = d / f"key{i}.png"
            shortcut_label(seg["shortcut"], lang, key)
            overlays.append((key, "H-h-190" if captions else "H-h-70"))
        for image, y in overlays:
            inputs += ["-loop", "1", "-i", str(image)]
            n = 2 + burned
            burned += 1
            filters.append(
                f"[{last}][{n}:v]overlay=(W-w)/2:{y}:shortest=1:"
                f"enable='between(t,{start:.3f},{end:.3f})'[v{n}]"
            )
            last = f"v{n}"
    # Each click: the ring frames as one short clip, shifted to the click's
    # time and placed at its point. One overlay per click keeps the render
    # fast. Guest pixels map to the video by the scale the screen was fitted
    # with (full width kept).
    if clicks:
        scale = WIDTH / 2560
        ring_frames(d)
        inputs += ["-framerate", str(FPS / 2), "-i", str(d / "ring%02d.png")]
        ring = 2 + burned
        burned += 1
        filters.append(f"[{ring}:v]format=rgba,split={len(clicks)}" +
                       "".join(f"[r{k}]" for k in range(len(clicks))))
        for k, (at, x, y) in enumerate(clicks):
            ox, oy = x * scale - RING_BOX / 2, y * scale - RING_BOX / 2
            filters.append(f"[r{k}]setpts=PTS-STARTPTS+{at:.3f}/TB[rs{k}]")
            filters.append(f"[{last}][rs{k}]overlay={ox:.0f}:{oy:.0f}:eof_action=pass[c{k}]")
            last = f"c{k}"
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
        "-t", f"{end_time:.3f}", str(mp4))
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
    p.add_argument("--machine", default="screencast")
    p.add_argument("--out", type=Path, default=Path("target/screencasts"))
    p.add_argument("--langs", default=",".join(LANGS))
    # Feature guides use Moonframe by default.
    p.add_argument("--window", default="moonframe",
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

    spec = transcript.load(a.feature)
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
    # A feature may name the windows it can be shown in, such as one whose
    # page only draws in some.
    windows = [w for w in windows if w in spec.get("windows", WINDOWS)]
    if not windows:
        sys.exit(f"{spec['id']} is recorded in {', '.join(spec['windows'])} only")
    global WINDOW
    for window in windows:
        WINDOW = window
        if a.skip_record:
            saved = json.loads((work / window / "timing.json").read_text())
            fits(saved["slots"], slots, window)
            starts = saved["starts"]
        else:
            launch(a.machine, window, work / window)
            starts = record(spec, a.machine, slots, work / window)
        clicks = json.loads((work / window / "timing.json").read_text()).get("clicks", [])
        print(window, "segments start at", ", ".join(f"{s:.1f}s" for s in starts))
        for provider in providers:
            for lang in langs:
                print(render(spec, provider, lang, starts, work, window, a.out,
                             a.captions, a.crop, clicks))


if __name__ == "__main__":
    if not shutil.which("ffmpeg") or not shutil.which("magick"):
        sys.exit("needs ffmpeg and ImageMagick")
    main()
