#!/usr/bin/env python3
"""Make a virtual Mac ready for feature screencasts, from this computer.

  prepare.py [--machine NAME] [--app PATH/Sheprd.app] [--sign-ins]

The machine needs Sheprd Agent installed with both its permissions (a person
does that once, at the machine's screen). Then this script:

- pushes setup.sh, the scripted demo agent and the demo repository in, and
  runs setup.sh there (settings, Claude Code, Codex, the demo repository);
- with --app, installs that Sheprd build at ~/Apps/Sheprd.app, keeping the
  previous one in ~/Apps/old, and opens it;
- with --sign-ins, copies this computer's Claude Code and Codex sign-ins in,
  access tokens only: without their refresh tokens the machine can never
  rotate them and sign this computer out. Claude's lasts a few hours; run
  --sign-ins again before a recording session. Other tokens stored beside
  them, such as MCP servers', are never copied.

Save the result as an image afterwards (`sheprd machine image save`) so a lost
machine is one `sheprd machine new NAME --from IMAGE` away.
"""

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHEPRD = (os.environ.get("SHEPRD") or shutil.which("sheprd")
          or "/Applications/Sheprd.app/Contents/MacOS/sheprd")
CHUNK = 190_000  # base64 characters per exec: control messages cap at 256 KB
DEST = "/tmp/screencast-machine"


def machine(name, *args, timeout=120):
    return subprocess.run([SHEPRD, "machine", "exec", name, "--timeout", str(timeout), "--", *args],
                          check=True, capture_output=True, text=True).stdout


def push(name, local, remote):
    data = base64.b64encode(Path(local).read_bytes()).decode()
    machine(name, "sh", "-c", 'mkdir -p "$(dirname "$1")"; cp /dev/null "$1"', "_", remote)
    for i in range(0, len(data), CHUNK):
        machine(name, "sh", "-c", 'printf %s "$1" | base64 -D >> "$2"', "_",
                data[i:i + CHUNK], remote)


def install_hover(name):
    """Builds hover.c here (the machine may have no compiler) and installs it
    as ~/.local/bin/screencast-hover, signed for the machine."""
    with tempfile.TemporaryDirectory() as tmp:
        binary = Path(tmp) / "hover"
        subprocess.run(["/usr/bin/clang", "-O2", "-arch", "arm64", "-o", str(binary),
                        str(HERE / "hover.c"), "-framework", "ApplicationServices"], check=True)
        push(name, binary, "/tmp/screencast-hover")
    machine(name, "sh", "-c", "install -m 755 /tmp/screencast-hover ~/.local/bin/screencast-hover"
            " && codesign -f -s - ~/.local/bin/screencast-hover")


def install_app(name, app):
    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp) / "Sheprd.zip"
        subprocess.run(["ditto", "-c", "-k", "--keepParent", str(app), str(archive)], check=True)
        print(f"pushing {archive.stat().st_size // 1_000_000} MB")
        push(name, archive, "/tmp/Sheprd.zip")
    machine(name, "sh", "-c",
            "pkill -x sheprd; sleep 2; mkdir -p ~/Apps/old; "
            "[ -d ~/Apps/Sheprd.app ] && mv ~/Apps/Sheprd.app ~/Apps/old/Sheprd-$(date +%s).app; "
            "ditto -x -k /tmp/Sheprd.zip ~/Apps/ && open ~/Apps/Sheprd.app", timeout=300)


def sign_ins(name):
    codex = Path.home() / ".codex/auth.json"
    if codex.exists():
        auth = json.loads(codex.read_text())
        auth.get("tokens", {})["refresh_token"] = ""
        machine(name, "sh", "-c", 'umask 077; mkdir -p ~/.codex; printf %s "$1" > ~/.codex/auth.json',
                "_", json.dumps(auth))
        print("copied Codex's sign-in (no refresh token)")
    found = subprocess.run(["security", "find-generic-password", "-s", "Claude Code-credentials",
                            "-a", os.environ.get("USER", ""), "-w"],
                           capture_output=True, text=True, timeout=10)
    if found.returncode == 0:
        oauth = json.loads(found.stdout)["claudeAiOauth"]
        oauth.pop("refreshToken", None)
        oauth.pop("refreshTokenExpiresAt", None)
        machine(name, "sh", "-c",
                'security add-generic-password -U -s "Claude Code-credentials" -a "$USER" -w "$1"',
                "_", json.dumps({"claudeAiOauth": oauth}))
        print("copied Claude Code's sign-in (no refresh token)")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--machine", default="screencast")
    p.add_argument("--app", type=Path, help="a Sheprd.app to install in the machine")
    p.add_argument("--sign-ins", action="store_true", help="copy Claude Code and Codex sign-ins")
    a = p.parse_args()
    for f in ("setup.sh", "acp_stub.py", "forecast.py", "days.json", "test_forecast.py", "index.html"):
        push(a.machine, HERE / f, f"{DEST}/{f}")
    print(machine(a.machine, "sh", f"{DEST}/setup.sh", timeout=600).strip())
    install_hover(a.machine)
    if a.app:
        install_app(a.machine, a.app)
    if a.sign_ins:
        sign_ins(a.machine)


if __name__ == "__main__":
    if sys.platform != "darwin":
        sys.exit("prepare.py runs on the Mac that hosts the machine")
    main()
