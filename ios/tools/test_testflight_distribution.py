#!/usr/bin/env python3
"""Tests automatic external-group assignment with a stand-in ASC API."""

import io
import unittest
import urllib.error
from unittest import mock

import testflight_distribution as distribution


def http_error(code):
    return urllib.error.HTTPError("https://example.invalid", code, "error", {}, io.BytesIO(b"{}"))


class FakeApi:
    def __init__(self, builds, groups):
        self.builds = list(builds)
        self.groups = groups
        self.sent = []

    def get(self, path, query, bearer):
        if path == "builds":
            result = self.builds.pop(0) if len(self.builds) > 1 else self.builds[0]
            if isinstance(result, BaseException):
                raise result
            return {"data": result}

    def get_all(self, path, query, bearer):
        self.group_query = (path, query)
        if self.groups.get("links", {}).get("next"):
            raise RuntimeError("App Store Connect returned more than 200 groups")
        return self.groups["data"]

    def send(self, method, path, body, bearer):
        self.sent.append((method, path, body))
        return {}


class DistributionTests(unittest.TestCase):
    def run_distribution(self, api):
        slept = 0

        def fake_sleep(seconds):
            nonlocal slept
            slept += seconds

        def authenticated(request):
            def checked(*args):
                issued_at = args[-1]
                if slept - issued_at >= 600:
                    raise http_error(401)
                return request(*args)
            return checked

        with mock.patch.multiple(
            distribution.asc,
            bearer_from_env=lambda: slept,
            app_id=lambda bundle_id, bearer: "app",
            get=authenticated(api.get),
            get_all=authenticated(api.get_all),
            send=authenticated(api.send),
        ):
            distribution.distribute(
                "bundle", "42", sleep=fake_sleep, clock=lambda: slept
            )
        return slept

    def test_waits_until_processed_then_adds_all_external_groups(self):
        api = FakeApi(
            [
                [{"id": "build", "attributes": {"version": "42", "processingState": "PROCESSING"}}],
                [{"id": "build", "attributes": {"version": "42", "processingState": "VALID"}}],
            ],
            {
                "data": [
                    {
                        "id": "external-a",
                        "attributes": {"name": "Partner A", "isInternalGroup": False},
                    },
                    {"id": "internal", "attributes": {"name": "Staff", "isInternalGroup": True}},
                    {
                        "id": "external-b",
                        "attributes": {"name": "Partner B", "isInternalGroup": False},
                    },
                ]
            },
        )

        slept = self.run_distribution(api)

        self.assertEqual(slept, distribution.POLL_SECONDS)
        self.assertEqual(len(api.sent), 1)
        method, path, body = api.sent[0]
        self.assertEqual(
            (method, path), ("POST", "builds/build/relationships/betaGroups")
        )
        self.assertEqual(
            body,
            {
                "data": [
                    {"type": "betaGroups", "id": "external-a"},
                    {"type": "betaGroups", "id": "external-b"},
                ]
            },
        )
        self.assertEqual(
            api.group_query,
            (
                "apps/app/betaGroups",
                {
                    "fields[betaGroups]": "name,isInternalGroup",
                    "limit": distribution.MAX_PAGE_SIZE,
                },
            ),
        )

    def test_assigns_groups_after_initial_token_expires(self):
        processing = [{"id": "build", "attributes": {"version": "42", "processingState": "PROCESSING"}}]
        valid = [{"id": "build", "attributes": {"version": "42", "processingState": "VALID"}}]
        polls = 600 // distribution.POLL_SECONDS + 1
        api = FakeApi(
            [processing] * polls + [valid],
            {"data": [{"id": "external", "attributes": {"isInternalGroup": False}}]},
        )

        self.assertGreater(self.run_distribution(api), 600)
        self.assertEqual(api.sent[0][2]["data"], [{"type": "betaGroups", "id": "external"}])

    def test_no_external_groups_is_a_successful_no_op(self):
        api = FakeApi(
            [[{"id": "build", "attributes": {"version": "42", "processingState": "VALID"}}]],
            {
                "data": [
                    {"id": "internal", "attributes": {"name": "Staff", "isInternalGroup": True}}
                ]
            },
        )

        self.run_distribution(api)

        self.assertEqual(api.sent, [])

    def test_pagination_refuses_to_silently_skip_groups(self):
        api = FakeApi(
            [[{"id": "build", "attributes": {"version": "42", "processingState": "VALID"}}]],
            {"data": [], "links": {"next": "https://example.invalid/next"}},
        )

        with self.assertRaises(RuntimeError) as stopped:
            self.run_distribution(api)

        self.assertIn("more than", str(stopped.exception))
        self.assertEqual(api.sent, [])

    def test_failed_build_is_not_assigned(self):
        api = FakeApi(
            [[{"id": "build", "attributes": {"version": "42", "processingState": "INVALID"}}]],
            {"data": []},
        )

        with self.assertRaises(SystemExit) as stopped:
            self.run_distribution(api)

        self.assertIn("INVALID", str(stopped.exception))
        self.assertEqual(api.sent, [])

    def test_wait_retries_service_errors(self):
        api = FakeApi(
            [
                http_error(503),
                [{"id": "build", "attributes": {"version": "42", "processingState": "VALID"}}],
            ],
            {"data": []},
        )

        self.assertEqual(self.run_distribution(api), distribution.POLL_SECONDS)


if __name__ == "__main__":
    unittest.main()
