#!/bin/sh
# Hard-resets the Sound Blaster over the PCI bus (the motherboard pulses the
# card's reset line), then loads the driver again. Closer to a cold start than
# a driver reload, which leaves the card's processors running.
# Run:  sudo sh dev/hard-reset.sh       Log: /tmp/zxr-hard-reset.log
[ "$(id -u)" -eq 0 ] || { echo "Please run it with sudo."; exit 1; }
if [ -z "$ZXR_LOGGING" ]; then ZXR_LOGGING=1 sh "$0" "$@" 2>&1 | tee /tmp/zxr-hard-reset.log; chmod 644 /tmp/zxr-hard-reset.log; exit 0; fi
DRV=/sys/bus/pci/drivers/snd_hda_intel
DEV=""
for d in /sys/bus/pci/devices/*; do
  if [ "$(cat "$d/vendor" 2>/dev/null)" = 0x1102 ] && [ "$(cat "$d/device" 2>/dev/null)" = 0x0012 ]; then DEV=$(basename "$d"); fi
done
[ -n "$DEV" ] || { echo "Sound Blaster not found"; exit 1; }
echo "reset method: $(cat /sys/bus/pci/devices/$DEV/reset_method)"
since=$(date '+%Y-%m-%d %H:%M:%S')
[ -e "$DRV/$DEV" ] && echo "$DEV" > "$DRV/unbind" && echo "driver unbound"
sleep 1
if echo 1 > "/sys/bus/pci/devices/$DEV/reset"; then echo "bus reset done"; else echo "bus reset FAILED"; fi
sleep 1
echo "$DEV" > "$DRV/bind" && echo "driver bound"
sleep 7
logger -t soundblaster-reset "hard reset $DEV"
journalctl -k --since "$since" --no-pager -o cat | grep -iE "67:00|ca0132|ZxR" | grep -vE "autoconfig|speaker_outs|hp_outs|mono:|dig-|inputs:|Mic=|Line=|Aux=|picked fixup"
