# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

## What this is

- Sound Blaster Command for Linux: a GTK 4 / libadwaita control panel (Rust 2024) for the Creative Sound Blaster ZxR (CA0132), modelled on Creative's Windows app. Public at https://github.com/ImZoomBoy/soundblaster-command-linux, GPL-3.0-or-later (except `driver/`, which is GPL-2.0-only kernel code).
- The developer's machine: ZxR at PCI `0000:67:00.0` (`1102:0012`, subsystem `1102:0010`, ALSA card id `Creative`), Cambridge SoundWorks DTT2200 5.1 speakers, CachyOS (clang-built kernel), KDE Plasma, PipeWire + WirePlumber.
- Module map, features and install steps are in [README](README.md). The UI is the "N6 / Studio split" design: room card (left) with a now-playing title and capsule; output, SBX profile and EQ cards (right); Mixer, CrystalVoice, Speaker setup and Settings as side panels.

## Build and test

- `cargo build --release`, `cargo test` (unit tests), then the GUI self-test, always inside the isolated screen: `dev/isolated-kwin.sh "$PWD/target/release/sbcommand selftest"` (23 checks; prints `INFO first frame after N ms` separately from freeze checks).
- Screenshots for `docs/screenshots`: `dev/screenshots.sh` (dry run, fictional demo media from `assets/demo`, rebuilds `hero.png`).
- `SBCOMMAND_DRYRUN=1` prints card writes instead of making them; `SBCOMMAND_CONFIG_DIR` points settings elsewhere; `SBCOMMAND_SHOT=file.png`, `SBCOMMAND_SHOT_SIZE=WxH`, `SBCOMMAND_SHOT_PATH`, `SBCOMMAND_DEMO_MEDIA=music|movie|game`, `SBCOMMAND_DEMO_ART` drive screenshots.
- Install for the user: `./install.sh` (binary to `~/.local/bin/sbcommand`, `sbcommand-watch` user service, desktop entry). After installing, restart `sbcommand-watch` and reopen the app; check that no PipeWire/WirePlumber restart happened (`systemctl --user show -p ActiveEnterTimestamp pipewire pipewire-pulse wireplumber`).
- `GSK_RENDERER` must be `gl` (the old `ngl` name caused a 455 ms freeze). The test screen uses the NVIDIA GPU, so the first frame takes about 1.3 s there; that is not an app freeze.

## Safety rules (from incidents; do not relax)

- Never start a test KWin (or any Qt/KDE session process) on the user's session bus or with their config folders. It took over their keyboard shortcuts once. Use `dev/isolated-kwin.sh` only, and afterwards check that `~/.config/kglobalshortcutsrc` and `~/.config/kwinrc` mtimes are unchanged. Never screenshot the real desktop.
- Never play sound on the real speakers without the user's go-ahead. A load test once played a tone on them. Prefer silent checks: `dev/where-is-each-channel.sh` (null sink) and reading the card sink's `.monitor` with `parec`.
- Never restart `pipewire`, `pipewire-pulse` or `wireplumber` without asking (it cuts browser audio). The app only rewrites their config files when the text changes.
- Card resets (`systemctl start soundblaster-reset-now.service`) cut sound for a few seconds; say so first.
- Commit or push only when asked. Commits use the user's GitHub no-reply identity.

## Hardware and audio facts

- 5.1 channel order: the ZxR's DSP takes Windows order, so the 5.1 sink is `audio.position = FL,FR,FC,LFE,RL,RR` (WirePlumber rule written by the app). The driver's own chmap says the same. Do not change it.
- ZxR outputs: front L/R on the two RCA sockets; rear L+R on one 3.5 mm socket (DTT2200 black plug); centre+sub on another 3.5 mm socket (orange plug). The DTT2200 subwoofer unit splits them to separate satellite jacks.
- The driver forces SBX X-Bass off whenever a subwoofer is configured, so the app applies Bass through the card EQ. Bass redirection sends every channel's bass to the sub: a 60 Hz tone on the front-left channel coming out of the sub proves the sub path works.
- **Silent centre/sub/rear right after a card start (root cause found 2026-10-27):** after each DSP start (boot, wake, reload, reset) the DSP can miss the start-up state of converter amps 0x03 (centre L / LFE R) and 0x04 (surround L/R) and output exact zeros on any mix of FC, LFE and RR. It only applies those amps on a change, and the HDA amp cache skips unchanged writes. Fixes: the app's `card::wake_surround()` toggles Center/LFE/Surround Playback Switch off and on after every card appearance (watch loop), and the driver patch's `zxr_wake_outputs` mutes and restores the amps 1.5 s after start-up (end of `ca0132_init()` is too early). Each alone gave 8/8 good starts. It is random per start, not warm-vs-cold: a power-off only seemed to help. A card reset now brings a silent speaker back; don't tell the user to power off.
- To check the DSP outputs without listening: `capture/dsp-probe.sh` (needs `capture/debug-helper.sh`; `QUICK=1` gives a one-line verdict). It plays six quiet tones and records each DSP output through the What U Hear router entries. Its readings matched the user's ears.
- Speaker tests must be multichannel files that are silent except on one channel (`system::spread_to_channel`). A one-channel "FC" stream gets copied to FL+FR by PipeWire, which sounds like a centre speaker (phantom centre) even when the centre is dead; that hid a silent centre for days. Stock ALSA voices are quiet, so tests are normalised to -1 dBFS. For the sub, steady 60-80 Hz tones are audible; the rumble file is fainter.
- `speaker-test -s N` does not follow the card's chmap; use `aplay` with a prepared 6-channel file instead.
- `driver/zxr-fixes.patch` is the product patch for `snd-hda-codec-ca0132`: the delayed amp re-send above (per-card delayed work, cancelled on suspend/remove, re-armed on resume) and three bass-redirection crossover bugs (wrong variable, 10 Hz off by one, out-of-bounds at 100). `driver/dev/zxr-debug.patch` (test-only, `sh driver/build.sh --dev`) adds the root-only `zxr_debug` hook that `capture/` needs, plus experiments (off by default). Regenerate patches with `diff -u` against `driver/build/ca0132.c.orig`, and check that `zxr-fixes.patch` then `dev/zxr-debug.patch` apply in order. See [driver/README](driver/README.md).
- The stock driver reloads the X-Bass crossover into the card whenever it redoes the output setup (speaker setup, Output Select, the effects master switch), so the app writes the crossover last in every batch with a neighbouring value first (`model::crossover_pairs`).

## Sharp edges in the code

- `room::SPEAKERS` is ordered FL,FC,FR,LFE,RL,RR; `meters::ORDER` is FL,FR,FC,LFE,RL,RR. Map between them with `ch_index`.
- "Stereo plays on" (`stereo_on`: front | rear | all) is applied per stream with `pw-cli set-param ... Props` channelmix params (`simple` upmix; `fc-cutoff` 0 = rears only, 100000 = rears and centre; PipeWire's centre low-pass sits at about half the value, so 12000 muffled the centre and 48000+ is flat). `spread_streams` skips nodes named `sbcommand*` except `sbcommand.relabel.out`; the speaker test stream is named `sbcommand.test` so it is never changed mid-play.
- Pure = SBX off: the five SBX effects (Bass and Scout included) are off and their knobs grey out; the equaliser stays on, as in Creative's app where SBX and the EQ are separate switches (user's choice, 2026-10-27).
- The tray (ksni) runs in the `watch` process and may fail to register in the first seconds after login; the watch loop retries.
- The Spotify Flatpak labels its stereo `aux0,aux1`; `relabel_aux_streams` moves such streams through the `sbcommand.relabel` loopback so they can be placed.
