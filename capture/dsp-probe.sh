#!/bin/sh
# Measures which input channel reaches which DSP output, without listening.
# Plays six quiet tones at once (one pitch per 5.1 channel) and records the
# DSP's output channels c0..cb two at a time, by pointing the card's
# "What U Hear" capture router entries (0x36/0x37) at them.
# Needs the debug helper (capture/debug-helper.sh). Prints a level matrix.
# Usage: capture/dsp-probe.sh [NAME]   (saves capture/build/probe/NAME-*.wav)
#        QUICK=1 capture/dsp-probe.sh NAME   only c0-c5, prints a one-line verdict
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"; B="$HERE/build"; NAME="${1:-probe}"
P="$B/probe"; mkdir -p "$P"
s() { systemctl start "sbcap-cmd@$(systemd-escape "$1")"; }
mkdir -p "$B/tests"; [ -f "$B/tests/tones6.wav" ] || python3 -I "$HERE/dsp-probe.py" make "$B/tests/tones6.wav"
pw-play --channels 6 --channel-map FL,FR,FC,LFE,RL,RR --properties '{ node.name = "sbcommand.test" media.role = "Test" }' \
        --target alsa_output.pci-0000_67_00.0.analog-surround-51 "$B/tests/tones6.wav" & PLAY=$!
sleep 1
PAIRS="c0 c1,c2 c3,c4 c5"; [ -n "${QUICK:-}" ] || PAIRS="$PAIRS,c6 c7,c8 c9,ca cb"
IFS=,; set -f; for pair in $PAIRS; do unset IFS
  set -- $pair
  arecord -q -D hw:CARD=Creative,DEV=2 -f S32_LE -c 2 -r 48000 -d 3 "$P/$NAME-$1$2.wav" & REC=$!
  sleep 0.8
  s "chipw 1900d8 100$1"; s "chipw 1900dc 101$2"; s "chipw 19042c 1"
  wait $REC
done
kill $PLAY 2>/dev/null || true
unset IFS; python3 -I "$HERE/dsp-probe.py" analyse "$P" "$NAME" ${QUICK:+quick}
