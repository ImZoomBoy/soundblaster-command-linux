#!/bin/sh
# Records every HD-audio command the Linux driver sends to the Sound Blaster
# while it restarts the card (kernel hda tracepoints), for comparison with the
# Windows capture. Restarts the card's driver: sound stops for a few seconds.
# Run:  sudo sh capture/linux-trace.sh [label]     Output: /var/tmp/sbcap/linux-<label>-<time>.log
[ "$(id -u)" -eq 0 ] || { echo "Please run it with sudo."; exit 1; }
T=/sys/kernel/tracing
[ -d $T/events/hda ] || T=/sys/kernel/debug/tracing
DEV=""; for d in /sys/bus/pci/devices/*; do
  [ "$(cat $d/vendor 2>/dev/null)" = 0x1102 ] && [ "$(cat $d/device 2>/dev/null)" = 0x0012 ] && DEV=$(basename $d); done
[ -n "$DEV" ] || { echo "Sound Blaster not found"; exit 1; }
mkdir -p /var/tmp/sbcap; chmod 1777 /var/tmp/sbcap
OUT=/var/tmp/sbcap/linux-${1:-restart}-$(date +%Y%m%d-%H%M%S).log
echo 0 > $T/tracing_on; echo > $T/trace; echo 65536 > $T/buffer_size_kb
for e in hda_send_cmd hda_get_response hda_unsol_event; do [ -d $T/events/hda/$e ] && echo 1 > $T/events/hda/$e/enable; done
echo 1 > $T/tracing_on
DRV=/sys/bus/pci/drivers/snd_hda_intel
echo "$DEV" > $DRV/unbind; sleep 1; echo "$DEV" > $DRV/bind; sleep 8
echo 0 > $T/tracing_on
cat $T/trace > "$OUT"; chmod 644 "$OUT"
for e in hda_send_cmd hda_get_response hda_unsol_event; do [ -d $T/events/hda/$e ] && echo 0 > $T/events/hda/$e/enable; done
echo > $T/trace
echo "saved $OUT ($(grep -c hda_send_cmd "$OUT") commands)"
