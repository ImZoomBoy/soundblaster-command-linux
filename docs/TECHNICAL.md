# Sound Blaster Command for Linux: technical details

The [README](../README.md) is the short manual. This page has the details: how the app works inside, every sound setting, what it fixes in the Linux driver, supported systems, the command line, privacy in detail, and notes for developers.

## Contents

1. [How the app works](#1-how-the-app-works)
2. [Install in detail](#2-install-in-detail)
3. [The sound choices in full](#3-the-sound-choices-in-full)
4. [Sound reference](#4-sound-reference)
5. [What it fixes on Linux](#5-what-it-fixes-on-linux)
6. [Supported cards and systems](#6-supported-cards-and-systems)
7. [Command line and shortcuts](#7-command-line-and-shortcuts)
8. [Privacy in detail](#8-privacy-in-detail)
9. [For developers](#9-for-developers)

## 1. How the app works

### The window, the background service and the tray

The app comes in three parts:

| Part | What it does |
|---|---|
| **The window** (`sbcommand`) | Shows what's playing and every setting. Closing it closes only the window; the card keeps its settings. |
| **The background service** (`sbcommand-watch`) | A systemd user service started at login. It sends your saved settings to the card at login and whenever the card comes back (after sleep, a reset or a driver reload), switches the sound when something new starts playing, even with the window closed, and runs the tray icon. |
| **The tray icon** | The common switches without opening the window. Click it to open the window. |

Your settings are kept in `~/.config/sbcommand/settings.json`. Pictures and details looked up online are
kept in `~/.cache/sbcommand`.

### Knowing what's playing

The app watches three things, without recording anything:

- the audio streams PipeWire is playing, and which app each one belongs to;
- the media players that say what they're playing (MPRIS: Spotify, browsers, Jellyfin, VLC and most others),
  which give the title, the artist, the album, the cover and the stream's format;
- game processes: Steam, Proton and Wine games are recognised by their process, and Steam games by their
  Steam number.

From the player, the title and the details it looks up, it decides what kind of thing is playing: a studio
song, a live recording from a small venue or a stadium, a film, an episode of a series, a game, a match
or race, an online video, or a bare stream. Each kind gets its own page ([The main page](../README.md#3-the-main-window))
and its own sound.

### Each kind of content keeps its own sound

There are seven kinds: **Music**, **Films**, **TV**, **Games**, **Sport**, **Online video** and **Streams**.
Each keeps its own SBX profile, equaliser, tone, where stereo plays, and SBX on or off. When you change the
sound while something plays, it's remembered for that kind.

When something new starts playing, the app picks its sound in this order:

1. the sound for its kind;
2. if you've used that app or game before, your last profile and equaliser for it (**Remember settings per
   game and app**), with an Undo;
3. for 14 games, Creative's own profile for that game ([Game profiles](#game-profiles)), unless you've chosen
   your own for it.

For music, the equaliser can also follow each song's genre (**Match the EQ to the song's genre**), so rock gets
Creative's Rock and hip hop gets Hip Hop.

Only your own changes are remembered, never the automatic switches. **Settings → When something starts
playing** decides what happens: **Switch automatically**, **Suggest a switch** (it offers the switch and you
press it), or **Do nothing**.

### Plain words for Creative's settings

Creative's app shows knobs and profile names. The page shows four choices in plain words instead: *in front of
you*, *close, like the studio*, *a little warmer*, *clearer voices*. Each one sets the real thing (an SBX
profile, an effect, the equaliser), and the small print under each option says exactly what. The full controls,
knob by knob, are in the [control centre](../README.md#6-the-control-centre).

### Several apps at once

The card has one sound processor, so only one app at a time can set its SBX effects and equaliser. The app
picks which by rank: a game beats a film or video, which beats music; among equals, the newest wins.

Every other app still gets its own sound: an equaliser inside its own stream, worked out so that after the
card's equaliser it hears its own kind's curve, and its own *stereo plays on*. So music playing beside a game keeps its music sound instead of sounding
like the game. While music plays beside a game, the card's effects are eased for both: Surround halved and Smart
Volume off (unless Scout Mode is on). Games and players start and stop their streams often, so this is held
steady: it starts once both have played for 4 seconds and ends 10 seconds after one stops.

**Settings → Each app its own sound** turns this off.

### When the card restarts

After sleep, a card reset, a driver reload or a reboot, the Linux driver puts the card back to its defaults
(2.0 speakers, every effect on). The background service puts your settings back within a few seconds. It also
switches the centre, subwoofer and surround outputs off and on again, because the card's processor sometimes
misses their start-up state and stays silent on them ([What it fixes on Linux](#5-what-it-fixes-on-linux)).

## 2. Install in detail

### Requirements

- Linux with **PipeWire**, **WirePlumber** and **pipewire-pulse** (for `pactl` and `parec`)
- **GTK 4.14+** and **libadwaita 1.6+**
- **Rust** (edition 2024, so 1.85 or newer) to build
- systemd (user services, `busctl`), `curl`; optional: `notify-send`, a StatusNotifierItem tray (KDE Plasma, or the AppIndicator extension on GNOME)
- Optional, for game trailers: `ffmpeg` (saves Steam's trailer streams once) and the GStreamer plugins GTK plays
  video with (`gst-plugins-good`, `gst-libav`). Without them the pictures still show.
- A **Creative Sound Blaster ZxR**; see [Supported cards and systems](#6-supported-cards-and-systems)

`./install.sh` builds the app, installs it to `~/.local/bin/sbcommand` with a menu entry and icon, and starts the
`sbcommand-watch` user service. The first run writes small PipeWire/WirePlumber config files, so sound may stop
for a second once while they load. `./install.sh --uninstall` removes it and keeps `~/.config/sbcommand`.

### Optional extras

- **A password-free Reset button** (and a sleep hook that is off by default), with sudo:

  ```sh
  sudo packaging/root/install-root-helpers.sh
  ```

- **If sound or video stalls after waking from sleep,** run `sudo bash packaging/root/boot-and-sleep-fix.sh` and
  restart. It adds `iommu=pt` to the kernel command line, which stopped the card's IOMMU errors after sleep.
  It does this automatically on Limine (CachyOS's default boot loader) and prints the steps for GRUB and
  systemd-boot.
- **The [driver patch](../driver/README.md)** fixes the same driver bugs inside the driver: `sh driver/build.sh`
  (downloads and builds the matching driver source, no sudo), then `sudo sh driver/install.sh`. Sound stops for
  a few seconds while the card reloads. You don't need it: the app works around the same problems on the stock
  driver.

## 3. The sound choices in full

Each choice opens a short list in plain words. The small print under each option says what it sets. The names
change with what's playing:

| Choice | Music | Films and TV | Games | Sport |
|---|---|---|---|---|
| [Where](#where) | Where | Seat | Speakers | Crowd |
| [Room](#room) | Room (Venue when live) | Venue | Mode | Atmosphere |
| [Tone](#tone) | Tone | Mix | Tone | Tone |
| [Extras](#extras) | Extras | Extras | Extras | Commentary |

### Where

Which speakers play.

| Option | What it does |
|---|---|
| *in front of you* | Stereo on the front speakers (and the subwoofer), as mixed |
| *all around you* | Stereo on the front and back speakers ([Stereo plays on](#stereo-plays-on)) |
| *on headphones* / *on the speakers* | Switches to the headphone socket or back. It always asks first, so you never switch by accident |

A 5.1 source shows *all six speakers*: it plays on every speaker by itself, so there's nothing to choose.

### Room

The SBX profile, in words. The list offers the profiles made for what's playing, any profiles of your own, and
*untouched*.

| Words | SBX profile |
|---|---|
| *close, like the studio* | Music (this app's own, for studio albums) |
| *like a live show*, *like the concert hall* | Concert |
| *like a stadium* | Arena for music (this app's own), Stadium for sport |
| *in the stands* | Stadium (sport) |
| *like a cinema*, *like a big screen* | Cinema |
| *quietly, for late at night* | Night |
| *wide, to place every sound* | Gaming |
| *tight, to hear every footstep* | FPS |
| *fast, for racing* | Racing |
| *rich, for role-playing* | RPG |
| *clear, for strategy* | Strategy |
| *big, for adventures* | Adventure |
| *made for Fortnite* (and other games) | Creative's profile for that game |
| *untouched* | SBX off: the sound as it was made ([Pure](#pure)) |

### Tone

Five steps: *much warmer*, *a little warmer*, *as mixed*, *a little brighter*, *much brighter*. Tone is a tilt
added on top of the equaliser preset: warmer gives more low end and a softer top, brighter a crisper top and less
low end. *As mixed* is the preset alone.

### Extras

Switches you can combine, each shown as on or off:

| Option | Sets |
|---|---|
| *clearer voices* (*clearer commentary* for sport) | Dialog+ |
| *more bass* | SBX Bass ([Bass and the subwoofer](#bass-and-the-subwoofer)) |
| *hear footsteps sooner* (games only) | Scout Mode |

## 4. Sound reference

### SBX effects

| Effect | What it does |
|---|---|
| **Surround** | Widens the sound around you |
| **Crystalizer** | Restores the punch and detail that compression takes out |
| **Bass** | Adds low end, below its crossover ([Bass and the subwoofer](#bass-and-the-subwoofer)) |
| **Smart Volume** | Evens out loud and quiet parts (Normal, Loud or Night) |
| **Dialog+** | Brings voices forward |

Each has an on/off switch and a level, in **Sound** in the control centre.

### SBX profiles

Creative's profiles, with the exact values from Creative's Sound Blaster Command for Windows (separate speaker
and headphone values where Creative has them), plus two of this app's own:

| For | Profiles |
|---|---|
| Music | **Music** (this app's own, for studio albums), **Concert**, **Arena** (this app's own: a stadium gig, wide with big bass and nothing squeezed) |
| Films, TV, video | **Cinema**, **Night** |
| Sport | **Stadium** (made for watching sport: Smart Volume and Dialog+ for the commentary), **Cinema**, **Night** |
| Games | **Gaming**, **FPS**, **Racing**, **RPG**, **Strategy**, **Adventure** |

You can make your own, rename and delete them, and export or import them as files.

### Game profiles

Creative's 14 game profiles, each with its own equaliser, are picked by themselves when that game runs: Apex
Legends, Arena of Valor, Battlefield 1, CoD: Infinite Warfare, Counter-Strike, Dota 2, Fortnite, League of Legends,
Metal Gear Solid V, Overwatch, PUBG, Project CARS, Rocket League and The Witcher 3. If you've chosen your own
sound for a game, yours wins.

### Equaliser presets

Ten bands from 31 Hz to 16 kHz. Creative's presets: **Flat**, **Acoustic**, **Classical**, **Country**,
**Dance**, **Hip Hop**, **Jazz**, **Pop**, **R&B**, **Rock**, **Vocal**, **Gaming**, **Cinema**, **Concert** and
**Night**, plus **Rock (classic)**, the Linux driver's older Rock. Drag any band to change it; **Revert** puts a
preset back to Creative's values, and your own presets can be made, exported and imported.

### Pure

<img src="screenshots/pure.png" alt="Pure: SBX off, the equaliser on" width="100%">

Pure is SBX off: the five SBX effects (Bass and Scout Mode included) are off and their knobs grey out. The
equaliser stays on, as in Creative's own app, where SBX and the equaliser are separate switches. On the page,
Pure is the Room choice *untouched*.

### Stereo plays on

Each source plays on the speakers it was made for, by itself: a stereo song on the fronts and the subwoofer, a
5.1 film, game or track on all six. **Stereo plays on** decides only what stereo does:

- **Fronts:** the front speakers only, exactly as mixed.
- **+ Rears:** the rear speakers get a copy too, 3 dB quieter. The centre stays silent, because PipeWire's
  centre fill sounds hollow on stereo recordings.

### Bass and the subwoofer

The Linux driver turns SBX Bass off whenever your speakers have a subwoofer, so the app applies Bass through the
card's own equaliser instead (up to +8 dB). With **bass redirection** on, every speaker's bass goes to the
subwoofer, and Bass reaches everything the subwoofer plays (up to the speakers' crossover: 140 Hz for the
DTT2200), not just Creative's 80 Hz.

## 5. What it fixes on Linux

These are real problems with the ZxR on a stock Linux system, found while building this:

| Problem | What this app does |
|---|---|
| **Fuzzy, distorted sound at high volume.** The driver puts the card's Front/Surround/Centre/LFE sliders at +9 dB and PipeWire drives them as hardware volume, so the card clips. | Pins the card's own sliders at 0 dB and lets PipeWire do volume in software (WirePlumber `api.alsa.soft-mixer`). **Speaker boost** lets you deliberately go up to +9 dB again if you want Windows-level loudness. |
| **5.1 speakers in the wrong places.** The ZxR's DSP expects Windows channel order. | Sets the 5.1 output to `FL,FR,FC,LFE,RL,RR`, so every channel reaches the right speaker. |
| **SBX Bass does nothing on 5.1 or 2.1.** In `ca0132_effects_set` the driver forces X-Bass off whenever the speakers have a subwoofer. | Applies the Bass knob through the card's own equaliser instead, and bass redirection sends it to the subwoofer. The EQ graph shows it as a dashed line. |
| **Spotify (Flatpak) never reaches the other speakers.** Its stream is labelled `aux0,aux1`, which PipeWire won't place. | Re-labels such streams so they play on the front speakers like any stereo source. |
| **Centre, sub or rear right silent after a boot, sleep or reset.** The card's sound processor sometimes misses the start-up state of those outputs, and the driver never sends it again. Only a lucky restart brought them back. | Switches those outputs off and on again a couple of seconds after the card starts, so the processor picks them up (all six channels in 8 of 8 test restarts). The [driver patch](../driver/README.md) does the same inside the driver. |
| **Subwoofer crossover wrong.** The driver replaces the bass redirection crossover with the X-Bass one whenever it redoes the speaker setup, so the sub got less bass than set. | Re-sends the crossover after every change. The driver patch fixes the bug itself, and two smaller ones in the same code. |
| **Settings lost after sleep or a card reset.** The driver resets the card to defaults (2.0, every effect on). | The background service puts your settings back whenever the card comes back. Optional root helpers add a password-free **Reset card** button. |
| **Effects and EQ are a wall of ALSA controls.** | Creative's profiles and presets with their exact values, read from Creative's own Sound Blaster Command data files. |

## 6. Supported cards and systems

| Sound card | Status |
|---|---|
| **Sound Blaster ZxR** (Sound Core3D / CA0132, PCI `1102:0012`, main codec subsystem `1102:0033`, with the DBpro daughterboard) | **Supported and tested**: every feature, 2.0 to 5.1 speakers and headphones. Developed with Cambridge SoundWorks DTT2200 5.1 speakers. |
| Sound Blaster Z / Zx, AE-5, AE-5 Plus, AE-7, Recon3D (other PCIe CA0132 cards) | **Untested.** The app finds them and skips controls a card doesn't have, so the basics may work; ZxR-only parts (DBpro input, 600 Ω headphone gain) won't. Reports are welcome. |
| Recon3Di motherboard audio, USB Sound Blasters (G3, G6, X3, X4 ...) | **Not supported.** |

| System | Status |
|---|---|
| **Linux** with PipeWire, WirePlumber and pipewire-pulse, GTK 4.14+ and libadwaita 1.6+ | **Supported.** Developed and tested on CachyOS (Arch-based), Linux 7.3, KDE Plasma 6 on Wayland. Current Arch, Fedora 41+, Ubuntu 24.10+ and Debian 13+ have the right library versions but are untested. Works on GNOME too; the tray icon needs StatusNotifierItem support (the AppIndicator extension on GNOME), and the Strip's pin is KDE only. |
| Linux with PulseAudio only, or Ubuntu 24.04 and older (libadwaita 1.5) | **Not supported.** The app configures PipeWire and WirePlumber. |
| Windows, macOS | **No.** On Windows, use Creative's own Sound Blaster Command. |
| **Optional driver patch** ([`driver/`](../driver/README.md)) | Linux **6.17 or newer** (the driver moved to `sound/hda/codecs/` in 6.17), tested on 7.3. Needs your kernel's headers. The module is unsigned, so it won't load where Secure Boot enforces module signatures. |

## 7. Command line and shortcuts

```sh
sbcommand                # the window
sbcommand sound          # open straight to a tab: sound, mixer, crystalvoice, setup, look, settings
sbcommand speaker-test   # open the speaker test
sbcommand apply          # send the saved settings to the card (what the service does at login)
sbcommand detect         # read-only: what's playing and what the app would pick
sbcommand watch          # the background service (normally started by systemd)
sbcommand selftest       # drive every control in a dry run (no card, temporary settings)
```

| Key | Does |
|---|---|
| Esc | Closes the control centre, the speaker test or full screen |
| Ctrl+R | Fix fuzzy sound: sends every setting to the card again |
| ← → | In full screen, the previous or next picture; on a focused knob, turn it |
| ↑ ↓ | On a focused knob, turn it |

## 8. Privacy in detail

- **No AI, no telemetry, no accounts.** Everything runs locally.
- For the page's pictures and details, only names (the artist, the album, the game, the film) and Steam's game
  number are sent. Everything fetched is kept under `~/.cache/sbcommand`, so each one is asked about once
  (renewed after 30 days):
  - **MusicBrainz**: the artist's genre, to match the Music EQ (can be switched off), and the artist's other
    albums for the record shelf, with covers from the **Cover Art Archive**;
  - **TheAudioDB**: artist photos, logo and a short bio;
  - **Steam**: a game's art from Steam's own files first, then its store page (screenshots, trailer). Trailers are
    saved once with `ffmpeg` (the newest 12 are kept) and play **muted** on the monitor; sound only in full
    screen or when you turn it on;
  - **TMDB**: film and series pictures, only if you enter your own free API key;
  - your **Playnite** library folder, if you set one (read from disk, no network);
  - the **cover art** the player itself reports.
- **Settings → Look up pictures and details online** switches the page's lookups off; it then uses the player's
  own cover art. The genre lookup has its own switch.
- The **level meters** only *read* the card's output monitor while the window is open. Nothing is recorded or played.
- The app never restarts PipeWire, pipewire-pulse or WirePlumber during normal use (that would cut sound
  in your browser). It only writes their config files when a setting really needs it.
- Card controls are written directly through ALSA; anything slow runs on a background thread, so the window
  never freezes.

## 9. For developers

### How it's built

Rust 2024, GTK 4 and libadwaita. The rooms are drawn with cairo, and the fonts (Anton, Archivo Narrow and Archivo)
are bundled for this app only.

| Module | Does |
|---|---|
| `model.rs` | Settings, Creative's SBX profiles and EQ presets (exact values from Sound Blaster Command 3.4.98), game profiles, each kind's sound, the ALSA control values for each setting, Pure, Speaker boost, per-app memory |
| `card.rs` | Writes the card's ALSA controls and hardware levels |
| `system.rs` | PipeWire / WirePlumber: 5.1 channel order, soft-mixer, extra outputs (2.1, stereo), stream re-labelling, stereo on the rears and each app's own EQ (per stream), volume, test sounds |
| `worker.rs` | Background thread for all card and audio work; the `watch` service |
| `detect.rs` | Tells games, films and TV, and music apart from the playing streams, MPRIS players and Steam/Proton/Wine processes; which app sets the card when several play |
| `media.rs` | Now playing: MPRIS metadata, stream format from PipeWire, Steam game names and artwork |
| `genre.rs` | Song genre → Creative EQ preset (rules, no AI), with MusicBrainz lookups |
| `meters.rs` | Live per-speaker levels from the card's monitor (`parec`) |
| `meta.rs` | What kind of thing is playing (studio song, live recording, film, series, game, match, web video) and its pictures and details: TheAudioDB, Steam, MusicBrainz and the Cover Art Archive, TMDB, Playnite; cached |
| `art.rs` | Picture processing: the page-red print, auto levels for dark game art, darkening |
| `stage.rs` | The page's layers: background pictures, the room, the title block, the corner, the cover, the sound choices (slim dock, rows, bar, half-screen tiles), the controls, the monitor video; sizes for every window |
| `ui/page.rs` | The page's behaviour: what's playing → title, credits, corner and choices; the monitor and full screen; Look and feel; which view the window's size shows |
| `ui/speaker_test.rs` | The full-screen speaker test |
| `oys.rs` | The Over your shoulder room |
| `themes.rs` | The Blueprint, Screenprint and Callouts rooms |
| `fonts.rs` | The bundled fonts |
| `mini.rs` | The compact views: the Remote, Over your shoulder, mini, and the Strip with its pin (a KWin script) |
| `room.rs`, `dash.rs` | The speakers as real cabinets with LED meters (the Remote's room, the Strip's row), knobs, the EQ curve and signal-path dots |
| `ui.rs` | The GTK 4 / libadwaita window and the control centre |
| `tray.rs` | The tray icon (StatusNotifierItem via `ksni`) |
| `selftest.rs` | `sbcommand selftest`: clicks through every control and checks the settings follow |

### Building and testing

```sh
cargo test                # unit tests (model, detection, genres, media, layouts)
cargo build --release
dev/isolated-kwin.sh "$PWD/target/release/sbcommand selftest"   # 38 checks, in a private test screen
```

The self-test drives every control in a dry run (it never touches the card or your settings), checks each theme's
half-screen layout and the Strip at several sizes, and fails on any GTK warning.

`SBCOMMAND_DRYRUN=1` prints card writes instead of making them; `SBCOMMAND_CONFIG_DIR` points settings
elsewhere. A dry run looks nothing up online; `SBCOMMAND_META_LIVE=1` allows the picture lookups anyway (and a
muted trailer), with `SBCOMMAND_DEMO_TITLE`, `SBCOMMAND_DEMO_ARTIST`, `SBCOMMAND_DEMO_ALBUM` and
`SBCOMMAND_DEMO_NOART=1` to fake what's playing. `SBCOMMAND_TMDB_KEY` is another way to give a TMDB key.
Screenshots like the ones in the README come from the app itself (`SBCOMMAND_SHOT_DELAY` in ms,
`SBCOMMAND_SHOT_FULL=pictures|trailer` for the full-screen view):

```sh
SBCOMMAND_DRYRUN=1 SBCOMMAND_CONFIG_DIR=/tmp/sbc SBCOMMAND_DEMO_MEDIA=music \
SBCOMMAND_DEMO_ART=assets/demo/music.png SBCOMMAND_SHOT_SIZE=1280x820 \
SBCOMMAND_SHOT=shot.png ./target/release/sbcommand
```

### Helper scripts

| Script | Does |
|---|---|
| `dev/isolated-kwin.sh "<cmd>"` | Runs a GUI command in a private virtual KWin, apart from your desktop (use it for `selftest`) |
| `dev/screenshots.sh` | Retakes every screenshot in `docs/screenshots` |
| `dev/where-is-each-channel.sh` | Silent check that each of the six channels lands where it should in PipeWire |
| `sudo sh dev/pin-sense.sh` | Read-only: which output sockets the card detects a plug in, and how each output is set up |

[`capture/`](../capture/README.md) holds the tools that found the silent-channel bug:
- `capture/dsp-probe.sh` records each output of the card's sound processor and shows which channel reaches it,
  without listening. It needs the test driver (`sh driver/build.sh --dev`).
- The scripts that captured Creative's Windows driver starting the card, in a virtual machine.

The demo tracks, film and game (*Neon Rain*, *Starfall*, *Night Circuit*) and their artwork are made up for these screenshots.

Contributions are welcome, especially tests on other CA0132 cards (Sound Blaster Z, ZxR variants, Recon3D, AE-5).

