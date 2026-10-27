"""Tone file and analysis for dsp-probe.sh."""
import math, struct, sys, wave

TONES = [("FL", 310), ("FR", 430), ("FC", 570), ("LFE", 50), ("RL", 730), ("RR", 890)]
RATE = 48000

def make(path):
    n = RATE * 40
    amp = 32767 * 10 ** (-30 / 20)
    frames = bytearray(n * 12)
    for i in range(n):
        for c, (_, f) in enumerate(TONES):
            struct.pack_into("<h", frames, i * 12 + c * 2, int(amp * math.sin(2 * math.pi * f * i / RATE)))
    w = wave.open(path, "wb"); w.setnchannels(6); w.setsampwidth(2); w.setframerate(RATE); w.writeframes(bytes(frames)); w.close()

def level(x, f):
    """Goertzel level of frequency f in dBFS (sine peak = 0 dBFS)."""
    k = 2 * math.cos(2 * math.pi * f / RATE); s1 = s2 = 0.0
    for v in x:
        s0 = v + k * s1 - s2; s2, s1 = s1, s0
    p = s1 * s1 + s2 * s2 - k * s1 * s2
    a = 2 * math.sqrt(max(p, 0)) / len(x)
    return 20 * math.log10(a) if a > 1e-9 else -180

def analyse(d, name, quick=False):
    if quick:
        own = []
        for i, pair in enumerate(["c0c1", "c2c3", "c4c5"]):
            w = wave.open(f"{d}/{name}-{pair}.wav", "rb"); n = w.getnframes(); raw = w.readframes(n); w.close()
            vals = struct.unpack(f"<{n * 2}i", raw); start = int(RATE * 1.6)
            for ch in (0, 1):
                x = [v / 2 ** 31 for v in vals[start * 2 + ch::2]][: RATE]
                cname, f = TONES[i * 2 + ch]
                own.append(f"{cname} {'ok' if level(x, f) > -60 else 'DEAD'}")
        print(f"{name}: " + "  ".join(own))
        return
    print("DSP output   " + "".join(f"{n:>7s}" for n, _ in TONES) + "   (dBFS of each input's tone; tones played at -30)")
    for pair in ["c0c1", "c2c3", "c4c5", "c6c7", "c8c9", "cacb"]:
        w = wave.open(f"{d}/{name}-{pair}.wav", "rb"); n = w.getnframes(); raw = w.readframes(n); w.close()
        vals = struct.unpack(f"<{n * 2}i", raw)
        start = int(RATE * 1.6)                     # after the router change
        for ch in (0, 1):
            x = [v / 2 ** 31 for v in vals[start * 2 + ch::2]][: RATE]
            row = "".join(f"{level(x, f):7.0f}" for _, f in TONES)
            print(f"  {pair[ch * 2:ch * 2 + 2]}        {row}")

if __name__ == "__main__":
    make(sys.argv[2]) if sys.argv[1] == "make" else analyse(sys.argv[2], sys.argv[3], len(sys.argv) > 4)
