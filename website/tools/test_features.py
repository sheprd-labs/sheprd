"""Tests for website/tools/features.py's pages. Needs only Python.

Run: python3 website/tools/test_features.py
"""

import importlib.util
import json
import re
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("features", HERE / "features.py")
features = importlib.util.module_from_spec(spec)
sys.modules["features"] = features
spec.loader.exec_module(features)


def entry(fid, category):
    return {
        "id": fid, "category": category, "kicker": f"{fid} kicker",
        "headline": "Line one<br>line two", "dek": f"{fid} dek",
        "description": f"{fid} description", "chapters": [[1, "Start"], [9, "Past the end"]],
        "how": [["Step", "Do it."]], "docs": ["/docs/x/", "Docs"], "keywords": "special",
    }


def narration(fid):
    return {"id": fid, "title": {"en": f"{fid} title", "fr": f"{fid} titre"},
            "segments": [{"say": {"en": f"{fid} first.", "fr": f"{fid} premier thème."}},
                         {"say": {"en": f"{fid} second.", "fr": f"{fid} deuxième."}}]}


def transcript_md(fid):
    n = narration(fid)
    lines = [f"# {n['title']['en']}", f"fr: {n['title']['fr']}"]
    for i, seg in enumerate(n["segments"], 1):
        lines += ["", f"## {i}", f"en: {seg['say']['en']}", f"fr: {seg['say']['fr']}"]
    return "\n".join(lines) + "\n"


class Build(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        self.site = root / "site"
        clips = self.site / "assets/feature-clips"
        specs = root / "specs"
        clips.mkdir(parents=True)
        specs.mkdir()
        catalog = [entry("alpha", "Agents"), entry("beta", "Review"), entry("gamma", "Agents")]
        (root / "features.json").write_text(json.dumps({"features": catalog}))
        manifest = {
            "alpha": {"windows": ["moonframe", "classic"], "langs": ["en", "fr"],
                      "length": 24.4, "starts": [1.0, 5.25]},
            "beta": {"windows": ["classic"], "langs": ["en", "fr"],
                     "length": 70.0, "starts": [1.0, 4.0]},
            # gamma has no clip yet: it must be left out, not break the build.
        }
        (clips / "manifest.json").write_text(json.dumps(manifest))
        for fid in ("alpha", "beta", "gamma"):
            (specs / f"{fid}.json").write_text(json.dumps({"id": fid, "segments": [{}, {}]}))
            (specs / f"{fid}.md").write_text(transcript_md(fid))
        for name, value in {"SITE": self.site, "CLIPS": clips, "CATALOG": root / "features.json",
                            "SPECS": specs}.items():
            patch = unittest.mock.patch.object(features, name, value)
            patch.start()
            self.addCleanup(patch.stop)
        features.build()

    def read(self, path):
        return (self.site / "features" / path).read_text()

    def test_pages_only_for_features_with_clips(self):
        self.assertTrue((self.site / "features/alpha/index.html").exists())
        self.assertTrue((self.site / "features/beta/index.html").exists())
        self.assertFalse((self.site / "features/gamma").exists())

    def test_index_searches_every_language_and_keyword(self):
        index = self.read("index.html")
        data = json.loads(re.search(r'id="feature-index">(.*?)</script>', index, re.S).group(1))
        alpha = next(e for e in data if e["id"] == "alpha")
        self.assertIn("alpha premier thème.", alpha["text"])
        self.assertIn("alpha titre", alpha["text"])
        self.assertIn("alpha description", alpha["text"])
        self.assertIn("special", alpha["text"])
        self.assertIn("Line one line two", alpha["text"])
        self.assertIn('data-category="Agents"', index)
        self.assertIn("2 of 2 features", re.sub(r"<[^>]+>", "", index))

    def test_publish_rejects_partial_window_before_copying(self):
        renders = self.site.parent / "renders"
        for window, languages in (("moonframe", ("en", "fr")), ("classic", ("en",))):
            folder = renders / window / features.VOICE
            folder.mkdir(parents=True)
            for lang in languages:
                (folder / f"alpha.{lang}.mp4").touch()
                (folder / f"alpha.{lang}.srt").touch()
        with (unittest.mock.patch.object(features, "RENDERS", renders),
              unittest.mock.patch.object(features, "WINDOW_ORDER", ["moonframe", "classic"]),
              self.assertRaisesRegex(ValueError, "same narration languages")):
            features.publish()
        self.assertFalse(list(features.CLIPS.glob("*.mp4")))

    def test_publish_without_local_renders_keeps_existing_guides(self):
        manifest = features.CLIPS / "manifest.json"
        previous = json.loads(manifest.read_text())
        with unittest.mock.patch.object(features, "RENDERS", self.site.parent / "empty"):
            features.publish()
        self.assertEqual(json.loads(manifest.read_text()), previous)
        features.build()
        self.assertIn('href="/features/alpha/"', self.read("index.html"))
        self.assertIn('href="/features/beta/"', self.read("alpha/index.html"))

    def test_feature_page_chapters_switches_and_pager(self):
        alpha = self.read("alpha/index.html")
        self.assertIn('data-seek="5.25"', alpha)
        self.assertNotIn("Past the end", alpha)
        self.assertIn('data-window="classic"', alpha)
        self.assertIn('data-lang="fr"', alpha)
        self.assertIn('href="/features/beta/"', alpha)
        beta = self.read("beta/index.html")
        # One window: no switch, the window is named instead.
        self.assertNotIn('data-window="moonframe"', beta)
        self.assertIn("Classic window", beta)
        self.assertIn("1:10", self.read("index.html"))
        self.assertIn('rel="next" href="/features/alpha/"', beta)

    def test_transcript_must_match_the_steps(self):
        bad = self.site.parent / "specs/beta.md"
        bad.write_text("# Beta\n\n## 1\nen: Only one.\n")
        with self.assertRaises(features.transcript.Error):
            features.spec("beta")

    def test_text_is_escaped(self):
        self.assertEqual(features.esc('<a & "b">'), "&lt;a &amp; &quot;b&quot;&gt;")


if __name__ == "__main__":
    unittest.main()
