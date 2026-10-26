#!/bin/bash
# Optional, needs sudo. Installs two system helpers for the Sound Blaster ZxR:
#  1. soundblaster-sleep.service: switches the card off before sleep and on
#     after waking (fixes video/audio freezing after resume on some systems).
#  2. soundblaster-reset-now.service + a polkit rule, so the app's
#     "Reset the sound card" button works without a password for this user.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run with sudo: sudo $0"; exit 1; }
USER_NAME="${SUDO_USER:?Run this with sudo from your own account}"
HERE="$(cd "$(dirname "$0")" && pwd)"

install -Dm755 "$HERE/soundblaster-reset" /usr/local/bin/soundblaster-reset

cat > /etc/systemd/system/soundblaster-sleep.service <<'UNIT'
[Unit]
Description=Reset the Sound Blaster card around sleep
Before=sleep.target
StopWhenUnneeded=yes

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/local/bin/soundblaster-reset pre
ExecStop=/usr/local/bin/soundblaster-reset post

[Install]
WantedBy=sleep.target
UNIT

cat > /etc/systemd/system/soundblaster-reset-now.service <<'UNIT'
[Unit]
Description=Reset the Sound Blaster card now

[Service]
Type=oneshot
ExecStart=/usr/local/bin/soundblaster-reset now
UNIT

cat > /etc/polkit-1/rules.d/50-soundblaster-reset.rules <<RULE
// Lets ${USER_NAME} reset the Sound Blaster from Sound Blaster Command without a password.
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "soundblaster-reset-now.service" &&
        action.lookup("verb") == "start" &&
        subject.user == "${USER_NAME}") {
        return polkit.Result.YES;
    }
});
RULE
chmod 644 /etc/polkit-1/rules.d/50-soundblaster-reset.rules
systemctl daemon-reload
systemctl enable soundblaster-sleep.service
echo "Done: sleep fix enabled, Reset button allowed for ${USER_NAME}."
