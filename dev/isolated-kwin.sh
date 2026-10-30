#!/bin/sh
# Usage: dev/isolated-kwin.sh "<command>"
# Runs a command in a private virtual KWin (1400x1000, or SBTEST_W x SBTEST_H), fully apart from the desktop:
# a private D-Bus that cannot start services, private config/data/cache
# folders, and no access to the real screen. Use it for every GUI test.
T=$(mktemp -d /tmp/sbtest-xdg.XXXXXX)
mkdir -p "$T/config" "$T/data" "$T/cache"
for f in kdeglobals gtk-4.0 gtk-3.0; do [ -e "$HOME/.config/$f" ] && cp -r "$HOME/.config/$f" "$T/config/"; done
env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS \
  XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache" \
  XDG_DATA_DIRS="$HOME/.local/share:/usr/local/share:/usr/share" \
  GDK_DEBUG=no-portals GIO_USE_VFS=local GTK_A11Y=none NO_AT_BRIDGE=1 \
  dbus-run-session --config-file="$(cd "$(dirname "$0")" && pwd)/isolated-bus.conf" -- \
  kwin_wayland --virtual --no-lockscreen --socket sbtest-wl --width "${SBTEST_W:-1400}" --height "${SBTEST_H:-1000}" --exit-with-session "$*"
rc=$?
rm -rf "$T"
exit $rc
