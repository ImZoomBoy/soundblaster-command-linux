#!/bin/bash
# Builds and installs Sound Blaster Command for the current user (no sudo).
#   ./install.sh            build, install to ~/.local, start the background service
#   ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/scalable/apps"
SYMBOLIC="$HOME/.local/share/icons/hicolor/symbolic/apps"
HICOLOR="$HOME/.local/share/icons/hicolor"
SIZES="16 22 24 32 48 64 128 256"
# The icon's name changes with the icon (KDE keeps icons by name in memory);
# OLD_ICONS are the names earlier versions installed.
ICON=com.local.sbcommand-sb
OLD_ICONS="com.local.sbcommand"
UNITS="$HOME/.config/systemd/user"

if [ "${1:-}" = "--uninstall" ]; then
  systemctl --user disable --now sbcommand-watch.service 2>/dev/null || true
  rm -f "$BIN/sbcommand" "$APPS/sbcommand.desktop" "$UNITS/sbcommand-watch.service"
  for n in $ICON $OLD_ICONS; do
    rm -f "$ICONS/$n.svg" "$SYMBOLIC/$n-symbolic.svg"
    for s in $SIZES; do rm -f "$HICOLOR/${s}x${s}/apps/$n.png"; done
  done
  systemctl --user daemon-reload
  echo "Removed. Your settings are kept in ~/.config/sbcommand (delete it to remove them too)."
  exit 0
fi

command -v cargo >/dev/null || { echo "Needs Rust (https://rustup.rs)"; exit 1; }
cargo build --release
install -Dm755 target/release/sbcommand "$BIN/sbcommand"
install -Dm644 packaging/sbcommand.desktop "$APPS/sbcommand.desktop"
sed -i "s|^Exec=sbcommand|Exec=$BIN/sbcommand|" "$APPS/sbcommand.desktop"
# The icon, under its current name; the old names go.
for n in $OLD_ICONS; do
  rm -f "$ICONS/$n.svg" "$SYMBOLIC/$n-symbolic.svg"
  for s in $SIZES; do rm -f "$HICOLOR/${s}x${s}/apps/$n.png"; done
done
install -Dm644 packaging/com.local.sbcommand.svg "$ICONS/$ICON.svg"
install -Dm644 packaging/com.local.sbcommand-symbolic.svg "$SYMBOLIC/$ICON-symbolic.svg"
# Fixed sizes too, for panels and menus that want exact pixels.
for s in $SIZES; do
  install -Dm644 "packaging/icons/com.local.sbcommand-$s.png" "$HICOLOR/${s}x${s}/apps/$ICON.png"
done
# (a newer folder date makes icon caches look again)
touch "$HICOLOR"
install -Dm644 packaging/sbcommand-watch.service "$UNITS/sbcommand-watch.service"
systemctl --user daemon-reload
systemctl --user enable --now sbcommand-watch.service
"$BIN/sbcommand" apply || true
echo
echo "Installed. Open \"Sound Blaster Command\" from your app menu, or run: sbcommand"
echo "Optional (sudo): packaging/root/install-root-helpers.sh  - sleep fix and password-free Reset button"
