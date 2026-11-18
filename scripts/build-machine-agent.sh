#!/bin/bash
# Builds the installer disk every virtual Mac sees (crates/sheprd-gpui/src/
# machine/configuration.rs) as OUTPUT, which `just bundle` makes the bundle's
# Contents/Resources/MachineAgent.img: a raw, read-only HFS+ volume named
# Sheprd Agent holding Install Sheprd Agent.pkg. The package installs
# Sheprd Agent.app (macos/sheprd-machine-agent) in /Applications and a
# LaunchAgent that starts it at each login. The guest is a Mac of the same
# architecture as the host.
set -euo pipefail
[[ $# == 1 ]] || { echo 'Usage: bash scripts/build-machine-agent.sh OUTPUT' >&2; exit 2; }
[[ $(uname -s) == Darwin ]] || { echo 'macOS required' >&2; exit 1; }
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
source=$root/macos/sheprd-machine-agent
output=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
payload=$work/payload
app="$payload/Applications/Sheprd Agent.app"
mkdir -p "$app/Contents/MacOS" "$payload/Library/LaunchAgents" "$work/scripts" "$work/volume"
xcrun swiftc -O -swift-version 6 "$source/main.swift" "$source/Listener.swift" \
    "$source/Exec.swift" "$source/Screen.swift" "$source/Input.swift" "$source/Permissions.swift" \
    -o "$app/Contents/MacOS/sheprd-machine-agent"
cp "$source/Info.plist" "$app/Contents/Info.plist"
mkdir -p "$app/Contents/Resources"
cp "$root/assets/icons/Sheprd.icns" "$app/Contents/Resources/AppIcon.icns"
plutil -lint "$app/Contents/Info.plist" >/dev/null
# macOS keeps the guest's Accessibility and Screen Recording answers for
# this signature, so an agent built again must be allowed again.
codesign --force --sign - --identifier so.pen.sheprd.machine-agent "$app"
cp "$source/package/so.pen.sheprd.machine-agent.plist" "$payload/Library/LaunchAgents/"
cp "$source/package/postinstall" "$work/scripts/postinstall"
version=$(plutil -extract CFBundleShortVersionString raw "$app/Contents/Info.plist")
# The component list keeps the app in /Applications: by default Installer
# moves a package's app to wherever another copy with its identifier is.
# pkgbuild says "write: Permission denied" for each file whose
# com.apple.provenance attribute it cannot clear; the package is whole.
pkgbuild --quiet --root "$payload" --component-plist "$source/package/components.plist" \
    --scripts "$work/scripts" --identifier so.pen.sheprd.machine-agent \
    --version "$version" --install-location / "$work/volume/Install Sheprd Agent.pkg" \
    2> >(grep -v '^write: Permission denied$' >&2)
# UDTO is raw sectors, which the Virtualization framework attaches as is.
hdiutil create -quiet -ov -srcfolder "$work/volume" -volname 'Sheprd Agent' \
    -fs HFS+ -format UDTO "$work/agent"
mkdir -p "$(dirname -- "$output")"
mv -f "$work/agent.cdr" "$output"
