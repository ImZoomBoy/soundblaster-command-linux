#!/bin/sh
# Saves a snapshot of the ZxR's internal state (read-only, silent) through the
# debug helper (capture/debug-helper.sh): ChipIO serial-port/clock registers,
# the audio router, the 8051 PLL/PMU registers and its XRAM tables.
# Usage: capture/snapshot.sh NAME   -> capture/build/regdump/NAME.txt
# Compare two:  diff capture/build/regdump/A.txt capture/build/regdump/B.txt
set -eu
NAME="${1:?usage: snapshot.sh NAME}"
OUT="$(dirname "$0")/build/regdump/$NAME.txt"
mkdir -p "$(dirname "$OUT")"
s() { systemctl start "sbcap-cmd@$(systemd-escape "$1")"; }
T0=$(date '+%Y-%m-%d %H:%M:%S'); sleep 1
s "dumpc 189000 40"; s "dumpc 18a000 40"; s "dumpc 18b000 40"; s "dumpc 190000 100"
s "dumpp 0"; s "dumpx 1400 200"; s "dumpx fa00 100"
sleep 1
journalctl -k --since "$T0" --no-pager -o cat | grep -o 'zxr_dump .*' | sed 's/^zxr_dump //' > "$OUT"
echo "$(wc -l < "$OUT") lines -> $OUT"
