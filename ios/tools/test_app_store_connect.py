#!/usr/bin/env python3
"""Tests App Store Connect response handling with the standard library."""

import json
import unittest
from unittest import mock

import app_store_connect as asc


class Response:
    def __init__(self, body):
        self.body = body

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def read(self):
        return self.body


class SendTests(unittest.TestCase):
    @mock.patch("app_store_connect.urllib.request.urlopen")
    def test_empty_success_response_is_accepted(self, urlopen):
        urlopen.return_value = Response(b"")

        result = asc.send("POST", "builds/b/relationships/betaGroups", {}, "token")

        self.assertEqual(result, {})

    @mock.patch("app_store_connect.urllib.request.urlopen")
    def test_json_success_response_is_decoded(self, urlopen):
        expected = {"data": {"id": "localization"}}
        urlopen.return_value = Response(json.dumps(expected).encode())

        result = asc.send("POST", "betaBuildLocalizations", {}, "token")

        self.assertEqual(result, expected)


class GetAllTests(unittest.TestCase):
    @mock.patch("app_store_connect.urllib.request.urlopen")
    def test_reads_all_pages(self, urlopen):
        urlopen.side_effect = [
            Response(
                json.dumps(
                    {
                        "data": [{"id": "first"}],
                        "links": {"next": f"{asc.API}/apps/app/betaGroups?cursor=next"},
                    }
                ).encode()
            ),
            Response(json.dumps({"data": [{"id": "second"}]}).encode()),
        ]

        result = asc.get_all("apps/app/betaGroups", {"limit": 200}, "token")

        self.assertEqual([item["id"] for item in result], ["first", "second"])
        self.assertEqual(urlopen.call_count, 2)

    @mock.patch("app_store_connect.urllib.request.urlopen")
    def test_refuses_a_pagination_link_to_another_host(self, urlopen):
        urlopen.return_value = Response(
            json.dumps(
                {"data": [], "links": {"next": "https://example.invalid/steal-token"}}
            ).encode()
        )

        with self.assertRaisesRegex(ValueError, "unsafe pagination URL"):
            asc.get_all("apps/app/betaGroups", {"limit": 200}, "token")

        self.assertEqual(urlopen.call_count, 1)


if __name__ == "__main__":
    unittest.main()
