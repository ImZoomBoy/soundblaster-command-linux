#!/bin/sh
# Installs (or with --uninstall removes) the patched Sound Blaster ZxR driver
# module built by driver/build.sh. The original module stays in
# place; the patched one goes in /lib/modules/<kernel>/updates/, which only
# applies to this exact kernel, so a kernel update removes it automatically.
# Run:  sudo sh driver/install.sh [--uninstall]
set -e
[ "$(id -u)" -eq 0 ] || { echo "Please run it with sudo."; exit 1; }
LOG=/tmp/ca0132-fix.log
if [ -z "$CA0132_LOGGING" ]; then CA0132_LOGGING=1 sh "$0" "$@" 2>&1 | tee "$LOG"; chmod 644 "$LOG"; exit 0; fi
KVER=$(uname -r)
SRC="$(cd "$(dirname "$0")" && pwd)/build/snd-hda-codec-ca0132.ko"
DST="/lib/modules/$KVER/updates/snd-hda-codec-ca0132.ko"
DRV=/sys/bus/pci/drivers/snd_hda_intel
DEV=""
for d in /sys/bus/pci/devices/*; do
  if [ "$(cat "$d/vendor" 2>/dev/null)" = 0x1102 ] && [ "$(cat "$d/device" 2>/dev/null)" = 0x0012 ]; then DEV=$(basename "$d"); fi
done
[ -n "$DEV" ] || { echo "Sound Blaster not found"; exit 1; }
if [ "$1" = "--uninstall" ]; then
  rm -f "$DST"; echo "removed $DST"
else
  modinfo -F vermagic "$SRC" | grep -q "^$KVER " || { echo "module was built for a different kernel"; exit 1; }
  install -Dm644 "$SRC" "$DST"; echo "installed $DST"
fi
depmod -a "$KVER"
echo "reloading the card driver (sound stops for a few seconds)..."
since=$(date '+%Y-%m-%d %H:%M:%S')
[ -e "$DRV/$DEV" ] && echo "$DEV" > "$DRV/unbind"
sleep 1
modprobe -r snd_hda_codec_ca0132 || echo "note: module still in use, it will reload on bind"
modprobe snd_hda_codec_ca0132
echo "$DEV" > "$DRV/bind"
sleep 6
echo "now using: $(modinfo -n snd_hda_codec_ca0132)"
journalctl -k --since "$since" --no-pager -o cat | grep -iE "ca0132|ZxR surround" || true
chmod 644 "$LOG" 2>/dev/null || true
