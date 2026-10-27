#!/usr/bin/env python3
"""Decodes a Sound Blaster (CA0132) capture into readable operations.

Reads either log:
  - the patched QEMU log (capture/qemu-sbcap-trace.patch): V/A/W/R lines
  - a Linux hda tracepoint log (capture/linux-trace.sh): hda_send_cmd/hda_get_response

Usage: ops.py LOG [--from USEC] [--to USEC] [--raw]
Prints one operation per line: "<usec> <codec> <OP> <details>".

Operations (main codec unless noted):
  CHIP_W addr=val      ChipIO memory write (widget 0x15, address auto-increments)
  CHIP_R addr -> val   ChipIO memory read
  EXRAM_W addr=val     8051 external RAM write;   EXRAM_R addr -> val
  PLL_W addr=val       8051 PLL/PMU register write; PLL_R addr -> val
  IRAM_W, DIRECT_W     8051 internal RAM / direct writes
  PARAM name=val       ChipIO control parameter (stream routing, rates, ASI ...)
  CONN_RATE id=rate    connection point sample rate (PARAM 29 + 30)
  STREAM id ...        stream source/dest/channels/control (PARAM 24..28)
  FLAG id=on|off       ChipIO control flag
  SCP mod req [vals]   DSP parameter message (widget 0x16); floats shown as numbers
  GPIO pin=0|1         card GPIO (BAR2 0x320), Windows captures only
  BAR2_W off=val       other BAR2 writes, Windows captures only
  VERB nid name=payload  any other HD-audio command
"""
import re, struct, sys

CHIP, DSP = 0x15, 0x16
PARAM_NAMES = {1: "VIP_SOURCE", 2: "SPDIF1_SOURCE", 8: "PORTA_160OHM_GAIN", 10: "PORTD_160OHM_GAIN",
               23: "ASI", 24: "STREAM_ID", 25: "STREAM_SRC", 26: "STREAM_DEST", 27: "STREAM_CHANNELS",
               28: "STREAM_CONTROL", 29: "CONN_ID", 30: "CONN_RATE", 31: "NODE_ID"}
RATES = {0x00: "6k", 0x01: "8k", 0x02: "9.6k", 0x03: "11.025k", 0x04: "16k", 0x05: "22.05k", 0x06: "24k",
         0x07: "32k", 0x08: "44.1k", 0x09: "48k", 0x0a: "88.2k", 0x0b: "96k", 0x0c: "144k", 0x0d: "176.4k",
         0x0e: "192k", 0x0f: "384k"}
STD12 = {0x701: "CONN_SEL", 0x703: "PROC_STATE", 0x705: "POWER", 0x706: "CHAN_STREAM", 0x707: "PIN_CTL",
         0x708: "UNSOL", 0x709: "PIN_SENSE", 0x70C: "EAPD", 0x70D: "DIGI1", 0x70E: "DIGI2",
         0x715: "GPIO_DATA", 0x716: "GPIO_MASK", 0x717: "GPIO_DIR", 0x71C: "CFG0", 0x71D: "CFG1",
         0x71E: "CFG2", 0x71F: "CFG3", 0x7FF: "FUNC_RESET", 0x724: "CVT_CHAN_CNT"}


def f32(v):
    return struct.unpack("<f", struct.pack("<I", v & 0xffffffff))[0]


