#!/bin/sh
# Sets the Sound Blaster ZxR's control pins (the card's "ca0113 GPIO" register,
# BAR2 offset 0x320) exactly the way the driver does: one 16-bit write per pin,
# value = pin | (state << 8). Used to find which pin silences which outputs.
# Run:  sudo sh dev/zxr-gpio.sh PIN STATE [PIN STATE ...]    e.g.  0 0 4 0
[ "$(id -u)" -eq 0 ] || { echo "Please run it with sudo."; exit 1; }
DEV=""
for d in /sys/bus/pci/devices/*; do
  if [ "$(cat "$d/vendor" 2>/dev/null)" = 0x1102 ] && [ "$(cat "$d/device" 2>/dev/null)" = 0x0012 ]; then DEV=$d; fi
done
[ -n "$DEV" ] || { echo "Sound Blaster not found"; exit 1; }
python3 -I - "$DEV/resource2" "$@" <<'PY' 2>&1 | tee /tmp/zxr-gpio.log
import sys, mmap, os, ctypes
path, args = sys.argv[1], sys.argv[2:]
if not args or len(args) % 2:
    sys.exit("usage: PIN STATE [PIN STATE ...]")
pairs = [(int(args[i]), int(args[i + 1])) for i in range(0, len(args), 2)]
if any(not 0 <= p <= 15 or s not in (0, 1) for p, s in pairs):
    sys.exit("pins are 0-15, states are 0 or 1")
fd = os.open(path, os.O_RDWR | os.O_SYNC)
mm = mmap.mmap(fd, 0x1000, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE)
reg = ctypes.c_uint16.from_buffer(mm, 0x320)
for p, s in pairs:
    reg.value = p | (s << 8)
    print(f"GPIO {p} -> {s}")
del reg
mm.close(); os.close(fd)
PY
chmod 644 /tmp/zxr-gpio.log 2>/dev/null || true
