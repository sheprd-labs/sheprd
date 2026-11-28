#!/bin/sh
# Measures the Pull Requests tab, SwiftUI against Luau, in a simulator:
# the app built optimized (with the DEBUG launch arguments kept), the same
# 100 made-up pull requests, alternating engines, fresh launch each run.
# See SheprdRemote/Luau/PullRequestsBench.swift for what a run measures.
#
#   ios/tools/prs_bench.sh SIMULATOR_UDID [RUNS]
set -eu

device=${1:?usage: prs_bench.sh SIMULATOR_UDID [RUNS]}
runs=${2:-5}
cd "$(dirname "$0")/.."
bundle=so.pen.herdr.remote
derived=build/bench

xcodegen -q
xcodebuild -project SheprdRemote.xcodeproj -scheme SheprdRemote -configuration Debug \
    -destination "platform=iOS Simulator,id=$device" -derivedDataPath "$derived" \
    SWIFT_OPTIMIZATION_LEVEL=-O GCC_OPTIMIZATION_LEVEL=s SWIFT_COMPILATION_MODE=wholemodule \
    CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= -quiet build
xcrun simctl boot "$device" 2>/dev/null || true
xcrun simctl install "$device" "$derived/Build/Products/Debug-iphonesimulator/SheprdRemote.app"

out=$(mktemp -d /tmp/prs-bench.XXXXXX)
i=1
while [ "$i" -le "$runs" ]; do
    for engine in swift luau; do
        xcrun simctl terminate "$device" "$bundle" 2>/dev/null || true
        data=$(xcrun simctl get_app_container "$device" "$bundle" data)
        rm -f "$data/Documents/prs-bench.json"
        xcrun simctl launch "$device" "$bundle" -demo YES -hideDemoBar YES -provisionalAlerts YES \
            -gitHubDemo bench -prsBench YES -pullRequestsEngine "$engine" >/dev/null
        waited=0
        until [ -f "$data/Documents/prs-bench.json" ] || [ "$waited" -ge 90 ]; do
            sleep 1
            waited=$((waited + 1))
        done
        cp "$data/Documents/prs-bench.json" "$out/$engine-$i.json"
        echo "$engine $i: $(cat "$out/$engine-$i.json")"
    done
    i=$((i + 1))
done
xcrun simctl terminate "$device" "$bundle" 2>/dev/null || true

python3 - "$out" <<'EOF'
import json, pathlib, statistics, sys
runs = {}
for path in sorted(pathlib.Path(sys.argv[1]).glob("*.json")):
    run = json.loads(path.read_text())
    runs.setdefault(run["engine"], []).append(run)
metrics = [
    ("firstFrameMs", "Tab open to first frame with rows, ms"),
    ("hitchMsPerS", "Hitch time while scrolling, ms/s"),
    ("hitches", "Hitched frames per run"),
    ("frames", "Frames per run"),
    ("medianFrameMs", "Median frame, ms"),
    ("worstFrameMs", "Worst frame, ms"),
    ("footprintBeforeMB", "Memory before the tab, MB"),
    ("footprintShownMB", "Memory with the tab shown, MB"),
    ("footprintAfterScrollMB", "Memory after scrolling, MB"),
]
print(f"\n{len(runs.get('swift', []))} runs each, median; {runs['swift'][0]['maxFramesPerSecond']:.0f} Hz screen\n")
print("| Metric | SwiftUI | Luau |\n| --- | ---: | ---: |")
for key, title in metrics:
    swift = statistics.median(r[key] for r in runs["swift"])
    luau = statistics.median(r[key] for r in runs["luau"])
    print(f"| {title} | {swift:.1f} | {luau:.1f} |")
EOF
