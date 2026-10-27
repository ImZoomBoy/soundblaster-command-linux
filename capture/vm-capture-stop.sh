#!/bin/sh
# Ends the capture: shuts the VM down (also logs the Windows driver's shutdown
# sequence), puts the original VM settings back, deletes the throwaway disk
# layer and starts the CI runner VM again. The sound card then needs a full
# power-off before Linux can use all its channels again.
set -e
cd "$(dirname "$0")"
V="virsh -c qemu:///system"
DIR=/var/lib/libvirt/images/win11
[ -f build/win11-original.xml ] || { echo "No capture set up."; exit 1; }
if [ "$($V domstate win11)" != "shut off" ]; then
  $V shutdown win11 >/dev/null 2>&1 || true
  for i in $(seq 1 120); do [ "$($V domstate win11)" = "shut off" ] && break; sleep 2; done
  [ "$($V domstate win11)" = "shut off" ] || $V destroy win11
fi
$V define build/win11-original.xml >/dev/null
rm -f "$DIR/win11.sbcap.qcow2"
mv build/win11-original.xml "build/win11-original.xml.used-$(date +%Y%m%d-%H%M%S)"
$V start win11
echo "win11 restored and started. Trace: $(cat build/current-trace 2>/dev/null)"