def parse(path):
    """Yields (usec, kind, a, b): kind V (verb, response) or W/R (bar, off, size, data)."""
    pending = None
    with open(path, errors="replace") as fh:
        for line in fh:
            if "hda_send_cmd" in line or "hda_get_response" in line:
                m = re.search(r"(\d+\.\d+): hda_(send_cmd|get_response): \[[^\]]*:(\d)\] val=0x([0-9a-f]+)", line)
                if not m:
                    continue
                t = int(float(m.group(1)) * 1e6)
                if m.group(2) == "send_cmd":
                    if pending:
                        yield pending + (None,)
                    pending = (t, "V", int(m.group(4), 16))
                elif pending:
                    yield pending + (int(m.group(4), 16),)
                    pending = None
                continue
            p = line.split()
            if not p or p[0] == "#":
                continue
            if p[0] in ("V", "A") and len(p) < 4:
                continue  # line still being written
            if p[0] == "V":
                if pending:
                    yield pending + (None,)
                pending = (int(p[1]), "V", int(p[3], 16))
            elif p[0] == "A":
                if pending:
                    yield pending + (int(p[3], 16),)
                    pending = None
            elif p[0] in ("W", "R") and len(p) >= 6:
                yield (int(p[1]), p[0], (int(p[2]), int(p[3], 16), int(p[4]), int(p[5], 16)), None)
    if pending:
        yield pending + (None,)


class Decoder:
    def __init__(self):
        self.chip_addr = None
        self.chip_lo = None
        self.chip_data_lo = 0
        self.e_addr = 0
        self.params = {}
        self.scp = []          # words of the SCP message being assembled
        self.scp_lo = None
        self.stream = None

    def verb(self, t, v, resp):
        cad, nid = v >> 28, (v >> 20) & 0xff
        hi = (v >> 16) & 0xf
        if hi in (0x7, 0xF):
            vid, pay = (v >> 8) & 0xfff, v & 0xff
        else:
            vid, pay = hi << 8, v & 0xffff
        r = f"{resp:08x}" if resp is not None else "-"
        out = []
        if nid == CHIP and cad == 1:
            out += self.chip(vid, pay, resp)
        elif nid == DSP and cad == 1:
            out += self.dsp(vid, pay, resp)
        else:
            name = STD12.get(vid, f"{vid:03x}")
            if vid >> 8 == 0xF:
                out.append(f"GET nid={nid:02x} {name}={pay:02x} -> {r}")
            else:
                out.append(f"VERB nid={nid:02x} {name}={pay:04x}")
        return [(t, cad, o) for o in out]

    def chip(self, vid, pay, resp):
        o = []
        if vid == 0x000:
            self.chip_lo = pay
        elif vid == 0x100:
            lo = self.chip_lo if self.chip_lo is not None else (self.chip_addr or 0) & 0xffff
            self.chip_addr = (pay << 16) | lo
            self.chip_lo = None
        elif vid == 0x300:
            self.chip_data_lo = pay
        elif vid == 0x400:
            addr = self.cur_chip_addr()
            o.append(f"CHIP_W {addr:06x}={(pay << 16) | self.chip_data_lo:08x}")
            self.chip_addr = addr + 4
        elif vid == 0xF03:
            addr = self.cur_chip_addr()
            o.append(f"CHIP_R {addr:06x} -> {resp if resp is not None else 0:08x}")
            self.chip_addr = addr + 4
        elif vid == 0x702:
            pass  # HIC post-read
        elif vid == 0x70D:
            self.e_addr = (self.e_addr & 0xff00) | pay
        elif vid == 0x70E:
            self.e_addr = (self.e_addr & 0x00ff) | (pay << 8)
        elif vid == 0x707:
            o.append(f"EXRAM_W {self.e_addr:04x}={pay:02x}")
        elif vid == 0xF07:
            o.append(f"EXRAM_R {self.e_addr:04x} -> {(resp or 0) & 0xff:02x}")
        elif vid == 0x70C:
            o.append(f"PLL_W {self.e_addr & 0xff:02x}={pay:02x}")
        elif vid == 0xF0C:
            o.append(f"PLL_R {self.e_addr & 0xff:02x} -> {(resp or 0) & 0xff:02x}")
        elif vid == 0x709:
            o.append(f"IRAM_W {self.e_addr & 0xff:02x}={pay:02x}")
        elif vid == 0x500:
            o.append(f"DIRECT_W {pay & 0xff:02x}={pay >> 8:02x}")
        elif vid == 0x710:
            o += self.param(pay & 0x1f, pay >> 5)
        elif vid == 0x717:
            self.ex_id = pay
        elif vid == 0x718:
            o += self.param(getattr(self, "ex_id", -1), pay)
        elif vid == 0x70F:
            o.append(f"FLAG {pay >> 1}={'on' if pay & 1 else 'off'}")
        elif vid == 0x70A:
            o.append(f"CT_EXTENSIONS={pay:02x}")
        elif vid == 0x200:
            o.append(f"CHIP_STREAM_FORMAT={pay:04x}")
        elif vid in (0xF01, 0xF00, 0xF0A, 0xF0F, 0xF10, 0xF12):
            pass  # status / getters: noise
        else:
            o.append(f"CHIP {vid:03x}={pay:04x}")
        return o

    def cur_chip_addr(self):
        if self.chip_lo is not None:
            self.chip_addr = ((self.chip_addr or 0) & 0xffff0000) | self.chip_lo
            self.chip_lo = None
        return self.chip_addr or 0

    def param(self, pid, val):
        self.params[pid] = val
        name = PARAM_NAMES.get(pid, str(pid))
        if pid == 30:
            return [f"CONN_RATE {self.params.get(29, -1):02x}={RATES.get(val, val)}"]
        if pid == 29:
            return []
        if pid == 24:
            self.stream = val
            return []
        if pid in (25, 26, 27, 28):
            return [f"STREAM {self.params.get(24, -1):02x} {name[7:]}={val:x}"]
        return [f"PARAM {name}={val:x}"]

    def dsp(self, vid, pay, resp):
        o = []
        if vid == 0x000:
            self.scp_lo = pay
        elif vid == 0x100:
            word = (pay << 16) | (self.scp_lo or 0)
            self.scp.append(word)
            hdr = self.scp[0]
            size = (hdr >> 27) & 0x1f
            if len(self.scp) == size + 1:
                mod, req, get = hdr & 0xff, (hdr >> 17) & 0x7f, (hdr >> 16) & 1
                vals = " ".join(fmt_scp(w) for w in self.scp[1:])
                o.append(f"SCP {'GET ' if get else ''}{mod:02x}/{req:02x} {vals}".rstrip())
                self.scp = []
        elif vid == 0x703:
            o.append(f"DSP_INIT={pay:02x}")
        elif vid in (0xF01, 0x702, 0xF02, 0x704, 0xF04):
            pass
        else:
            o.append(f"DSP {vid:03x}={pay:04x}")
        return o


