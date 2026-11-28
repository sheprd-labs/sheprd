#!/usr/bin/env python3
"""Adds a processed build to every existing external TestFlight group and
submits it for Beta App Review, without which external testers never get it.

Usage: python3 testflight_distribution.py BUNDLE_ID BUILD_NUMBER
"""

import sys
import time
import urllib.error

import app_store_connect as asc

BUILD_TIMEOUT = 30 * 60
POLL_SECONDS = 30
MAX_PAGE_SIZE = 200
RETRYABLE_STATUSES = {404, 409, 429}
FAILED_STATES = {"FAILED", "INVALID"}
# A build's external TestFlight state once it has been submitted: in review,
# or approved and testable. Nothing more to do for these.
SUBMITTED_STATES = {
    "WAITING_FOR_BETA_REVIEW",
    "IN_BETA_REVIEW",
    "IN_EXPORT_COMPLIANCE_REVIEW",
    "BETA_APPROVED",
    "READY_FOR_BETA_TESTING",
    "IN_BETA_TESTING",
}


def retryable(error: urllib.error.URLError) -> bool:
    if isinstance(error, urllib.error.HTTPError):
        return error.code in RETRYABLE_STATUSES or error.code >= 500
    return True


def find_build(app_id: str, build_number: str, bearer: str):
    builds = asc.get(
        "builds",
        {
            "filter[app]": app_id,
            "filter[version]": build_number,
            "fields[builds]": "version,processingState",
            "limit": MAX_PAGE_SIZE,
        },
        bearer,
    )["data"]
    matches = [
        build
        for build in builds
        if build.get("attributes", {}).get("version") == build_number
    ]
    if len(matches) > 1:
        sys.exit(f"testflight-distribution: build number {build_number} is ambiguous")
    return matches[0] if matches else None


def wait_for_build(
    app_id: str,
    build_number: str,
    sleep=time.sleep,
    clock=time.monotonic,
):
    deadline = clock() + BUILD_TIMEOUT
    while True:
        try:
            build = find_build(app_id, build_number, asc.bearer_from_env())
            if build:
                state = build.get("attributes", {}).get("processingState")
                if state == "VALID":
                    return build
                if state in FAILED_STATES:
                    sys.exit(f"testflight-distribution: build {build_number} is {state}")
                if state != "PROCESSING":
                    sys.exit(
                        f"testflight-distribution: build {build_number} "
                        f"has unexpected state {state!r}"
                    )
        except urllib.error.URLError as error:
            if not retryable(error):
                raise
        if clock() >= deadline:
            sys.exit(
                f"testflight-distribution: gave up waiting for build {build_number} to process"
            )
        sleep(POLL_SECONDS)


def external_groups(app_id: str, bearer: str) -> list:
    groups = asc.get_all(
        f"apps/{app_id}/betaGroups",
        {"fields[betaGroups]": "name,isInternalGroup", "limit": MAX_PAGE_SIZE},
        bearer,
    )
    if any(
        not isinstance(group.get("attributes", {}).get("isInternalGroup"), bool)
        for group in groups
    ):
        sys.exit("testflight-distribution: a beta group is missing its internal/external status")
    return [
        group
        for group in groups
        if group["attributes"]["isInternalGroup"] is False
    ]


def external_state(build_id: str, bearer: str):
    detail = asc.get(
        f"builds/{build_id}/buildBetaDetail",
        {"fields[buildBetaDetails]": "externalBuildState"},
        bearer,
    )["data"]
    return detail.get("attributes", {}).get("externalBuildState")


def submit_for_review(build_id: str, build_number: str, sleep=time.sleep, clock=time.monotonic) -> None:
    """Submits the build for Beta App Review once App Store Connect is ready
    for it. Builds of a version Apple has already approved are usually
    approved within minutes."""
    deadline = clock() + BUILD_TIMEOUT
    while True:
        try:
            state = external_state(build_id, asc.bearer_from_env())
        except urllib.error.URLError as error:
            if not retryable(error):
                raise
            state = "PROCESSING"
        if state in SUBMITTED_STATES:
            print(f"Build {build_number} is already submitted ({state}).")
            return
        if state == "READY_FOR_BETA_SUBMISSION":
            break
        if state != "PROCESSING":
            sys.exit(
                f"testflight-distribution: build {build_number} cannot be "
                f"submitted for review: its external state is {state!r}"
            )
        if clock() >= deadline:
            sys.exit(
                f"testflight-distribution: gave up waiting for build {build_number} "
                "to be ready for review"
            )
        sleep(POLL_SECONDS)

    print(f"Submitting build {build_number} for Beta App Review…", flush=True)
    asc.send(
        "POST",
        "betaAppReviewSubmissions",
        {
            "data": {
                "type": "betaAppReviewSubmissions",
                "relationships": {"build": {"data": {"type": "builds", "id": build_id}}},
            }
        },
        asc.bearer_from_env(),
    )


def distribute(bundle_id: str, build_number: str, sleep=time.sleep, clock=time.monotonic) -> None:
    app = asc.app_id(bundle_id, asc.bearer_from_env())
    if app is None:
        sys.exit(f"testflight-distribution: App Store Connect has no app {bundle_id}")

    print(f"Waiting for build {build_number} to finish processing…", flush=True)
    build = wait_for_build(app, build_number, sleep=sleep, clock=clock)
    groups = external_groups(app, asc.bearer_from_env())
    if not groups:
        print("No external TestFlight groups found; no groups were changed.")
        return

    names = ", ".join(group["attributes"].get("name", "(unnamed)") for group in groups)
    print(f"Adding build {build_number} to {len(groups)} external TestFlight groups: {names}")
    asc.send(
        "POST",
        f"builds/{build['id']}/relationships/betaGroups",
        {"data": [{"type": "betaGroups", "id": group["id"]} for group in groups]},
        asc.bearer_from_env(),
    )
    submit_for_review(build["id"], build_number, sleep=sleep, clock=clock)


def main() -> None:
    arguments = sys.argv[1:]
    if len(arguments) != 2:
        sys.exit(__doc__)
    try:
        distribute(arguments[0], arguments[1])
    except KeyError as missing:
        sys.exit(f"testflight-distribution: {missing.args[0]} is not set")
    except urllib.error.HTTPError as error:
        sys.exit(
            f"testflight-distribution: App Store Connect answered "
            f"{error.code}: {error.read()[:500]!r}"
        )
    except (RuntimeError, ValueError) as error:
        sys.exit(f"testflight-distribution: {error}")


if __name__ == "__main__":
    main()
