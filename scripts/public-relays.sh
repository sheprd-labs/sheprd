#!/usr/bin/env bash
# Deploys and checks Sheprd's public relays (docs/public-relays-plan.md):
# open droplets named sheprd-public-relay-REGION, served at
# REGION.relay.sheprd.org.
#
#   public-relays.sh deploy fra1 sfo3   create them, then check them
#   public-relays.sh check fra1 sfo3    wait for DNS, HTTPS, and QUIC
#
# Neither publishes anything: once a relay checks out, `check` prints the
# `just relays-add` line that lists and signs it.
#
# Optional environment:
#   PUBLIC_RELAY_DOMAIN  default relay.sheprd.org
#   RELAY_SSH_KEYS       SSH key ids for the droplets (default: every key
#                        on the DigitalOcean account)
#   RELAY_SIZE           droplet size (create-droplet.sh's default)
#   CHECK_MINUTES        how long `check` waits per relay (default 60)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
domain=${PUBLIC_RELAY_DOMAIN:-relay.sheprd.org}
prefix=sheprd-public-relay

usage() {
  echo "usage: $0 deploy|check REGION [REGION...]" >&2
  exit 2
}

# The droplet's public IPv4 address, or nothing when there is no droplet.
droplet_ip() {
  doctl compute droplet list --format Name,PublicIPv4 --no-header |
    awk -v name="$prefix-$1" '$1 == name { print $2; exit }'
}

# What the list calls a relay in a DigitalOcean region.
region_name() {
  case ${1%%[0-9]*} in
    fra) echo Frankfurt ;; ams) echo Amsterdam ;; lon) echo London ;;
    sfo) echo "San Francisco" ;; nyc) echo "New York" ;; tor) echo Toronto ;;
    atl) echo Atlanta ;; sgp) echo Singapore ;; syd) echo Sydney ;;
    blr) echo Bangalore ;; *) echo "$1" ;;
  esac
}

deploy() {
  local region
  for region in "$@"; do
    if [ -n "$(droplet_ip "$region")" ]; then
      echo "$prefix-$region already exists; check it with: just relays-check $region" >&2
      exit 1
    fi
  done
  local keys=${RELAY_SSH_KEYS:-}
  if [ -z "$keys" ]; then
    keys=$(doctl compute ssh-key list --format ID --no-header | paste -sd, -)
  fi
  RELAY_OPEN=1 RELAY_DOMAIN_SUFFIX=$domain RELAY_NAME_PREFIX=$prefix \
    RELAY_SSH_KEYS=$keys "$root/deploy/relay/create-droplet.sh" "$@"

  echo
  echo "Add these A records at sheprd.org's DNS host (TTL 300 or less):"
  for region in "$@"; do
    printf '  %-28s A  %s\n' "$region.$domain" "$(droplet_ip "$region")"
  done
  echo
  check "$@"
}

# Waits up to the deadline for `$@` to succeed, saying what it waits for.
wait_for() {
  local what=$1 deadline=$2
  shift 2
  until "$@"; do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "  gave up waiting for $what; run the check again later" >&2
      return 1
    fi
    echo "  waiting for $what..."
    sleep 30
  done
}

resolves_to() {
  [ "$(dig +short A "$1" @1.1.1.1 | tail -n 1)" = "$2" ]
}

# /v1/info answers over HTTPS from an open relay serving QUIC and the
# WebSocket fallback.
info_ok() {
  curl -fsS --max-time 10 "https://$1/v1/info" 2>/dev/null |
    python3 -c 'import json, sys
info = json.load(sys.stdin)
sys.exit(0 if info.get("listen_token") is False and info.get("websocket") and info.get("quic") else 1)'
}

check() {
  local region host ip deadline failed=0 ready=()
  for region in "$@"; do
    host=$region.$domain
    echo "$host"
    ip=$(droplet_ip "$region")
    if [ -z "$ip" ]; then
      echo "  no droplet $prefix-$region; deploy it with: just relays-deploy $region" >&2
      failed=1
      continue
    fi
    deadline=$(($(date +%s) + ${CHECK_MINUTES:-60} * 60))
    if wait_for "$host to resolve to $ip" "$deadline" resolves_to "$host" "$ip" &&
      wait_for "https://$host/v1/info from an open relay" "$deadline" info_ok "$host" &&
      cargo run --quiet --locked -p sheprd-relay --no-default-features --features client \
        --example probe -- "https://$host"; then
      ready+=("$region")
    else
      failed=1
    fi
  done
  if [ "${#ready[@]}" -gt 0 ]; then
    echo
    echo "Ready. Run the canary (docs/public-relays-plan.md), then list each one;"
    echo "compare the pin it prints with the relay's pin= log line before signing:"
    for region in "${ready[@]}"; do
      echo "  just relays-add https://$region.$domain $region \"$(region_name "$region")\""
    done
  fi
  return "$failed"
}

[ "$#" -ge 2 ] || usage
for region in "${@:2}"; do
  [[ $region =~ ^[a-z]+[0-9]+$ ]] || { echo "$region: not a DigitalOcean region such as fra1" >&2; exit 2; }
done
command -v doctl >/dev/null || { echo "doctl is not installed" >&2; exit 1; }
command=$1
shift
case $command in
  deploy) deploy "$@" ;;
  check) check "$@" ;;
  *) usage ;;
esac
