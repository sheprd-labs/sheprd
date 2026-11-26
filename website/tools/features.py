#!/usr/bin/env python3
"""Build sheprd.org/features: one page per feature screencast and a searchable index.

  python3 website/tools/features.py publish   # copy the rendered clips in (needs ffmpeg)
  python3 website/tools/features.py build     # write the pages from what is published
  python3 website/tools/features.py posters   # re-pick stills after editing `poster`

`publish` takes each feature's MP4s and subtitles from target/screencasts (what
scripts/screencast/screencast.py renders), writes WebVTT captions and a poster,
and records each clip's windows, languages, length and segment times in
site/assets/feature-clips/manifest.json. `build` needs no tools beyond Python:
it reads website/features.json (page copy and order), the manifest, and each
feature's narration in scripts/screencast/features, and writes site/features.
"""

import html
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "website/site"
CLIPS = SITE / "assets/feature-clips"
CATALOG = ROOT / "website/features.json"
SPECS = ROOT / "scripts/screencast/features"
TEMPLATE = Path(__file__).with_name("feature-page.html")
RENDERS = ROOT / "target/screencasts"
VOICE = "elevenlabs-river"
# The window a page plays first: the one Sheprd opens by default.
WINDOW_ORDER = ["moonframe"]
WINDOW_LABELS = {"moonframe": "Moonframe", "classic": "Classic"}
LANG_LABELS = {"en": "English", "fr": "Français"}


def catalog():
    return json.loads(CATALOG.read_text())["features"]


def tour():
    """The basics video the index opens with, if the catalog names one."""
    return json.loads(CATALOG.read_text()).get("tour")


sys.path.insert(0, str(ROOT / "scripts/screencast"))
import transcript  # noqa: E402


def spec(feature_id):
    return transcript.load(SPECS / f"{feature_id}.json")


def seconds(stamp):
    h, m, s = stamp.replace(",", ".").split(":")
    return int(h) * 3600 + int(m) * 60 + float(s)


def srt_starts(text):
    return [seconds(m) for m in re.findall(r"^(\S+) --> ", text, re.M)]


def probe(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        check=True, capture_output=True, text=True).stdout
    return float(out.strip())


def poster(fid, window, at, starts):
    """The still a page shows before its video plays: the frame at `at`
    seconds (the catalog's `poster`), else one into the third segment."""
    if at is None:
        at = starts[min(2, len(starts) - 1)] + 1.5
    first = CLIPS / f"{fid}.{window}.en.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{at:.2f}", "-i", str(first),
                    "-frames:v", "1", "-vf", "scale=1600:-1", "-q:v", "3",
                    str(CLIPS / f"{fid}.jpg")], check=True)


def posters():
    """Pick each published video's still again, after changing `poster` in
    website/features.json, without copying any clip."""
    manifest = json.loads((CLIPS / "manifest.json").read_text())
    for feature in catalog() + ([tour()] if tour() else []):
        clip = manifest.get(feature["id"])
        if clip:
            poster(feature["id"], clip["windows"][0], feature.get("poster"), clip["starts"])
            print(f"poster {feature['id']}: {feature.get('poster', 'default')}")


def rendered_clips(fid):
    """Validate the window/language choices before replacing published assets."""
    by_window = {}
    for window in WINDOW_ORDER:
        renders = RENDERS / window / VOICE
        clips = {p.name[len(fid) + 1:-4]: p for p in sorted(renders.glob(f"{fid}.*.mp4"))
                 if not p.name.endswith(".captioned.mp4")}
        if not clips:
            continue
        if "en" not in clips:
            raise ValueError(f"{fid}/{window}: the default English clip is missing")
        for lang, mp4 in clips.items():
            if lang not in LANG_LABELS:
                raise ValueError(f"{mp4}: unknown narration language {lang}")
            if not mp4.with_suffix(".srt").is_file():
                raise ValueError(f"{mp4}: missing subtitles")
        if by_window and clips.keys() != next(iter(by_window.values())).keys():
            raise ValueError(f"{fid}: all published windows need the same narration languages")
        by_window[window] = clips
    return by_window


def publish():
    """Copy every rendered clip of the catalog's features into the site."""
    features = catalog() + ([tour()] if tour() else [])
    rendered = {f["id"]: rendered_clips(f["id"]) for f in features}
    CLIPS.mkdir(parents=True, exist_ok=True)
    manifest_path = CLIPS / "manifest.json"
    previous = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    manifest = {}
    for feature in features:
        fid = feature["id"]
        clips = rendered[fid]
        if not clips:
            if fid in previous:
                manifest[fid] = previous[fid]
                print(f"kept published {fid}: no local renders")
            else:
                print(f"skipped {fid}: no clips in {RENDERS}", file=sys.stderr)
            continue
        windows = list(clips)
        starts, length = None, 0.0
        for window, variants in clips.items():
            for lang, mp4 in variants.items():
                shutil.copyfile(mp4, CLIPS / f"{fid}.{window}.{lang}.mp4")
                srt = mp4.with_suffix(".srt").read_text()
                vtt = "WEBVTT\n\n" + re.sub(r"(\d\d:\d\d:\d\d),(\d\d\d)", r"\1.\2", srt)
                (CLIPS / f"{fid}.{window}.{lang}.vtt").write_text(vtt)
                if starts is None and lang == "en":
                    starts = srt_starts(srt)
                    length = probe(mp4)
        poster(fid, windows[0], feature.get("poster"), starts)
        manifest[fid] = {"windows": windows, "langs": [lang for lang in LANG_LABELS if lang in clips[windows[0]]],
                         "length": round(length, 1), "starts": [round(s, 2) for s in starts]}
        print(f"published {fid}: {', '.join(windows)}, {length:.0f}s")
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")


