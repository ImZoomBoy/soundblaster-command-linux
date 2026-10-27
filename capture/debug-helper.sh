#!/bin/bash
# TEMPORARY, for testing the Windows-capture findings without a password per step.
# First build the test driver as yourself:  sh driver/build.sh --dev
# Installs (needs sudo once):
#  - that driver from driver/build (with the zxr_debug hook) and reloads the card
#  - /usr/local/bin/sbcap-cmd: writes ONE whitelisted command to the driver's
#    zxr_debug parameter, or "reload" (driver restart)
#  - sbcap-cmd@.service + a polkit rule so your user can run it without a password:
#      systemctl start "sbcap-cmd@$(systemd-escape 'scp 96 1e 0')"
# Remove everything with:  sudo bash capture/debug-helper.sh --uninstall
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run with sudo."; exit 1; }
USER_NAME="${SUDO_USER:?run with sudo from your own account}"
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ "${1:-}" != "--uninstall" ] && ! modinfo "$HERE/../driver/build/snd-hda-codec-ca0132.ko" 2>/dev/null | grep -q "parm: *zxr_debug"; then
  echo "Build the test driver first, as yourself:  sh driver/build.sh --dev"; exit 1
fi
if [ "${1:-}" = "--uninstall" ]; then
  rm -f /usr/local/bin/sbcap-cmd /etc/systemd/system/sbcap-cmd@.service /etc/polkit-1/rules.d/51-sbcap-cmd.rules
  systemctl daemon-reload
  echo "debug helper removed. The test driver stays installed: rebuild without --dev and run driver/install.sh, or driver/install.sh --uninstall"
  exit 0
fi
cat > /usr/local/bin/sbcap-cmd <<'SH'
#!/bin/sh
# One whitelisted Sound Blaster debug command (see capture/debug-helper.sh).
CMD="$1"
P=/sys/module/snd_hda_codec_ca0132/parameters/zxr_debug
case "$CMD" in
  reload)
    for d in /sys/bus/pci/devices/*; do
      [ "$(cat $d/vendor 2>/dev/null)" = 0x1102 ] && [ "$(cat $d/device 2>/dev/null)" = 0x0012 ] && DEV=$(basename $d)
    done
    [ -n "$DEV" ] || exit 1
    echo "$DEV" > /sys/bus/pci/drivers/snd_hda_intel/unbind; sleep 1
    echo "$DEV" > /sys/bus/pci/drivers/snd_hda_intel/bind
    logger -t sbcap-cmd "reload $DEV"; exit 0 ;;
esac
echo "$CMD" | grep -Eq '^(scp|scpget|chipw|chipr|gpio|pll|exram|param|connrate|skipexit|desktopfw|winmmio|dumpx|dumpp|dumpc|verb|verbr|eapdfix|settle|streamgap|wake|wakenow)( [0-9a-fA-F]{1,8}){1,3}$' || { logger -t sbcap-cmd "rejected: $CMD"; exit 2; }
printf '%s' "$CMD" > "$P"
logger -t sbcap-cmd "$CMD"
SH
chmod 755 /usr/local/bin/sbcap-cmd
cat > /etc/systemd/system/sbcap-cmd@.service <<'UNIT'
[Unit]
Description=Sound Blaster debug command: %I

[Service]
Type=oneshot
ExecStart=/usr/local/bin/sbcap-cmd %I
UNIT
cat > /etc/polkit-1/rules.d/51-sbcap-cmd.rules <<RULE
// TEMPORARY: lets ${USER_NAME} run Sound Blaster debug commands (capture/debug-helper.sh).
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        /^sbcap-cmd@.*\.service$/.test(action.lookup("unit")) &&
        action.lookup("verb") == "start" &&
        subject.user == "${USER_NAME}") {
        return polkit.Result.YES;
    }
});
RULE
chmod 644 /etc/polkit-1/rules.d/51-sbcap-cmd.rules
systemctl daemon-reload
echo "helper installed; installing the patched driver..."
sh "$HERE/../driver/install.sh" >/dev/null 2>&1 || true
grep -E "now using|ZxR" /tmp/ca0132-fix.log | head -6
ls -la /sys/module/snd_hda_codec_ca0132/parameters/zxr_debug && echo "OK"
