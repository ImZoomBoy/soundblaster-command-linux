#!/bin/sh
# Reads the ZxR DSP's memory (chip addresses 0x000000-0x0bffff) through the debug
# helper (capture/debug-helper.sh). Read-only and silent.
# Usage: capture/dsp-snapshot.sh NAME  -> capture/build/regdump/dsp-NAME.txt
set -eu
NAME="${1:?usage: dsp-snapshot.sh NAME}"
OUT="$(dirname "$0")/build/regdump/dsp-$NAME.txt"
mkdir -p "$(dirname "$OUT")"; : > "$OUT"
a=0
while [ $a -lt $((0xc0000)) ]; do
  T0=$(date '+%Y-%m-%d %H:%M:%S'); sleep 1
  for i in 1 2 3 4 5 6 7 8; do
    systemctl start "sbcap-cmd@$(systemd-escape "dumpc $(printf %x $a) 400")"; a=$((a + 0x1000))
  done
  sleep 0.5
  journalctl -k --since "$T0" --no-pager -o cat | grep -o 'zxr_dump c .*' | sed 's/^zxr_dump c //' >> "$OUT"
done
echo "$(wc -l < "$OUT") lines -> $OUT (expect 24576)"
