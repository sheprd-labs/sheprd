#!/usr/bin/env python3
"""Verify every bundle ships a privacy manifest that covers what it uses.

Apple flags uploads whose bundles call a required-reason API without one
(App Store Connect, ITMS-91053). Run after `xcodegen generate` in ios/.
Uses only Python's standard library.
"""
from pathlib import Path
import plistlib
import re
import unittest

IOS = Path(__file__).resolve().parents[1]
TARGETS = ("SheprdRemote", "SheprdRemoteWidgets", "SheprdRemoteNotifications",
           "SheprdRemoteShare", "SheprdRemoteWatch")


def manifest(target):
    return plistlib.loads((IOS / target / "PrivacyInfo.xcprivacy").read_bytes())


def reasons(target, category):
    for api in manifest(target)["NSPrivacyAccessedAPITypes"]:
        if api["NSPrivacyAccessedAPIType"] == category:
            return set(api["NSPrivacyAccessedAPITypeReasons"])
    return set()


class PrivacyManifestTests(unittest.TestCase):
    def test_every_target_bundles_its_manifest(self):
        project = (IOS / "SheprdRemote.xcodeproj/project.pbxproj").read_text()
        bundled = len(re.findall(r"PrivacyInfo\.xcprivacy in Resources \*/,", project))
        self.assertEqual(bundled, len(TARGETS))
        for target in TARGETS:
            self.assertFalse(manifest(target)["NSPrivacyTracking"], target)
            self.assertEqual(manifest(target)["NSPrivacyTrackingDomains"], [], target)

    def test_user_defaults_reasons(self):
        # Every target links SheprdRemoteCore, which keeps settings in the App
        # Group's defaults that the app and its extensions share.
        for target in TARGETS:
            self.assertEqual(reasons(target, "NSPrivacyAccessedAPICategoryUserDefaults"),
                             {"CA92.1", "1C8F.1"}, target)

    def test_file_timestamps_are_declared_where_read(self):
        for target in TARGETS:
            sources = "".join(p.read_text() for p in (IOS / target).rglob("*.swift"))
            if "ModificationDate" in sources or "creationDate" in sources:
                self.assertEqual(reasons(target, "NSPrivacyAccessedAPICategoryFileTimestamp"),
                                 {"C617.1"}, target)

    def test_app_discloses_push_and_opt_in_voice_content(self):
        collected = manifest("SheprdRemote")["NSPrivacyCollectedDataTypes"]
        linked = {c["NSPrivacyCollectedDataType"]: c["NSPrivacyCollectedDataTypeLinked"] for c in collected}
        self.assertEqual(linked, {"NSPrivacyCollectedDataTypeDeviceID": False,
                                  "NSPrivacyCollectedDataTypeOtherUserContent": True})
        self.assertTrue(all(not c["NSPrivacyCollectedDataTypeTracking"] for c in collected))
        for target in TARGETS[1:]:
            self.assertEqual(manifest(target)["NSPrivacyCollectedDataTypes"], [], target)


if __name__ == "__main__":
    unittest.main()
