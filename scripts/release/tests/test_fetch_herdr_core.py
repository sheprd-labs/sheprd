"""The bundled herdr-core is pinned by checksum and extracted, with its notices, by name."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("fetch", ROOT / "scripts/release/fetch-herdr-core.py")
fetch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fetch)


def tar_gz(members):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w:gz") as archive:
        for name, body in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(body)
            archive.addfile(info, io.BytesIO(body))
    return data.getvalue()


class FetchHerdrCoreTests(unittest.TestCase):
    def test_every_pinned_target_has_an_https_url_and_a_sha256(self):
        pinned = json.loads((ROOT / "scripts/release/herdr-core.json").read_text())
        for target, entry in pinned["targets"].items():
            self.assertTrue(entry["url"].startswith("https://"), target)
            self.assertIn(pinned["version"], entry["url"], target)
            self.assertRegex(entry["sha256"], "^[0-9a-f]{64}$", target)

    def test_a_checksum_mismatch_fails(self):
        data = b"archive"
        self.assertEqual(fetch.verified(data, hashlib.sha256(data).hexdigest()), data)
        with self.assertRaises(SystemExit):
            fetch.verified(data, "0" * 64)

    def test_the_binary_is_found_by_name_in_either_archive(self):
        self.assertEqual(fetch.executable(tar_gz({"dir/herdr-core": b"elf", "LICENSE": b"x"}), "herdr-core"), b"elf")
        self.assertEqual(
            fetch.executable(
                tar_gz({
                    "herdr-core": b"top-macho",
                    "Herdr Core.app/Contents/MacOS/herdr-core": b"app-macho",
                }),
                "herdr-core",
            ),
            b"top-macho",
        )
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("herdr-core.exe", b"pe")
        self.assertEqual(fetch.executable(data.getvalue(), "herdr-core.exe"), b"pe")
        with self.assertRaises(SystemExit):
            fetch.executable(tar_gz({"other": b"x"}), "herdr-core")

    def test_the_notices_come_from_either_archive_without_page_breaks(self):
        body = (b"hc (herdr-core) is Apache-2.0.\n\nOF CONTRACT\n\n"
                b"[7866 more lines in file. Use offset=401 to continue.]\nIN CONNECTION\n")
        expected = "herdr-core 9.9.9 (THIRD-PARTY-NOTICES from its release archive)\n\n" \
                   "hc (herdr-core) is Apache-2.0.\n\nOF CONTRACT\n\nIN CONNECTION\n"
        tarball = tar_gz({"herdr-core": b"elf", "THIRD-PARTY-NOTICES": body})
        self.assertEqual(fetch.notices(tarball, "9.9.9"), expected)
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("THIRD-PARTY-NOTICES/", b"")
            archive.writestr("THIRD-PARTY-NOTICES/herdr-core.txt", body.replace(b"\n", b"\r\n"))
            archive.writestr("THIRD-PARTY-NOTICES/ConPTY-LICENSE.txt", b"other")
        self.assertEqual(fetch.notices(data.getvalue(), "9.9.9"), expected)

    def test_missing_or_unlicensed_notices_fail(self):
        for members in ({"herdr-core": b"elf"}, {"THIRD-PARTY-NOTICES": b"no license named"},
                        {"docs/THIRD-PARTY-NOTICES": b"Apache-2.0"}):
            with self.subTest(members=list(members)), self.assertRaises(SystemExit):
                fetch.notices(tar_gz(members), "9.9.9")

    def test_an_unknown_target_is_refused(self):
        with self.assertRaises(SystemExit):
            fetch.pinned("windows-aarch64")


class BundledHerdrCoreTests(unittest.TestCase):
    def test_every_release_target_bundles_a_pinned_build(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        targets = json.loads(fetch.PINNED.read_text())["targets"]
        for rust, core in fetch.RUST_TARGETS.items():
            self.assertIn(f"target: {rust}", workflow)
            self.assertIn(core, targets)
            self.assertEqual(fetch.pinned(rust), targets[core])



if __name__ == "__main__":
    unittest.main()