def fmt_scp(w):
    x = f32(w)
    if w in (0, 0x3f800000) or (1e-4 < abs(x) < 1e6 and x == round(x, 4)):
        return f"{x:g}"
    return f"{w:08x}"


def ops(path, t0=None, t1=None):
    d = Decoder()
    for t, kind, a, b in parse(path):
        if (t0 is not None and t < t0) or (t1 is not None and t > t1):
            if kind == "V":
                d.verb(t, a, b)  # keep decoder state in step
            continue
        if kind == "V":
            yield from d.verb(t, a, b)
        elif kind == "W":
            bar, off, size, data = a
            if bar == 2 and off == 0x320:
                yield (t, 1, f"GPIO {data & 0xf}={(data >> 8) & 1}")
            elif bar == 2:
                yield (t, 1, f"BAR2_W {off:03x}={data:x}/{size}")


if __name__ == "__main__":
    args = sys.argv[1:]
    t0 = int(args[args.index("--from") + 1]) if "--from" in args else None
    t1 = int(args[args.index("--to") + 1]) if "--to" in args else None
    last = None
    for t, cad, op in ops(args[0], t0, t1):
        if last is not None and t - last > 1_000_000:
            print(f"# --- gap {(t - last) / 1e6:.1f} s ---")
        last = t
        print(t, cad, op)
