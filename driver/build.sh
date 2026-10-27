#!/bin/sh
# Builds the patched ca0132 driver module for the running kernel (no root needed).
# Downloads the matching upstream driver source (Linux v<major>.<minor>), applies
# zxr-fixes.patch and builds against the installed kernel headers.
#   sh driver/build.sh         the fixes only (what driver/install.sh installs)
#   sh driver/build.sh --dev   also dev/zxr-debug.patch: the temporary zxr_debug
#                              hook and experiments used by capture/ (testing only)
# Output: driver/build/snd-hda-codec-ca0132.ko
set -e
cd "$(dirname "$0")"
KVER=$(uname -r)
KDIR=/usr/lib/modules/$KVER/build
TAG=v$(echo "$KVER" | cut -d. -f1-2)
[ -d "$KDIR" ] || { echo "No kernel headers for $KVER (install your kernel's -headers package)."; exit 1; }
SRC=build/src-$TAG
if [ ! -f "$SRC/ca0132.c" ]; then
  mkdir -p "$SRC.part"
  URL=https://raw.githubusercontent.com/torvalds/linux/$TAG/sound/hda
  echo "Fetching the $TAG ca0132 driver source..."
  for f in codecs/ca0132.c codecs/ca0132_regs.h codecs/generic.h common/hda_local.h common/hda_auto_parser.h common/hda_jack.h; do
    curl -sfL -o "$SRC.part/$(basename "$f")" "$URL/$f" || { echo "Could not download $f for $TAG (Linux 6.17 or newer is needed)"; exit 1; }
  done
  mv "$SRC.part" "$SRC"
fi
cp "$SRC"/* build/ && cd build
cp ca0132.c ca0132.c.orig
patch -p0 ca0132.c < ../zxr-fixes.patch
[ "${1:-}" = "--dev" ] && patch -p0 ca0132.c < ../dev/zxr-debug.patch
printf 'obj-m += snd-hda-codec-ca0132.o\nsnd-hda-codec-ca0132-y := ca0132.o\n' > Makefile
LLVM=""; grep -q clang /proc/version && LLVM="LLVM=1"
make -C "$KDIR" M="$PWD" $LLVM modules
echo "Built $(pwd)/snd-hda-codec-ca0132.ko for $KVER${1:+ ($1)}"
