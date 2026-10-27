#!/bin/sh
# Read-only and silent: plays a hiss on each of the six channels into a
# temporary null sink with the card's channel order, and prints where each one
# lands. Proves PipeWire's side of the routing without using the speakers.
set -e
S=sbnull-check; def=$(pactl get-default-sink)
m=$(pactl load-module module-null-sink sink_name=$S channels=6 channel_map=front-left,front-right,front-center,lfe,rear-left,rear-right sink_properties=device.description=$S)
pactl set-default-sink "$def"
trap 'pactl unload-module $m' EXIT
for slot in 0 1 2 3 4 5; do
  python3 -I - "$slot" <<'PY'
import sys, wave, struct, random
slot=int(sys.argv[1]); r=48000; n=r; out=bytearray(n*12); rnd=random.Random(1)
for i in range(n): struct.pack_into('<h', out, i*12+slot*2, int(rnd.uniform(-0.3,0.3)*32767))
w=wave.open('/tmp/sbcheck.wav','wb'); w.setnchannels(6); w.setsampwidth(2); w.setframerate(r); w.writeframes(bytes(out)); w.close()
PY
  parec --device=$S.monitor --raw --format=float32le --rate=48000 --channels=6 \
        --channel-map=front-left,front-right,front-center,lfe,rear-left,rear-right > /tmp/sbcheck.raw & r=$!
  sleep 0.3; pw-play --channels 6 --channel-map FL,FR,FC,LFE,RL,RR --target $S /tmp/sbcheck.wav; sleep 0.2; kill $r; wait $r 2>/dev/null || true
  python3 -I - "$slot" <<'PY'
import sys, struct, math
names=["FL","FR","FC","LFE","RL","RR"]; d=open('/tmp/sbcheck.raw','rb').read(); n=len(d)//24; a=[0.0]*6
for i in range(n):
    v=struct.unpack_from('<6f',d,i*24)
    for c in range(6): a[c]+=v[c]*v[c]
heard=[names[c] for c in range(6) if a[c]>0 and 10*math.log10(a[c]/n) > -60]
print(f"channel {names[int(sys.argv[1])]:3} -> {', '.join(heard) or 'nothing'}")
PY
done
rm -f /tmp/sbcheck.wav /tmp/sbcheck.raw
