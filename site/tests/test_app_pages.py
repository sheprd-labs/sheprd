"""The iOS app's privacy and support pages, which App Store Connect links to."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
WEBSITE = ROOT / "website/site"
PAGES = ("privacy", "support")


class AppPagesTests(unittest.TestCase):
    def test_pages_are_canonical_and_share_the_masthead(self):
        masthead = re.search(r"<header class=\"masthead\">.*?</header>",
                             (WEBSITE / "testflight/index.html").read_text(), re.S).group(0)
        for slug in PAGES:
            with self.subTest(page=slug):
                html = (WEBSITE / slug / "index.html").read_text()
                self.assertIn(f'<link rel="canonical" href="https://sheprd.org/{slug}/">', html)
                self.assertIn(masthead, html)
                self.assertEqual(html.count("<h1"), 1)

    def test_privacy_keeps_the_push_relay_promises(self):
        # The same promises as the app's push relay setting (docs/apns-relay-plan.md).
        html = (WEBSITE / "privacy/index.html").read_text()
        for phrase in ("push.sheprd.org", "never stored or logged", "Settings &gt; Notifications &gt; Push Relay",
                       "Last updated", "no accounts, analytics, ads, or tracking"):
            self.assertIn(phrase, html)

    def test_support_offers_the_demo_and_links_privacy(self):
        html = (WEBSITE / "support/index.html").read_text()
        self.assertIn("Explore the Demo", html)
        self.assertIn('href="/privacy/"', html)
        demo = (ROOT / "ios/SheprdRemote/Demo/DemoOffer.swift").read_text()
        self.assertIn('Explore the Demo"', demo)


if __name__ == "__main__":
    unittest.main()
