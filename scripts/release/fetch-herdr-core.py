"""Download the pinned herdr-core release for one target and verify it.

    python3 scripts/release/fetch-herdr-core.py macos-aarch64 target/herdr-core
    python3 scripts/release/fetch-herdr-core.py aarch64-apple-darwin target/aarch64-apple-darwin/release

Every release bundles this binary beside the app's executable, as a companion
like sheprd-acp, so Herdr 2 terminals work with nothing else installed. The
target is herdr-core's own name for it or the Rust target the app was built
for. The version and checksums live in herdr-core.json beside this script; a
mismatch fails instead of bundling the download. Beside the binary goes
herdr-core-licenses.txt, the third-party notices from the same verified archive,
which every package ships as HERDR-CORE-LICENSES.txt.
"""
import hashlib
import io
import json
import re
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

PINNED = Path(__file__).with_name("herdr-core.json")
MAX_ARCHIVE = 64 << 20
LICENSES = "herdr-core-licenses.txt"
# Where each archive keeps herdr-core's third-party notices: a file in the
# tarballs, a folder holding herdr-core.txt in the Windows zip.
NOTICES = ("THIRD-PARTY-NOTICES", "THIRD-PARTY-NOTICES/herdr-core.txt")
# herdr-core 0.1.22's notices carry a file reader's page-break lines; flake.nix
# drops the same lines.
PAGE_BREAK = re.compile(r"^\[\d+ more lines in file\. Use offset=\d+ to continue\.\]\n", re.MULTILINE)

# The herdr-core build each Rust target the app ships for bundles. herdr-core
# publishes no Windows ARM64 build; Windows 11 on ARM runs the x64 one.
RUST_TARGETS = {
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-unknown-linux-gnu": "linux-aarch64",
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-pc-windows-msvc": "windows-x86_64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
}


def pinned(target):
    target = RUST_TARGETS.get(target, target)
    targets = json.loads(PINNED.read_text())["targets"]
    if target not in targets:
        raise SystemExit(f"herdr-core has no {target} build; known: {', '.join(sorted(targets))}")
    return targets[target]


def download(url):
    # The release service refuses urllib's default agent.
    request = urllib.request.Request(url, headers={"User-Agent": "herdr-gpui-release"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(MAX_ARCHIVE + 1)
    if len(data) > MAX_ARCHIVE:
        raise SystemExit(f"{url} is larger than {MAX_ARCHIVE} bytes")
    return data


def verified(data, sha256):
    actual = hashlib.sha256(data).hexdigest()
    if actual != sha256:
        raise SystemExit(f"herdr-core checksum mismatch: expected {sha256}, got {actual}")
    return data


def executable(data, name):
    """The archive member named `name`: top-level first, else the one at any depth."""
    if data[:2] == b"PK":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            members = [m for m in archive.namelist() if Path(m).name == name]
            top = [m for m in members if Path(m).parts == (name,)]
            if len(top) == 1:
                return archive.read(top[0])
            if len(members) == 1:
                return archive.read(members[0])
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            members = [m for m in archive.getmembers() if m.isfile() and Path(m.name).name == name]
            top = [m for m in members if Path(m.name).parts == (name,)]
            if len(top) == 1:
                return archive.extractfile(top[0]).read()
            if len(members) == 1:
                return archive.extractfile(members[0]).read()
    raise SystemExit(f"the herdr-core archive has no single {name}")


def member(data, names):
    """The first of `names`, as exact archive paths, that the archive holds."""
    wanted = [Path(name).parts for name in names]
    if data[:2] == b"PK":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            found = {Path(m).parts: m for m in archive.namelist() if not m.endswith("/")}
            for parts in wanted:
                if parts in found:
                    return archive.read(found[parts])
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            found = {Path(m.name).parts: m for m in archive.getmembers() if m.isfile()}
            for parts in wanted:
                if parts in found:
                    return archive.extractfile(found[parts]).read()
    raise SystemExit(f"the herdr-core archive has none of {', '.join(names)}")


def notices(data, version):
    """The archive's third-party notices, headed with `version`, without page breaks."""
    text = member(data, NOTICES).decode("utf-8").replace("\r\n", "\n")
    text = PAGE_BREAK.sub("", text)
    if "Apache-2.0" not in text:
        raise SystemExit("the herdr-core notices do not name its license")
    return f"herdr-core {version} (THIRD-PARTY-NOTICES from its release archive)\n\n{text}"


def write(path, data, mode):
    partial = path.with_suffix(".partial")
    partial.write_bytes(data)
    partial.chmod(mode)
    partial.replace(path)


def main(argv):
    if len(argv) != 3:
        raise SystemExit(__doc__)
    target, out_dir = RUST_TARGETS.get(argv[1], argv[1]), Path(argv[2])
    entry = pinned(target)
    name = "herdr-core.exe" if target.startswith("windows-") else "herdr-core"
    data = verified(download(entry["url"]), entry["sha256"])
    binary = executable(data, name)
    licenses = notices(data, json.loads(PINNED.read_text())["version"])
    out_dir.mkdir(parents=True, exist_ok=True)
    write(out_dir / LICENSES, licenses.encode("utf-8"), 0o644)
    path = out_dir / name
    write(path, binary, 0o755)
    print(path)


if __name__ == "__main__":
    main(sys.argv)
