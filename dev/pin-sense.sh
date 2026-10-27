#!/bin/sh
# Read-only: asks the Sound Blaster ZxR's codecs which output sockets have a
# plug in them, and how each output is set up. It only sends "get" commands;
# it changes nothing. Results go to /tmp/sb-pin-sense.txt.
# Run:  sudo sh dev/pin-sense.sh
[ "$(id -u)" -eq 0 ] || { echo "Please run it with sudo."; exit 1; }
python3 -I - > /tmp/sb-pin-sense.txt 2>&1 <<'PY'
import fcntl, struct, os, glob
IOC = 0xC0084811  # HDA_IOCTL_VERB_WRITE
def opener(path):
    fd = os.open(path, os.O_RDWR)
    def get12(nid, verb, payload=0):           # 12-bit "get" verbs
        buf = bytearray(struct.pack('<II', (nid << 24) | (verb << 8) | payload, 0))
        fcntl.ioctl(fd, IOC, buf, True); return struct.unpack('<II', buf)[1]
    def amp(nid, out, right):                  # GET_AMP_GAIN_MUTE
        p = (0x8000 if out else 0) | (0x0000 if right else 0x2000)
        buf = bytearray(struct.pack('<II', (nid << 24) | (0xB << 16) | p, 0))
        fcntl.ioctl(fd, IOC, buf, True); return struct.unpack('<II', buf)[1]
    return fd, get12, amp
card = [d for d in glob.glob('/proc/asound/card*/id') if open(d).read().strip() == 'Creative']
n = card[0].split('card')[1].split('/')[0] if card else '1'
for dev, label in [(f'/dev/snd/hwC{n}D1', 'main codec (ZxR)'), (f'/dev/snd/hwC{n}D2', 'daughterboard codec')]:
    if not os.path.exists(dev): print(dev, 'missing'); continue
    fd, get, amp = opener(dev)
    print(f'== {dev}  {label}  chip id=0x{get(0x00,0xf00,0x00):08x} (CA0132 = 0x11020011)')
    print(f'  GPIO data=0x{get(0x01,0xf15):02x} mask=0x{get(0x01,0xf16):02x} dir=0x{get(0x01,0xf17):02x}')
    for nid in (0x0b, 0x0f, 0x10, 0x11, 0x18):
        s = get(nid, 0xf09)
        print(f'  pin 0x{nid:02x}: plug={"yes" if s >> 31 else "no "} sense=0x{s:08x} pinctl=0x{get(nid,0xf07):02x} '
              f'eapd=0x{get(nid,0xf0c):02x} conn=0x{get(nid,0xf01):02x} power=0x{get(nid,0xf05):02x}')
    for nid in (0x02, 0x03, 0x04, 0x17):
        print(f'  dac 0x{nid:02x}: stream=0x{get(nid,0xf06):02x} power=0x{get(nid,0xf05):02x} '
              f'amp L=0x{amp(nid,1,0):02x} R=0x{amp(nid,1,1):02x}')
    os.close(fd)
PY
chmod 644 /tmp/sb-pin-sense.txt
cat /tmp/sb-pin-sense.txt
