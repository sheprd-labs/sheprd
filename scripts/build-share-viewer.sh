#!/bin/sh
# Regenerate the two static guest assets embedded in the native relay binary.
set -eu
cd "$(dirname "$0")/.."
bindgen=${WASM_BINDGEN:-wasm-bindgen}
required=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/"/, "", $3); print $3; exit}' Cargo.lock)
if [ "$("$bindgen" --version)" != "wasm-bindgen $required" ]; then
    echo "Install wasm-bindgen-cli $required (see docs/session-sharing/gateway.md)" >&2
    exit 1
fi
target=wasm32-unknown-unknown
# Keep developer-specific paths out of panic locations in the distributed binary.
root=$(pwd)
registry=${CARGO_HOME:-$HOME/.cargo}/registry/src
sysroot=$(rustc --print sysroot)
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="--remap-path-prefix=$root=/src --remap-path-prefix=$registry=/registry --remap-path-prefix=$sysroot=/rust"
cargo build --locked --release --target "$target" -p sheprd-share-viewer
output=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
"$bindgen" --target web --no-typescript --out-name guest \
    --out-dir crates/sheprd-relay/src/viewer/assets \
    "$output/$target/release/sheprd_share_viewer.wasm"
