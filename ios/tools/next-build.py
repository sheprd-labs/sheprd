#!/usr/bin/env python3
"""Prints the next build number for an app: the UTC timestamp YYYYMMDDHHMM,
or one more than the highest build App Store Connect already has across
every version if that is at or ahead of the clock.

Usage: python3 next-build.py BUNDLE_ID
"""

from datetime import datetime, timezone
import sys

import app_store_connect as asc

# Builds per page; the highest number is among the newest uploads, and
# one page of them covers far more than a burst of parallel uploads.
PAGE = 200


def highest_build(versions: list) -> int:
    """The highest numeric build among `versions`; others are skipped."""
    numbers = [int(version) for version in versions if version.isdigit()]
    return max(numbers, default=0)


def next_build(versions: list, now=None) -> int:
    """UTC YYYYMMDDHHMM, or `highest_build(versions) + 1` if higher."""
    current = datetime.now(timezone.utc) if now is None else now
    if current.tzinfo is None:
        raise ValueError("now must be timezone-aware")
    timestamp = int(current.astimezone(timezone.utc).strftime("%Y%m%d%H%M"))
    return max(timestamp, highest_build(versions) + 1)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit("Usage: python3 next-build.py BUNDLE_ID")
    bundle_id = sys.argv[1]
    try:
        bearer = asc.bearer_from_env()
    except KeyError as missing:
        sys.exit(f"next-build: {missing.args[0]} is not set")
    app = asc.app_id(bundle_id, bearer)
    if app is None:
        sys.exit(f"next-build: App Store Connect has no app {bundle_id}")
    builds = asc.get(
        "builds",
        {
            "filter[app]": app,
            "sort": "-uploadedDate",
            "fields[builds]": "version",
            "limit": PAGE,
        },
        bearer,
    )
    print(next_build([item["attributes"]["version"] for item in builds["data"]]))


if __name__ == "__main__":
    main()
