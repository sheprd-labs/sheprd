#!/bin/sh
# App Review guideline 2.5.2 bars code that changes what an app does, and
# 2.5.19 bars dormant features: MoonKit's live editing (MoonLive) must be
# absent from the store app, not switched off. Builds the Release app as an
# upload would and fails if it carries a trace of MoonLive, and also if it
# carries no trace of MoonKit itself: an audit that can't see the runtime it
# guards proves nothing. The markers are moonswift's scripts/store-audit.sh's.
#
#   ios/tools/luau_store_audit.sh
set -eu
cd "$(dirname "$0")/.."
derived=build/store-audit

# project.yml links the products it names; MoonLive must not be one.
if grep -n "product: MoonLive" project.yml; then
    echo "✘ project.yml links MoonLive" >&2
    exit 1
fi

xcodegen -q
xcodebuild -project SheprdRemote.xcodeproj -scheme SheprdRemote -configuration Release \
    -destination 'generic/platform=iOS' -derivedDataPath "$derived" CODE_SIGNING_ALLOWED=NO -quiet build

app="$derived/Build/Products/Release-iphoneos/SheprdRemote.app"
text=$(mktemp)
# Through a file: piped into `grep -q`, `strings` dies of SIGPIPE on a match.
strings -a "$app/SheprdRemote" > "$text"

found=0
for marker in "MoonLive" "applies as you type" "Showing the last good version." \
    "Point at anything to see what drew it" "Copy this element for an agent" "MOONSWIFT_EXAMPLE" \
    "LuauLive" "/api/agent/snapshot" "changed by an agent on"; do
    if grep -qF "$marker" "$text"; then
        echo "✘ the Release app contains: $marker" >&2
        found=$((found + 1))
    fi
done
grep -qF "MoonUI" "$text" || { echo "✘ no trace of MoonUI: the audit read the wrong binary" >&2; found=$((found + 1)); }
grep -qF "Luau/pull-requests" "$text" || { echo "✘ no trace of the Luau tab: the audit read the wrong binary" >&2; found=$((found + 1)); }

# Only fixed Luau ships: the bundled folder holds .luau files and nothing else.
others=$(find "$app/Luau" -type f ! -name '*.luau')
[ -z "$others" ] || { echo "✘ the bundled Luau folder holds other files: $others" >&2; found=$((found + 1)); }

rm -f "$text"
[ "$found" -eq 0 ] || exit 1
echo "✔ the Release app links MoonKit and MoonUI, no MoonLive or agents' live Luau, and bundles $(find "$app/Luau" -name '*.luau' | wc -l | tr -d ' ') Luau files"
