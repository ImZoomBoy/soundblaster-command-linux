#!/bin/bash
# Builds and installs Sound Blaster Command for the current user (no sudo).
#   ./install.sh            build, install to ~/.local, start the background service
#   ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/scalable/apps"
UNITS="$HOME/.config/systemd/user"

if [ "${1:-}" = "--uninstall" ]; then
  systemctl --user disable --now sbcommand-watch.service 2>/dev/null || true
  rm -f "$BIN/sbcommand" "$APPS/sbcommand.desktop" "$ICONS/com.local.sbcommand.svg" "$UNITS/sbcommand-watch.service"
  systemctl --user daemon-reload
  echo "Removed. Your settings are kept in ~/.config/sbcommand (delete it to remove them too)."
  exit 0
fi

command -v cargo >/dev/null || { echo "Needs Rust (https://rustup.rs)"; exit 1; }
cargo build --release
install -Dm755 target/release/sbcommand "$BIN/sbcommand"
install -Dm644 packaging/sbcommand.desktop "$APPS/sbcommand.desktop"
sed -i "s|^Exec=sbcommand|Exec=$BIN/sbcommand|" "$APPS/sbcommand.desktop"
install -Dm644 packaging/com.local.sbcommand.svg "$ICONS/com.local.sbcommand.svg"
install -Dm644 packaging/sbcommand-watch.service "$UNITS/sbcommand-watch.service"
systemctl --user daemon-reload
systemctl --user enable --now sbcommand-watch.service
"$BIN/sbcommand" apply || true
echo
echo "Installed. Open \"Sound Blaster Command\" from your app menu, or run: sbcommand"
echo "Optional (sudo): packaging/root/install-root-helpers.sh  - sleep fix and password-free Reset button"
