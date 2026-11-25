#!/usr/bin/env python3
"""Check the App Store listing in AppStore/ against Apple's limits.

Uses only Python's standard library.
"""
from pathlib import Path
import tomllib
import unittest

IOS = Path(__file__).resolve().parents[1]
ROOT = IOS.parent
LISTING = tomllib.loads((IOS / "AppStore/metadata.toml").read_text())
# App Store Connect's limits, in characters.
LIMITS = {"name": 30, "subtitle": 30, "promotional_text": 170, "keywords": 100, "description": 4000}


class AppStoreListingTests(unittest.TestCase):
    def test_fields_fit_apples_limits(self):
        for field, limit in LIMITS.items():
            with self.subTest(field=field):
                self.assertTrue(LISTING[field].strip())
                self.assertLessEqual(len(LISTING[field].strip()), limit)

    def test_keywords_do_not_repeat_the_name_or_name_others(self):
        # Words in the name are indexed already, and other companies' marks
        # get a listing rejected (Guideline 5.2.1).
        keywords = LISTING["keywords"].split(",")
        self.assertEqual(len(keywords), len(set(keywords)))
        self.assertTrue(all(k == k.strip() and k for k in keywords))
        listed = " ".join([LISTING["name"], LISTING["subtitle"], LISTING["keywords"]]).lower()
        for mark in ("claude", "codex", "herdr", "openai", "anthropic", "gemini"):
            self.assertNotIn(mark, listed)
        self.assertNotIn(LISTING["name"].lower(), keywords)

    def test_urls_are_pages_the_website_publishes(self):
        site = ROOT / "website/site"
        for field in ("support_url", "privacy_policy_url", "marketing_url"):
            with self.subTest(field=field):
                url = LISTING[field]
                self.assertTrue(url.startswith("https://sheprd.org/"))
                path = url.removeprefix("https://sheprd.org/")
                self.assertTrue((site / path / "index.html").is_file())

    def test_review_notes_name_the_demo_button(self):
        notes = (IOS / "AppStore" / LISTING["review"]["notes_file"]).read_text()
        demo = (IOS / "SheprdRemote/Demo/DemoOffer.swift").read_text()
        self.assertIn("Explore the Demo", notes)
        self.assertIn('Explore the Demo"', demo)
        self.assertIn("Explore the Demo", LISTING["description"])


if __name__ == "__main__":
    unittest.main()
