#!/usr/bin/env bash
# The relay's own checks, run from this folder: formatting, lints for each
# feature set a user may build, the tests, and the Swift client's tests
# when Swift is installed. This is what the relay's repository runs in CI.
set -euo pipefail
cd "$(dirname "$0")"

cargo fmt -p sheprd-relay --check

# Each feature on its own must build without warnings: clients take only
# what they need, and a test relay must not pull in QUIC or the dashboard.
for features in \
  "" \
  "--no-default-features" \
  "--no-default-features --features client" \
  "--no-default-features --features client-ws" \
  "--no-default-features --features testing" \
  "--no-default-features --features client-ws,testing" \
  "--no-default-features --features ops" \
  "--no-default-features --features quic-server" \
  "--all-features"; do
  echo "clippy ${features:-(default)}"
  # shellcheck disable=SC2086 # the feature flags are meant to split
  cargo clippy --locked -p sheprd-relay --all-targets $features -- -D warnings
done

cargo test --locked -p sheprd-relay --no-default-features --lib
cargo test --locked -p sheprd-relay --all-features

if command -v swift >/dev/null; then
  (cd swift && swift test)
else
  echo "swift not found: skipping the Swift client's tests" >&2
fi
