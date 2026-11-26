"""A feature's narration, kept apart from its steps so it is easy to edit.

features/NAME.json says what happens on screen; features/NAME.md says what is
said over it, one numbered section per segment of the JSON, in order:

    # Title in English
    fr: Titre en français

    ## 1
    en: The first sentence.
    fr: La première phrase.

The title's own line is English; other languages follow as `lang: text`
lines, as do each segment's sentences. A segment's notes for whoever edits
it can follow as plain lines and are ignored.
"""

import json
import re
from pathlib import Path

LINE = re.compile(r"^([a-z]{2}(?:-[A-Z]{2})?):\s*(.+)$")


class Error(ValueError):
    pass


def parse(text, path="transcript"):
    title, segments, current = {}, [], None
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if line.startswith("## "):
            current = {}
            segments.append(current)
        elif line.startswith("# "):
            title["en"] = line[2:].strip()
            current = title
        elif current is not None and (match := LINE.match(line)):
            current[match.group(1)] = match.group(2).strip()
        elif line and current is None:
            raise Error(f"{path}:{number}: text before the title")
    if "en" not in title:
        raise Error(f"{path}: no '# Title' line")
    for i, say in enumerate(segments, 1):
        if "en" not in say:
            raise Error(f"{path}: segment {i} has no 'en:' line")
    return title, segments


def render(spec):
    """The Markdown for a spec whose narration is still in its JSON."""
    lines = [f"# {spec['title']['en']}"]
    lines += [f"{k}: {v}" for k, v in spec["title"].items() if k != "en"]
    for i, seg in enumerate(spec["segments"], 1):
        lines += ["", f"## {i}"] + [f"{k}: {v}" for k, v in seg["say"].items()]
    return "\n".join(lines) + "\n"


def load(path):
    """features/NAME.json with its narration from features/NAME.md."""
    path = Path(path)
    spec = json.loads(path.read_text())
    md = path.with_suffix(".md")
    if not md.exists():
        return spec
    title, segments = parse(md.read_text(), md)
    if len(segments) != len(spec["segments"]):
        raise Error(f"{md}: {len(segments)} segments, but {path.name} has "
                    f"{len(spec['segments'])}")
    spec["title"] = title
    for seg, say in zip(spec["segments"], segments):
        seg["say"] = say
    return spec
