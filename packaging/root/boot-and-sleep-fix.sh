#!/bin/bash
# Fix 2 for the Sound Blaster ZxR (needs sudo, then a restart):
#  - adds iommu=pt to the kernel command line (prevents the IOMMU page faults
#    that stalled the card after waking from sleep)
#  - stops the sleep hook from restarting the card's driver (no longer needed
#    with iommu=pt)
#  - --capture: also adds pcie_acs_override=downstream,multifunction, so the
#    card gets its own IOMMU group and can be passed to the Windows VM for the
#    driver capture (temporary; remove with --end-capture)
# Kernel options go in limine-entry-tool drop-in files (one option per file,
# no quotes, no comments on the option line, as limine-entry-tool requires).
# Usage: sudo bash packaging/root/boot-and-sleep-fix.sh [--capture | --end-capture]
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run with sudo."; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"

install -Dm755 "$HERE/soundblaster-reset" /usr/local/bin/soundblaster-reset
[ -f /etc/default/soundblaster-reset ] || printf 'SLEEP_RESET=no\n' > /etc/default/soundblaster-reset
echo "== sleep hook: leaves the card running (SLEEP_RESET in /etc/default/soundblaster-reset)"

if ! command -v limine-update >/dev/null || ! command -v limine-entry-tool >/dev/null; then
  echo "== This script sets kernel options for the Limine boot loader (CachyOS's default) only."
  echo "   Add iommu=pt to your kernel command line yourself, then restart:"
  echo "   - GRUB: add it to GRUB_CMDLINE_LINUX_DEFAULT in /etc/default/grub, then run"
  echo "     update-grub (Debian/Ubuntu) or grub-mkconfig -o /boot/grub/grub.cfg"
  echo "   - systemd-boot: add it to the options line in /boot/loader/entries/*.conf (or /etc/kernel/cmdline)"
  exit 0
fi
D=/etc/limine-entry-tool.d
mkdir -p "$D"

# Undo the first version of this script, which put commented lines into
# /etc/default/limine (limine copies comments into the command line).
if [ -f /etc/default/limine ] && grep -q "sbcommand" /etc/default/limine; then
  cp -a /etc/default/limine "/etc/default/limine.bak-$(date +%Y%m%d-%H%M%S)"
  sed -i '/sbcommand/d' /etc/default/limine
  echo "removed the earlier sbcommand lines from /etc/default/limine"
fi

printf '%s\n' '### Sound Blaster Command: IOMMU passthrough, so the Sound Blaster does not stall after sleep.' \
  'KERNEL_CMDLINE[default]+=iommu=pt' > "$D/50-sbcommand.conf"
case "${1:-}" in
  --capture)
    printf '%s\n' '### Sound Blaster Command capture: TEMPORARY. Remove with boot-and-sleep-fix.sh --end-capture.' \
      'KERNEL_CMDLINE[default]+=pcie_acs_override=downstream,multifunction' > "$D/51-sbcommand-capture.conf" ;;
  --end-capture)
    rm -f "$D/51-sbcommand-capture.conf" ;;
esac

echo "== rebuilding boot entries (limine-update)..."
limine-update >/tmp/limine-update.log 2>&1 || { echo "limine-update FAILED, see /tmp/limine-update.log"; exit 1; }
CMD=$(limine-entry-tool --get-cmdline 2>/dev/null | head -1 || true)
echo "== kernel command line for the next boot:"
echo "   ${CMD:-<limine-entry-tool --get-cmdline printed nothing>}"
grep -E '^\s*(kernel_)?cmdline:' /boot/limine.conf | sort -u | sed 's/^/   limine.conf: /' | head -4
if grep -E '^\s*(kernel_)?cmdline:' /boot/limine.conf | grep -q '[#"]'; then
  echo "!! A boot entry still contains '#' or a quote. Do NOT restart; check /boot/limine.conf."
  exit 1
fi
echo "OK. Restart to use it."
