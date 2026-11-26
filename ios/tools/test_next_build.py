#!/usr/bin/env python3
"""Tests next-build.py build number calculation without network access."""

from datetime import datetime, timedelta, timezone
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parent / "next-build.py"
SPEC = importlib.util.spec_from_file_location("next_build", SCRIPT)
NEXT_BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NEXT_BUILD)


class NextBuildTests(unittest.TestCase):
    def test_advances_to_current_utc_timestamp_past_older_builds(self):
        now = datetime(2026, 11, 26, 8, 30, tzinfo=timezone.utc)
        versions = ["202610291941", "202610291940", "202610291938", "42"]
        self.assertEqual(NEXT_BUILD.next_build(versions, now=now), 202611260830)

    def test_increments_when_existing_build_is_at_or_ahead_of_clock(self):
        now = datetime(2026, 11, 26, 8, 30, tzinfo=timezone.utc)
        self.assertEqual(NEXT_BUILD.next_build(["202611260830"], now=now), 202611260831)
        self.assertEqual(NEXT_BUILD.next_build(["202611260835"], now=now), 202611260836)

    def test_uses_timestamp_when_no_numeric_builds_exist(self):
        now = datetime(2026, 11, 26, 8, 30, tzinfo=timezone.utc)
        self.assertEqual(NEXT_BUILD.next_build([], now=now), 202611260830)
        self.assertEqual(NEXT_BUILD.next_build(["1.0.0", "dev"], now=now), 202611260830)

    def test_converts_non_utc_timezones_to_utc(self):
        pst = timezone(timedelta(hours=-8))
        now = datetime(2026, 11, 26, 0, 30, tzinfo=pst)
        self.assertEqual(NEXT_BUILD.next_build(["202610291941"], now=now), 202611260830)

    def test_rejects_naive_datetimes(self):
        with self.assertRaises(ValueError):
            NEXT_BUILD.next_build([], now=datetime(2026, 11, 26, 8, 30))

    def test_main_prints_next_build_from_app_store_connect(self):
        fixed_now = datetime(2026, 11, 26, 8, 45, tzinfo=timezone.utc)

        class FixedDateTime(datetime):
            @classmethod
            def now(cls, tz=None):
                return fixed_now if tz is None else fixed_now.astimezone(tz)

        builds_response = {
            "data": [
                {"attributes": {"version": "202610291941"}},
                {"attributes": {"version": "202610291940"}},
            ]
        }
        stdout = io.StringIO()
        with (
            mock.patch.object(sys, "argv", ["next-build.py", "so.pen.herdr.remote"]),
            mock.patch.object(NEXT_BUILD, "datetime", FixedDateTime),
            mock.patch.multiple(
                NEXT_BUILD.asc,
                bearer_from_env=lambda: "token",
                app_id=lambda bundle_id, bearer: "app-1",
                get=lambda path, query, bearer: builds_response,
            ),
            mock.patch.object(sys, "stdout", stdout),
        ):
            NEXT_BUILD.main()

        self.assertEqual(stdout.getvalue(), "202611260845\n")


if __name__ == "__main__":
    unittest.main()
