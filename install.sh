#!/bin/sh
# Installs the latest Clipdeck release on macOS or Linux:
#
#   curl -fsSL https://raw.githubusercontent.com/baala3/clipdeck/main/install.sh | sh
#
# Clipdeck is unsigned (docs/adr/0002-ship-unsigned.md). On macOS a browser
# download is quarantined, which is what makes Gatekeeper block it; a curl
# download isn't, so an app installed this way opens without the first-launch
# steps.
set -eu

releases="https://github.com/baala3/clipdeck/releases/latest/download"

install_macos() {
  url="$releases/Clipdeck_universal.app.tar.gz"

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
}

# Linux gets an AppImage: one file that runs as it is, needs no administrator
# rights, and can replace itself when Clipdeck updates.
install_linux() {
  if [ "$(uname -m)" != "x86_64" ]; then
    echo "Clipdeck for Linux is built for 64-bit Intel and AMD computers only, and this is $(uname -m)." >&2
    exit 1
  fi
  if [ "${XDG_SESSION_TYPE:-}" = "wayland" ]; then
    echo "Clipdeck doesn't work on Wayland yet, and this is a Wayland session." >&2
    echo "Log out, pick an X11 session (often called \"Xorg\") on the login screen, and run this again." >&2
    exit 1
  fi

  dest="${CLIPDECK_INSTALL_DIR:-$HOME/.local/bin}"
  app="$dest/Clipdeck.AppImage"
  launcher="${XDG_DATA_HOME:-$HOME/.local/share}/applications/clipdeck.desktop"
  mkdir -p "$dest" "$(dirname "$launcher")"

  # The AppImage's file name carries the version, so ask the updater manifest
  # where the latest one is.
  url="${CLIPDECK_APPIMAGE_URL:-}"
  if [ -z "$url" ]; then
    url=$(curl -fsSL "$releases/latest.json" | grep -o 'https://[^"]*\.AppImage' | head -n 1)
  fi
  if [ -z "$url" ]; then
    echo "The latest Clipdeck release has no Linux build." >&2
    exit 1
  fi

  echo "Downloading Clipdeck..."
  # Downloaded beside its final place, so the swap below is a rename.
  curl -fL --progress-bar -o "$app.new" "$url"
  chmod +x "$app.new"

  # A running copy would hand this one's start over to itself and stay on
  # the old version.
  if pkill -x clipdeck-app 2>/dev/null; then
    sleep 1
  fi
  mv "$app.new" "$app"

  # So Clipdeck shows up in the applications menu.
  cat > "$launcher" <<LAUNCHER
[Desktop Entry]
Type=Application
Name=Clipdeck
Comment=A lightweight clipboard manager
Exec="$app"
Icon=edit-paste
Categories=Utility;
Terminal=false
LAUNCHER

  echo "Installed Clipdeck to $app"
  nohup "$app" > /dev/null 2>&1 &
  echo "Clipdeck is running. Look for it in the system tray, or press Ctrl+Shift+V."
}

case "$(uname -s)" in
  Darwin) install_macos ;;
  Linux) install_linux ;;
  *)
    echo "This script installs Clipdeck on macOS and Linux." >&2
    echo "For Windows, see https://github.com/baala3/clipdeck/blob/main/docs/INSTALL.md" >&2
    exit 1
    ;;
esac
