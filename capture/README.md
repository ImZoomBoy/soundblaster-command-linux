# Capturing the Windows driver's start-up

Goal: record exactly what Creative's Windows driver sends the ZxR when it starts the card, and compare it with
the Linux driver (`driver/`). This was how the search for the silent centre/sub/rear-right channels started. The
cause turned out to be the DSP missing the start-up state of two converter amps (see `driver/README.md`), found
with the probe tools below rather than with the Windows capture.

## Probe tools (Linux, through the debug helper)

`sudo bash capture/debug-helper.sh` installs the patched driver with its temporary `zxr_debug` hook and a
password-free `sbcap-cmd@` unit for whitelisted commands. `--uninstall` removes the helper.

| Script | What it does |
|---|---|
| `capture/dsp-probe.sh NAME` | Plays six quiet tones (one pitch per channel) and records each DSP output by pointing the "What U Hear" router entries at it; prints which input reaches which output. `QUICK=1` checks c0-c5 and prints one line |
| `capture/snapshot.sh NAME` | Serial-port/clock registers, audio router, 8051 PLL/PMU registers and XRAM tables |
| `capture/dsp-snapshot.sh NAME` | All DSP memory (0x000000-0x0bffff) |
| `capture/scp-snapshot.sh NAME` | DSP parameters of modules 0x8f and 0x96 (not 0x80: some of its "gets" allocate DMA channels) |

Output lands in `capture/build/` (not committed).

## How it works

- The card is passed into the `win11` libvirt VM with VFIO and `x-no-mmap`, so every access to its memory
  regions goes through QEMU.
- `qemu-sbcap-trace.patch` (QEMU 11.1.2, `hw/vfio/region.c`) logs every BAR read and write of the device. It also
  logs the HD-audio commands the driver puts in the CORB ring and the card's replies in the RIRB ring, which
  live in guest memory and are otherwise invisible. It is switched on by `SBCAP_TRACE_FILE` and `SBCAP_DEVICE`.
- The VM runs on a throwaway qcow2 layer, so its real disk (the CI runner) is never changed.

## Steps

1. `sudo bash packaging/root/boot-and-sleep-fix.sh --capture`, then restart. This adds
   `pcie_acs_override=downstream,multifunction`, so the card gets its own IOMMU group and the network, USB and
   disks in its old group stay with Linux.
2. Build the patched QEMU (no root). It installs to `/var/tmp/sbcap-qemu`, where libvirt's QEMU user can run it:

   ```sh
   cd capture/build && tar -xJf qemu-11.1.2.tar.xz && cd qemu-11.1.2
   patch -p0 hw/vfio/region.c < ../../qemu-sbcap-trace.patch
   python3 -m venv ../venv && ../venv/bin/pip install ninja && PATH=$PWD/../venv/bin:$PATH \
   ./configure --target-list=x86_64-softmmu --prefix=/var/tmp/sbcap-qemu --enable-kvm --enable-spice \
     --enable-usb-redir --enable-libusb --enable-vhost-net --enable-tpm --disable-docs --disable-werror \
     --disable-gtk --disable-sdl --disable-opengl && make -j20 && make install
   ```

3. Optional, before the VM capture: `sudo sh capture/linux-trace.sh` records the Linux driver restarting the card
   (kernel hda tracepoints).
4. `capture/vm-capture-start.sh`: shuts the runner down, starts the VM with the card and logging.
5. `capture/vm-guest.py install-driver`: copies the Microsoft-signed Creative driver (Update Catalog,
   "CREATIVE - MEDIA - 6.0.102.75", `ctHda.inf`) into Windows and installs it. Further commands run with
   `vm-guest.py run "<PowerShell>"`.
6. `capture/vm-capture-stop.sh`: shuts the VM down, which also logs the driver's shutdown, restores the VM's
   original settings and starts the runner again.
7. Restart the PC before using the card on Linux again.
8. `sudo bash packaging/root/boot-and-sleep-fix.sh --end-capture` removes the ACS override again.

Logs land in `/var/tmp/sbcap/`.

## License

`qemu-sbcap-trace.patch` modifies QEMU, so it is GPL-2.0-or-later like QEMU. The scripts here are
GPL-3.0-or-later like the rest of the repository.