def esc(text):
    return html.escape(text, quote=True)


def plain(text):
    return re.sub(r"<[^>]+>", " ", text).replace("  ", " ").strip()


def clock(t):
    return f"{int(t // 60)}:{int(t % 60):02d}"


def page(title, description, path, scripts, main):
    shell = TEMPLATE.read_text()
    for key, value in (("{title}", esc(title)), ("{description}", esc(description)),
                       ("{path}", path),
                       ("{scripts}", "".join(f'<script src="/assets/{name}" defer></script>'
                                             for name in scripts)),
                       ("{main}", main)):
        shell = shell.replace(key, value)
    return shell


def player(fid, label, clip):
    """A demo video with its window and language switches (feature-demo.js)."""
    base = f"/assets/feature-clips/{fid}"
    window, lang = clip["windows"][0], "en"
    switches = "".join(
        f'<span class="demo-langs" role="group" aria-label="{group}">' + "".join(
            f'<button type="button" data-{key}="{v}" aria-pressed="{str(v == first).lower()}"'
            + (f' lang="{v}"' if key == "lang" else "") + f">{names[v]}</button>" for v in values)
        + "</span>"
        for key, group, values, first, names in (
            ("window", "Window", clip["windows"], window, WINDOW_LABELS),
            ("lang", "Narration language", clip["langs"], lang, LANG_LABELS))
        if len(values) > 1)
    return (f'<video id="feature-demo" controls playsinline preload="metadata" poster="{base}.jpg" '
            f'data-clips="{base}" data-window="{window}" aria-label="{esc(label)}, narrated demonstration">\n'
            f'<source src="{base}.{window}.{lang}.mp4" type="video/mp4"><track kind="captions" '
            f'src="{base}.{window}.{lang}.vtt" srclang="{lang}" label="{LANG_LABELS[lang]}">\n'
            f'<a href="{base}.{window}.{lang}.mp4">Download the demo</a>.</video>', switches)


def feature_page(feature, index, features, clip):
    fid = feature["id"]
    narration = spec(fid)
    total = len(features)
    base = f"/assets/feature-clips/{fid}"
    window, lang = clip["windows"][0], "en"
    switches = "".join(
        f'<span class="demo-langs" role="group" aria-label="{label}">' + "".join(
            f'<button type="button" data-{key}="{v}" aria-pressed="{str(v == first).lower()}"'
            + (f' lang="{v}"' if key == "lang" else "") + f">{names[v]}</button>" for v in values)
        + "</span>"
        for key, label, values, first, names in (
            ("window", "Window", clip["windows"], window, WINDOW_LABELS),
            ("lang", "Narration language", clip["langs"], lang, LANG_LABELS))
        if len(values) > 1)
    only = ("" if len(clip["windows"]) > 1 or clip["windows"] == ["moonframe"] else
            f'<span class="demo-only">{WINDOW_LABELS[window]} window only'
            + (f" · {esc(feature['window_note'])}" if feature.get("window_note") else "")
            + "</span>")
    chapters = "".join(
        f'<button type="button" data-seek="{clip["starts"][seg]:.2f}"><span>{n:02d}</span> '
        f'{esc(label)} <time>{clock(clip["starts"][seg])}</time></button>'
        for n, (seg, label) in enumerate(feature["chapters"], 1) if seg < len(clip["starts"]))
    steps = "".join(f"<li><h3>{esc(h)}</h3><p>{esc(p)}</p></li>" for h, p in feature["how"])
    transcript = "".join(f"<p>{esc(s['say']['en'])}</p>" for s in narration["segments"])
    prev_f, next_f = features[index - 1], features[(index + 1) % total]
    main = f'''<div class="feature-breadcrumb"><a href="/features/">← All features</a><span>{index + 1:02d} / {esc(feature["category"])}</span></div>
<section class="feature-title"><div><p class="kicker">{esc(feature["kicker"])}</p><h1>{feature["headline"]}</h1></div><p class="feature-dek">{esc(feature["dek"])}</p></section>
<figure class="demo"><video id="feature-demo" controls playsinline preload="metadata" poster="{base}.jpg" data-clips="{base}" data-window="{window}" aria-label="{esc(feature["kicker"])}, narrated demonstration">
<source src="{base}.{window}.{lang}.mp4" type="video/mp4"><track kind="captions" src="{base}.{window}.{lang}.vtt" srclang="{lang}" label="{LANG_LABELS[lang]}">
<a href="{base}.{window}.{lang}.mp4">Download the demo</a>.</video>
<figcaption><span class="demo-switches">{switches}</span>{only}<span>{round(clip["length"])} seconds · narration &amp; captions</span></figcaption></figure>
<nav class="demo-chapters" aria-label="Video chapters">{chapters}</nav>
<section class="feature-how"><div><p class="kicker">Try it yourself</p><h2>{esc(narration["title"]["en"])}</h2><a class="text-link" href="{feature["docs"][0]}">{esc(feature["docs"][1])} ↗</a></div><ol>{steps}</ol></section>
<details class="demo-transcript"><summary>Read the video transcript</summary>{transcript}</details>
<nav class="feature-pager" aria-label="More features"><a rel="prev" href="/features/{prev_f["id"]}/"><span>← Previous</span>{esc(prev_f["kicker"])}</a><a rel="next" href="/features/{next_f["id"]}/"><span>Next →</span>{esc(next_f["kicker"])}</a></nav>'''
    return page(f'{feature["kicker"]} · Sheprd', feature["description"],
                f"/features/{fid}/", ["feature-demo.js"], main)


