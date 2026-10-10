#!/bin/sh
# Installs the latest Clipdeck release on macOS:
#
#   curl -fsSL https://raw.githubusercontent.com/baala3/clipdeck/main/install.sh | sh
#
# Clipdeck is unsigned (docs/adr/0002-ship-unsigned.md). A browser download is
# quarantined, which is what makes Gatekeeper block it; a curl download isn't,
# so an app installed this way opens without the first-launch steps.
set -eu

url="https://github.com/baala3/clipdeck/releases/latest/download/Clipdeck_universal.app.tar.gz"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This script installs Clipdeck on macOS only." >&2
  echo "For Windows, see https://github.com/baala3/clipdeck/blob/main/docs/INSTALL.md" >&2
  exit 1
fi

# Standard (non-admin) accounts can't write to /Applications.
dest="${CLIPDECK_INSTALL_DIR:-/Applications}"
if [ ! -w "$dest" ]; then
  dest="$HOME/Applications"
  mkdir -p "$dest"
fi
app="$dest/Clipdeck.app"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading Clipdeck..."
curl -fL --progress-bar -o "$tmp/Clipdeck.app.tar.gz" "$url"
tar -xzf "$tmp/Clipdeck.app.tar.gz" -C "$tmp"

# A running copy would keep the old version alive, and `open` would only
# bring that one forward instead of starting the new one.
if pkill -f "$app/Contents/MacOS/" 2>/dev/null; then
  sleep 1
fi

rm -rf "$app"
mv "$tmp/Clipdeck.app" "$app"

echo "Installed Clipdeck to $app"
open "$app"
