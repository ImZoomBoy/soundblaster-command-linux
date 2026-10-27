#!/usr/bin/env python3
"""Makes the capture version of the win11 domain XML.
Usage: vm-xml.py ORIGINAL.xml OVERLAY.qcow2 TRACEFILE PCI_ADDR EMULATOR > capture.xml"""
import sys, xml.etree.ElementTree as ET
src, overlay, trace, pci, emulator = sys.argv[1:6]
dma_dir = sys.argv[6] if len(sys.argv) > 6 else None
QNS = "http://libvirt.org/schemas/domain/qemu/1.0"
ET.register_namespace("qemu", QNS)
tree = ET.parse(src); dom = tree.getroot()
dev = dom.find("devices")
dev.find("emulator").text = emulator
for disk in dev.findall("disk"):
    if disk.get("device") == "disk":
        disk.find("source").set("file", overlay)
        for b in disk.findall("backingStore"):
            disk.remove(b)
for snd in dev.findall("sound"):            # the Creative card should be Windows' only sound device
    dev.remove(snd)
dom_, bus, slotfn = pci.split(":")
slot, fn = slotfn.split(".")
hd = ET.SubElement(dev, "hostdev", mode="subsystem", type="pci", managed="yes")
s = ET.SubElement(hd, "source")
ET.SubElement(s, "address", domain="0x" + dom_, bus="0x" + bus, slot="0x" + slot, function="0x" + fn)
ET.SubElement(hd, "alias", name="ua-sbcap")
cl = ET.SubElement(dom, f"{{{QNS}}}commandline")
ET.SubElement(cl, f"{{{QNS}}}env", name="SBCAP_TRACE_FILE", value=trace)
ET.SubElement(cl, f"{{{QNS}}}env", name="SBCAP_DEVICE", value=pci)
if dma_dir:
    ET.SubElement(cl, f"{{{QNS}}}env", name="SBCAP_DMA_DIR", value=dma_dir)
# x-no-mmap: every BAR access goes through QEMU, so the patched QEMU sees it.
ov = ET.SubElement(dom, f"{{{QNS}}}override")
d = ET.SubElement(ov, f"{{{QNS}}}device", alias="ua-sbcap")
fe = ET.SubElement(d, f"{{{QNS}}}frontend")
ET.SubElement(fe, f"{{{QNS}}}property", name="x-no-mmap", type="bool", value="true")
tree.write(sys.stdout.buffer, encoding="utf-8", xml_declaration=False)
