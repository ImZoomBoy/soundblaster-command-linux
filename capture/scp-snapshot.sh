#!/bin/sh
# Reads DSP parameters (SCP get) of modules 0x8f (speaker EQ)
# and 0x96 (speaker tuning / effects) through the debug helper. Silent.
# Usage: capture/scp-snapshot.sh NAME -> capture/build/regdump/scp-NAME.txt
set -eu
OUT="$(dirname "$0")/build/regdump/scp-${1:?usage: scp-snapshot.sh NAME}.txt"
T0=$(date '+%Y-%m-%d %H:%M:%S'); sleep 1
for m in 8f 96; do   # not 0x80: some of its "gets" allocate DMA channels
  last=3f; [ $m = 8f ] && last=0f; [ $m = 80 ] && last=1f
  for r in $(seq 0 $((0x$last))); do
    systemctl start "sbcap-cmd@$(systemd-escape "scpget $m $(printf %x $r)")"
  done
done
sleep 0.5
journalctl -k --since "$T0" --no-pager -o cat | grep -o 'zxr_debug: scp .*' | sed 's/zxr_debug: scp //' > "$OUT"
echo "$(wc -l < "$OUT") values -> $OUT"
