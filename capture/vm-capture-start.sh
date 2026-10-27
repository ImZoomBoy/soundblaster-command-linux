#!/bin/sh
# Starts the Windows driver capture (no sudo; needs the ACS override boot
# option so the sound card has its own IOMMU group):
#  - saves the win11 VM's settings, shuts it (the CI runner) down
#  - boots it on a throwaway disk layer with the Sound Blaster passed through,
#    every card access logged by the patched QEMU to /var/tmp/sbcap/
# Undo everything with capture/vm-capture-stop.sh.
set -e
cd "$(dirname "$0")"
V="virsh -c qemu:///system"
PCI=0000:67:00.0
DIR=/var/lib/libvirt/images/win11
EMU=/var/tmp/sbcap-qemu/bin/qemu-system-x86_64
[ -x "$EMU" ] || { echo "Patched QEMU missing: build it first (see capture/README.md)."; exit 1; }
G=$(basename "$(readlink /sys/bus/pci/devices/$PCI/iommu_group)")
N=$(ls /sys/kernel/iommu_groups/$G/devices | while read d; do [ "$(cat /sys/bus/pci/devices/$d/class)" = 0x060400 ] || echo "$d"; done | wc -l)
[ "$N" = 1 ] || { echo "The card shares IOMMU group $G with $((N-1)) other devices: boot with the ACS override first."; exit 1; }
if [ -f build/win11-original.xml ]; then
  if $V dumpxml --inactive win11 | grep -q sbcap; then
    echo "A capture is already set up. Run vm-capture-stop.sh first."; exit 1
  fi
  echo "Resuming an interrupted start (VM settings are still the original ones)."
else
  $V dumpxml --inactive win11 > build/win11-original.xml
fi
echo "Shutting down the win11 VM (CI runner)..."
$V shutdown win11 >/dev/null 2>&1 || true
for i in $(seq 1 120); do [ "$($V domstate win11)" = "shut off" ] && break; sleep 2; done
[ "$($V domstate win11)" = "shut off" ] || { echo "VM did not shut down"; exit 1; }
# -u: don't open the base image (libvirt may own it); give the size instead.
SIZE=$($V domblkinfo win11 vda | awk '/Capacity/{print $2}')
rm -f "$DIR/win11.sbcap.qcow2"
qemu-img create -q -f qcow2 -u -b "$DIR/win11.qcow2" -F qcow2 "$DIR/win11.sbcap.qcow2" "$SIZE"
mkdir -p /var/tmp/sbcap; chmod 1777 /var/tmp/sbcap 2>/dev/null || true; [ -w /var/tmp/sbcap ] || { echo "/var/tmp/sbcap is not writable"; exit 1; }
TS=$(date +%Y%m%d-%H%M%S)
TRACE=/var/tmp/sbcap/trace-$TS.log
DMA=/var/tmp/sbcap/dma-$TS
mkdir -p "$DMA" && chmod 1777 "$DMA"
python3 -I vm-xml.py build/win11-original.xml "$DIR/win11.sbcap.qcow2" "$TRACE" "$PCI" "$EMU" "$DMA" > build/win11-capture.xml
$V define build/win11-capture.xml >/dev/null
$V start win11
echo "$TRACE" > build/current-trace
echo "$DMA" > build/current-dma
echo "Capture running; log: $TRACE"