def index_page(features, manifest):
    entries, cards = [], []
    categories = list(dict.fromkeys(f["category"] for f in features))
    for n, feature in enumerate(features, 1):
        fid, clip = feature["id"], manifest[feature["id"]]
        narration = spec(fid)
        words = [feature["kicker"], feature["category"], plain(feature["headline"]),
                 feature["dek"], feature["description"], *narration["title"].values(),
                 feature.get("keywords", "")]
        words += [h + " " + p for h, p in feature["how"]]
        words += [text for s in narration["segments"] for text in s["say"].values()]
        entries.append({"id": fid, "category": feature["category"], "text": " ".join(words)})
        cards.append(f'''<li class="feature-card" data-id="{fid}" data-category="{esc(feature["category"])}"><a href="/features/{fid}/">
<span class="card-media"><img src="/assets/feature-clips/{fid}.jpg" width="1600" height="900" loading="lazy" alt=""><span class="card-time">▶ {clock(clip["length"])}</span></span>
<span class="card-meta">{n:02d} / {esc(feature["category"])}</span>
<span class="card-title">{esc(feature["kicker"])}</span>
<span class="card-dek">{esc(feature["dek"])}</span></a></li>''')
    chips = "".join(f'<button type="button" data-category="{esc(c)}" aria-pressed="false">{esc(c)}</button>'
                    for c in categories)
    data = json.dumps(entries, ensure_ascii=False).replace("</", "<\\/")
    basics = ""
    if tour() and "tour" in manifest:
        clip = manifest["tour"]
        video, switches = player("tour", "A tour of Sheprd", clip)
        basics = f'''<figure class="demo tour-demo">{video}
<figcaption><span class="demo-switches">{switches}</span><span>New to Sheprd? Start here · {round(clip["length"])} seconds</span></figcaption></figure>'''
    main = f'''<section class="feature-intro">
<p class="kicker">The field guide</p>
<h1>A little tour.<br>A lot you can do.</h1>
<p class="feature-dek">Short narrated demos of what Sheprd does, each under half a minute,<br class="desktop-break"> in English and French, in the window you use.</p>
</section>
{basics}
<search class="feature-search" role="search">
<label for="feature-query" class="sr-only">Search the features</label>
<input id="feature-query" type="search" placeholder="Search features, e.g. review, theme, agent…" autocomplete="off" spellcheck="false">
<div class="feature-filters" role="group" aria-label="Category"><button type="button" data-category="" aria-pressed="true">All</button>{chips}</div>
<p class="feature-count" aria-live="polite"><span id="feature-count">{len(features)}</span> of {len(features)} features</p>
</search>
<ul class="feature-grid" id="feature-grid">{"".join(cards)}</ul>
<p class="feature-empty" id="feature-empty" hidden>Nothing matches yet. Try another word, or <a href="/docs/">search the documentation</a>.</p>
<script type="application/json" id="feature-index">{data}</script>'''
    return page("Features · Sheprd", "Short narrated demos of what Sheprd does: agents, review, navigation and more.",
                "/features/", ["feature-demo.js", "feature-search.js"], main)


def build():
    manifest = json.loads((CLIPS / "manifest.json").read_text())
    features = [f for f in catalog() if f["id"] in manifest]
    out = SITE / "features"
    for i, feature in enumerate(features):
        target = out / feature["id"] / "index.html"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(feature_page(feature, i, features, manifest[feature["id"]]))
    (out / "index.html").write_text(index_page(features, manifest))
    print(f"wrote {len(features)} feature pages and the index to {out}")


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else "build"
    {"publish": publish, "build": build, "posters": posters}.get(command, lambda: sys.exit(__doc__))()
