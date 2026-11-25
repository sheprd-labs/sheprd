#!/bin/sh
# Takes the App Store screenshots from the demo (AppStore/, README.md "The
# demo"): no paired computer, so no one's real prompts or output appear, and
# the demo's own bar is hidden.
# Builds the app for the simulator, makes fresh iPhone 6.9" and iPad 13"
# simulators, and writes PNGs to AppStore/screenshots/ (not committed).
#
# Needs Xcode, XcodeGen, and the fleet log library (`just ios-fleet-ffi`).
# RUNTIME picks the simulator runtime (default: the newest iOS installed).
set -eu
cd "$(dirname "$0")/.."

out=AppStore/screenshots
mkdir -p "$out"
runtime=${RUNTIME:-$(xcrun simctl list runtimes | awk '/^iOS/ { id = $NF } END { print id }')}

xcodegen -q
# Signed to run locally, so the app has its entitlements in the simulator.
xcodebuild -project SheprdRemote.xcodeproj -scheme SheprdRemote \
    -destination 'generic/platform=iOS Simulator' -derivedDataPath build \
    CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= -quiet build
app=build/Build/Products/Debug-iphonesimulator/SheprdRemote.app

shoot() { # device, file, launch arguments…
    device=$1 file=$2
    shift 2
    xcrun simctl terminate "$device" so.pen.herdr.remote 2>/dev/null || true
    xcrun simctl launch "$device" so.pen.herdr.remote -provisionalAlerts YES -demo YES -hideDemoBar YES "$@" >/dev/null
    sleep 7
    xcrun simctl io "$device" screenshot "$out/$file.png" >/dev/null 2>&1
    echo "$out/$file.png"
}

for kind in iphone ipad; do
    case $kind in
        iphone) type="iPhone 17 Pro Max" ;;
        ipad) type="iPad Pro 13-inch (M5)" ;;
    esac
    device=$(xcrun simctl create "Sheprd store $kind" "$type" "$runtime")
    xcrun simctl boot "$device"
    xcrun simctl bootstatus "$device" >/dev/null
    xcrun simctl status_bar "$device" override --time 9:41 --batteryState charged \
        --batteryLevel 100 --cellularBars 4 --wifiBars 3
    xcrun simctl install "$device" "$app"
    # The iPad shows a chat beside the list.
    if [ "$kind" = ipad ]; then shoot "$device" "$kind-1-home" -demoChat c1; else shoot "$device" "$kind-1-home"; fi
    shoot "$device" "$kind-2-question" -demoChat c2
    shoot "$device" "$kind-3-chat" -demoChat c4 -agentPanel YES
    shoot "$device" "$kind-4-terminal" -openTerm p3
    shoot "$device" "$kind-5-workspaces" -tab workspaces
    xcrun simctl shutdown "$device"
    xcrun simctl delete "$device"
done
