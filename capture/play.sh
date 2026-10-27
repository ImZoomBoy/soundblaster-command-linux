#!/bin/sh
# Plays one channel's test sound on the card from Linux, N times.
# Usage: capture/play.sh FL|FR|FC|LFE|RL|RR [times]
T="$(dirname "$0")/build/tests"; n=${2:-2}
for i in $(seq 1 "$n"); do
  pw-play --channels 6 --channel-map FL,FR,FC,LFE,RL,RR --properties '{ node.name = "sbcommand.test" media.role = "Test" }' \
          --target alsa_output.pci-0000_67_00.0.analog-surround-51 "$T/$1.wav"; sleep 1.5
done
