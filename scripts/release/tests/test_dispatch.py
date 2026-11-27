"""dispatch.sh creates or reuses release/<sha> and dispatches CI and the release there."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[3]
DISPATCH = ROOT / "scripts/release/dispatch.sh"
REPO = "sheprd-labs/sheprd"
REQUEST_ID = "0" * 32

# A gh stand-in that parses its arguments as strictly as gh does: --jq takes
# exactly one filter, and any argument it does not expect fails the call.
FAKE_GH = textwrap.dedent("""\
    import json, os, subprocess, sys
    args = sys.argv[1:]
    with open(os.environ["MOCK_LOG"], "a") as log:
        log.write(json.dumps(args) + "\\n")
    sha, repo = os.environ["MOCK_SHA"], os.environ["MOCK_REPO"]

    def options(rest, flags, switches=()):
        found, index = {}, 0
        while index < len(rest):
            name = rest[index]
            if name in switches:
                found[name] = True
                index += 1
            elif name in flags and index + 1 < len(rest):
                found.setdefault(name, []).append(rest[index + 1])
                index += 2
            else:
                sys.exit(f"gh: unexpected argument {name!r} in {args}")
        return found

    def emit(value, found):
        text = json.dumps(value)
        if "--jq" in found:
            text = subprocess.run(["jq", "-r", found["--jq"][0]], input=text, text=True,
                                  capture_output=True, check=True).stdout
        sys.stdout.write(text if text.endswith("\\n") else text + "\\n")

    if args[0] == "api":
        endpoint, found = args[1], options(args[2:], ("--jq", "-f"), ("--silent",))
        responses = {
            "user": {"login": "penso"},
            f"repos/{repo}/git/ref/heads/main": {"object": {"sha": sha}},
            f"repos/{repo}/git/matching-refs/heads/release/{sha}":
                json.loads(os.environ["MOCK_REFS"]),
            f"repos/{repo}/git/refs": {},
            f"repos/{repo}/releases/latest": {"tag_name": "v20261126.1"},
        }
        if endpoint not in responses:
            sys.exit(f"gh: unexpected endpoint {endpoint}")
        if "--silent" not in found:
            emit(responses[endpoint], found)
    elif args[:2] == ["workflow", "run"]:
        options(args[3:], ("--repo", "--ref", "-f"))
    elif args[:2] == ["run", "list"]:
        found = options(args[2:], ("--repo", "--workflow", "--branch", "--event", "--limit", "--json"))
        title = (f"Release stable @ {sha} [{os.environ['MOCK_REQUEST_ID']}]"
                 if found["--workflow"] == ["release.yml"]
                 else f"Promote v20261126.1 [{os.environ['MOCK_REQUEST_ID']}]")
        emit([{"databaseId": 7, "displayTitle": title}], found)
    elif args[:2] == ["run", "watch"]:
        options(args[3:], ("--repo",), ("--exit-status",))
    elif args[:2] == ["run", "view"]:
        emit({"conclusion": "success"}, options(args[3:], ("--repo", "--json", "--jq")))
    else:
        sys.exit(f"gh: unexpected command {args}")
""")


@unittest.skipUnless(shutil.which("jq") and shutil.which("git"), "requires jq and git")
class Dispatch(unittest.TestCase):
    def setUp(self):
        self.home = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.repo = self.home / "repo"
        self.repo.mkdir()
        env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"}
        for command in (["init", "-q", "-b", "main"],
                        ["-c", "user.name=t", "-c", "user.email=t@example.com",
                         "commit", "-q", "--allow-empty", "-m", "init"],
                        ["remote", "add", "origin", f"https://github.com/{REPO}.git"]):
            subprocess.run(["git", *command], cwd=self.repo, env=env, check=True)
        self.sha = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.repo, env=env,
                                  capture_output=True, text=True, check=True).stdout.strip()
        bin_dir = self.home / "bin"
        bin_dir.mkdir()
        (bin_dir / "gh.py").write_text(FAKE_GH)
        tools = {"gh": f'exec "{sys.executable}" "{bin_dir / "gh.py"}" "$@"',
                 "openssl": f"printf '%s\\n' {REQUEST_ID}"}
        for name, body in tools.items():
            (bin_dir / name).write_text(f"#!/bin/sh\n{body}\n")
            (bin_dir / name).chmod(0o755)
        self.log = self.home / "gh.log"
        self.env = {**env, "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                    "MOCK_LOG": str(self.log), "MOCK_SHA": self.sha, "MOCK_REPO": REPO,
                    "MOCK_REQUEST_ID": REQUEST_ID}

    def dispatch(self, refs, *args):
        self.log.unlink(missing_ok=True)
        result = subprocess.run(["bash", str(DISPATCH), *args], cwd=self.repo,
                                env={**self.env, "MOCK_REFS": json.dumps(refs)},
                                capture_output=True, text=True)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        return result, calls

    def ref(self, name, sha):
        return {"ref": f"refs/heads/{name}", "object": {"sha": sha}}

    def test_creates_the_release_branch_and_dispatches_on_it(self):
        branch = f"release/{self.sha}"
        # A longer name sharing the prefix is not the release branch.
        result, calls = self.dispatch([self.ref(f"{branch}/other", "b" * 40)])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(["api", f"repos/{REPO}/git/refs", "-f", f"ref=refs/heads/{branch}",
                       "-f", f"sha={self.sha}", "--silent"], calls)
        self.assertIn(["workflow", "run", "ci.yml", "--repo", REPO, "--ref", branch], calls)
        self.assertIn(["workflow", "run", "release.yml", "--repo", REPO, "--ref", branch,
                       "-f", f"expected_sha={self.sha}", "-f", f"request_id={REQUEST_ID}",
                       "-f", "channel=stable"], calls)
        self.assertIn("Release v20261126.1 completed successfully.", result.stdout)

    def test_reuses_a_release_branch_at_the_same_commit(self):
        branch = f"release/{self.sha}"
        result, calls = self.dispatch([self.ref(branch, self.sha)])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(f"repos/{REPO}/git/refs", [call[1] for call in calls if call[0] == "api"])
        self.assertIn(["workflow", "run", "ci.yml", "--repo", REPO, "--ref", branch], calls)

    def test_refuses_a_release_branch_at_another_commit(self):
        result, calls = self.dispatch([self.ref(f"release/{self.sha}", "b" * 40)])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f"exists at {'b' * 40}, not {self.sha}", result.stderr)
        self.assertFalse([call for call in calls if call[0] == "workflow"])

    def test_promotion_runs_on_main_without_a_release_branch(self):
        result, calls = self.dispatch([], "--promote", "20261126.1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call for call in calls if call[0] == "workflow"],
                         [["workflow", "run", "promote.yml", "--repo", REPO, "--ref", "main",
                           "-f", "version=20261126.1", "-f", f"request_id={REQUEST_ID}"]])


if __name__ == "__main__":
    unittest.main()
