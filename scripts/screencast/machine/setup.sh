#!/bin/sh
# Prepares a virtual Mac for feature screencasts. Runs inside the machine as
# its user, from the folder prepare.py pushed this file and its siblings to:
#   sheprd machine exec NAME -- sh /tmp/screencast-machine/setup.sh
# Idempotent: safe to run again. Never touches sign-ins; prepare.py copies
# those separately.
set -eu
here=$(cd "$(dirname "$0")" && pwd)

# Stay awake and unlocked while recording.
defaults -currentHost write com.apple.screensaver idleTime 0
# A sleeping display locks the machine and a take then types into the lock
# screen: never sleep or lock, also after a restart, where caffeinate is gone.
echo admin | sudo -S pmset -a displaysleep 0 sleep 0 disksleep 0 2>/dev/null || true
echo admin | sudo -S sysadminctl -screenLock off -password admin 2>/dev/null || true
pkill -x caffeinate 2>/dev/null || true
nohup caffeinate -dimsu >/dev/null 2>&1 &

# A clean desktop: the Dock out of the way, desktop widgets hidden, and a
# ticking menu bar clock so screencapture keeps writing frames.
defaults write com.apple.dock autohide -bool true
defaults write com.apple.WindowManager StandardHideWidgets -bool true
defaults write com.apple.menuextra.clock ShowSeconds -bool true
# Hide the macOS menu bar so the window fills a 16:9 frame (System Settings >
# Menu Bar > Automatically hide: Always, which needs a log-out to apply). A
# video of a feature in a menu sets it back for itself. The larger pointer
# (Accessibility > Display > Pointer size, one step) cannot be set from a
# script: macOS protects that setting, so it is set by hand once.
defaults write NSGlobalDomain _HIHideMenuBar -bool true
defaults write NSGlobalDomain AppleMenuBarVisibleInFullscreen -bool false
killall Dock ControlCenter 2>/dev/null || true

# The agents a chat can pick: real Claude Code and Codex, plus the scripted
# demo agent under Gemini's name, which every recorded chat uses.
mkdir -p "$HOME/.local/bin"
if [ ! -x "$HOME/.local/bin/claude" ]; then
  curl -fsSL https://claude.ai/install.sh | bash >/dev/null
fi
if [ ! -x "$HOME/.local/bin/codex" ]; then
  url=https://github.com/openai/codex/releases/latest/download/codex-aarch64-apple-darwin.tar.gz
  curl -fsSL -o /tmp/codex.tgz "$url"
  tar xzf /tmp/codex.tgz -C /tmp
  mv /tmp/codex-aarch64-apple-darwin "$HOME/.local/bin/codex"
  chmod +x "$HOME/.local/bin/codex"
fi
# ~/.local/bin, because sheprd-acp looks there whatever PATH Sheprd was
# started with (an app opened by Finder or `open` gets launchd's, not a shell's).
cp "$here/acp_stub.py" "$HOME/.local/bin/gemini"
chmod +x "$HOME/.local/bin/gemini"

# The demo repository the videos work in, reset by each feature's setup.
repo="$HOME/code/forecast"
if [ ! -d "$repo/.git" ]; then
  mkdir -p "$repo"
  cp "$here/forecast.py" "$here/days.json" "$here/test_forecast.py" "$here/index.html" "$repo/"
  printf '__pycache__/\n' > "$repo/.gitignore"
  git -C "$repo" init -q -b main
  git -C "$repo" config user.name "Ada Lovelace"
  git -C "$repo" config user.email ada@example.com
  git -C "$repo" add -A
  git -C "$repo" commit -qm "Five-day forecast from sample data"
fi
# Older recording images have the forecast command but predate the browser demo.
if [ ! -e "$repo/index.html" ]; then
  cp "$here/index.html" "$repo/"
  git -C "$repo" add -- index.html
  git -C "$repo" commit -qm "Add the forecast preview" -- index.html
fi

# Sheprd's settings for recordings: the readable Default theme, Herdr 2
# terminals, chats that find the agents above, and no plan usage, which
# would show the signed-in accounts' limits.
config="$HOME/.config/herdr"
mkdir -p "$config"
# Kept as screencast.toml too: a video that changes settings copies it back
# over config-gpui.local.toml in its teardown.
cat > "$config/screencast.toml" <<'EOF'
# Screencast machine (scripts/screencast/machine/setup.sh). Saved changes reload.
layout = "comfortable-rounded"
theme = "Default"
herdr_2_terminals = true

[chat.env]
PATH = "/Users/admin/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

[usage]
show = false

# The palette's Projects list, which a video's setup opens the demo repo from.
[palette]
project_roots = ["~/code"]
EOF
cp "$config/screencast.toml" "$config/config-gpui.local.toml"
echo "machine ready: $(~/.local/bin/claude --version 2>/dev/null | head -1), $(~/.local/bin/codex --version 2>/dev/null)"
