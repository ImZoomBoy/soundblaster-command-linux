# ZxR fixes: optional driver patch

An optional patch for the Linux `snd-hda-codec-ca0132` driver, for the Sound Blaster ZxR. The app already works
around the main problem on the stock driver, so most people don't need it.

## What it fixes

- **Centre, sub or rear right silent after the card starts.** After every start of the card's sound processor
  (boot, wake from sleep, a driver reload), the processor can miss the start-up state of the centre/LFE (node
  `0x03`) and surround (node `0x04`) converter amps. It then keeps any mix of the centre, LFE and rear right
  silent: their processor outputs are exact zeros, while the hardware amp registers read back correctly.
  - The processor only applies those amps when they change. The driver's amp cache skips writes of an unchanged
    value, so nothing later puts them right. Even a direct same-value write doesn't help; a mute and unmute does.
  - The patch mutes and restores those amps 1.5 seconds after start-up (`zxr_wake_outputs`,
    `zxr_wake_delay_ms`). At the end of `ca0132_init()` it's too early and has no effect.
  - Measured with `capture/dsp-probe.sh`, which records the processor's outputs directly:
    - without the fix: all six channels came up in about 1 in 4 starts
    - with the fix, delayed: 8 of 8
    - with the fix at the end of start-up: 1 of 8
  - A power-off only seemed to help. The outcome is random on every start, and once broken it tends to stay
    broken across reloads.
- **Bass redirection crossover**, three bugs in the stock driver:
  - `ca0132_alt_surround_set_bass_redirection()` loads the X-Bass crossover (`xbass_xover_freq`) instead of
    `bass_redirect_xover_freq`. It runs on every output setup, including the effects master switch.
  - The "Bass Redirection Crossover" control comes out 10 Hz high: value 14 gives 150 Hz instead of 140. It uses
    `lookup[idx]` where the X-Bass slider correctly uses `lookup[idx - 1]`.
  - At the control's top value (100), it reads one past the end of the 100-entry table.

The driver logs one line about 1.5 seconds after each start, which shows the fix is active:

```
snd_hda_codec_ca0132 hdaudioC1D1: ZxR: centre/LFE/surround outputs re-sent
```

## Options

`options snd_hda_codec_ca0132 NAME=VALUE` in `/etc/modprobe.d/`, or at runtime in
`/sys/module/snd_hda_codec_ca0132/parameters/`:

| Option | Default | |
|---|---|---|
| `zxr_wake_outputs` | on | the delayed amp re-send above |
| `zxr_wake_delay_ms` | 1500 | how long after start-up |

The crossover fixes have no option. Only the ZxR is affected by the re-send; other CA0132 cards behave as before.

## Files

- `zxr-fixes.patch`: the fixes above, against the mainline `sound/hda/codecs/ca0132.c`.
- `dev/zxr-debug.patch`: test-only, applied on top by `sh driver/build.sh --dev`. It adds the temporary root-only
  `zxr_debug` command hook used by the probe tools in `capture/`, a surround DAC setup check, and the experiments
  from comparing with Creative's Windows driver. All the experiments are off by default:
  - `zxr_windows_mmio`: Windows' ca0113 start-up values
  - `zxr_desktop_fw`: load `ctefx-desktop.bin`
  - `zxr_skip_exit`: skip the shutdown sequence
  - `zxr_eapd_fix`: set the real EAPD bit
  - `zxr_dsp_settle_ms`, `zxr_stream_gap_ms`: start-up timing. Longer waits made more outputs go dead, and no
    setting fixed it.

  Don't use it day to day.

## Other upstream bugs noticed

- `ca0132_alt_select_out()` writes `0x01` (`AC_EAPDBTL_BALANCED`) under the comment "Enable EAPD". The EAPD bit
  is `0x02`, which Creative's Windows driver writes. It didn't affect the silent channels here.
- The ZxR isn't in the `DESKTOP_EFX_FILE` case of `ca0132_download_dsp_images()`, so it loads the generic
  `ctefx.bin`. That file's program segments match the desktop file, but it lacks two overlay segments that
  Windows loads.

## Build and install

```sh
sh driver/build.sh              # downloads the matching Linux driver source, patches and builds it (no root)
sudo sh driver/install.sh       # installs it and reloads the card (sound stops for a few seconds)
sudo sh driver/install.sh --uninstall
```

- Needs your kernel's headers (`linux-cachyos-headers` and so on). Clang-built kernels are detected.
- The module goes into `/lib/modules/<kernel>/updates/`. The original driver file isn't touched, and the next
  kernel update stops using the patched one automatically.
- The module isn't signed, so the kernel logs "tainting kernel". That only matters if your kernel enforces
  module signatures (Secure Boot with `module.sig_enforce=1`), in which case it won't load.
- `build.sh` uses the driver source from the mainline tag for your kernel's major.minor version (for example
  `v7.3`). Distribution kernels very rarely change this driver.

## License

The patch modifies Linux kernel code, so it is GPL-2.0-only, like the kernel. The rest of this repository is
GPL-3.0-or-later.
