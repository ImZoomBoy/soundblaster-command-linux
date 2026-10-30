# Redesign build plan (from the prototype, rounds 53–101)

Prototype: `~/Downloads/sbcommand-designs/redesign-round101.html` (served at
http://127.0.0.1:8767/). Design notes: the agent memory file
`game-page-design-locked.md`. Branch: `redesign` (not committed: the user commits).

The user is away and asked for it to be fully built and working, with:
- the full "now playing" page (Over your shoulder is the default theme),
- a second, smaller **Compact** view built with the same ideas,
- better settings pages in the full theme,
- working at different screen sizes,
- a better app / taskbar icon.

## Architecture

- `src/fonts.rs`: Anton + Archivo Narrow bundled (OFL), registered for this process only (FcConfigAppFontAddFile).
- `src/art.rs`: picture processing with gdk-pixbuf: the page-red duotone, auto levels, darkening; cached textures.
- `src/meta.rs` (subagent A): content type (studio / live small room / live stadium / film / TV / game / sport / online video / stream) and pictures: TheAudioDB (artist photos, logo, bio), Steam (local librarycache + store API: hero, logo, screenshots, trailers), Playnite on the NAS, MPRIS art. Cached under ~/.cache/sbcommand/meta.
- `src/oys.rs` (subagent B): the Over your shoulder room, a cairo port of the prototype's canvas code.
- `src/stage.rs`: the page: gtk::Overlay with background (duotone slideshow), room, head (kicker, title/logo, artist, credits, bio, facts), big 5.1 or live venue tag top right, big cover bottom left, slim dock (SOUND + Where / Room / Tone / Extras), controls dock (play/pause, readout, headphones, volume, speaker test, menu ☰), game monitor video (gtk::MediaFile, one stream moved to full screen).
- Look and feel (in `src/ui/page.rs`, `build_look_page`): theme (Over your shoulder, Blueprint, Screenprint, Callouts, Compact), layout (Default, Slim dock, Slim dock big cover, Rows, Rows cover by title, Sport-style bar, Split page rows / slim dock), per-kind defaults, saved in Settings.
- The old dashboard cards (output, SBX profile, EQ) move into a **Sound** side panel; Mixer, CrystalVoice, Speaker setup and Settings stay as side panels, restyled.
- Responsive: AdwBreakpoint; below ~900 px wide or ~600 px high the window uses the Compact view.

## Defaults (user decisions)

Theme Over your shoulder; layout Slim dock + big cover (music, film, TV, game); Slim dock (sport, video, stream).
Game page: logo leads; official art then fan art in red behind the page (slow); monitor: logo 3 s, then the
trailer muted once, then pictures every 10 s (official art, screenshots, fan art); full screen opens on the pictures,
"Play the trailer · with sound"; one stream moves between monitor and full screen. Studio songs keep the studio room
whatever the sound; live songs follow the sound; live stages show the neon name only. "5.1 → 5.1" shows "5.1".
Bottom-right controls sit on a dock (bottom edge fades darker). No solid black boxes. The slim dock shares that bottom line with the controls at every width (round 80: one line, left of the controls); narrower windows shorten its words rather than moving it to a second line.

## Status

- [x] fonts, art, page skeleton (stage.rs, ui/page.rs), Sound side panel, CSS, responsive classes
- [x] meta.rs integrated (subagent A): Options from Settings (online, TMDB key, Playnite folder); SBCOMMAND_META_LIVE=1 allows lookups in a dry run (test screenshots)
- [x] oys.rs integrated (subagent B): pixel-matched port, ping ripple on tap
- [x] sound controls wired (Where / Room / Tone / Extras) + tone tilt in model::effective_eq
- [x] game monitor: logo 3 s, trailer (saved once with ffmpeg to ~/.cache/sbcommand/trailers, GTK can't stream HLS) muted, then pictures; hover buttons; full screen window
- [x] Look and feel panel + layouts + per-kind defaults (Settings.theme/layout/layout_defaults)
- [x] other themes (subagent C): Blueprint, Screenprint, Callouts in `themes.rs`; wall screens keep clear of the top-right corner (`Scene.corner`)
- [x] film / TV cinema screen like the game monitor: title 3 s, then stills (TMDB backdrops), trailer only on play
- [x] different window sizes: big cover fits the space under the title (or moves by it), bio / full-screen button give way, compact choices two to a line on short windows, mini plan above the choices; checked 480x820 to 1400x1000
- [x] Compact view (theme or < 900x600) + header breakpoints + min size 360x520
- [x] settings pages restyled; Settings gains Pictures and details (online, TMDB key, Playnite folder), The page, About
- [x] icon: own design (red speaker + halftone dots) + symbolic; install.sh installs both
- [x] film / TV ticket band (Admit one, screen, source, barcode, outlined 5.1) as in round 101; sport scoreboard on the room's screen
- [x] selftest updated (26 checks), screenshots regenerated (dev/screenshots.sh adds blueprint, screenprint, callouts, compact-small), README / AGENTS.md rewritten for the page

## Notes
- Screenshots: /tmp/r54/shot.sh out.png media art [WxH] [settings-json] [page] (isolated KWin, dry run).
- GUI selftest: 26/26; cargo test: 65 passed (14 network tests ignored); no build warnings.
- [x] no header bar (2026-10-30): the ☰ menu at the bottom right holds every panel (with the readout and SBX/EQ line at the top and the Strip); the window buttons fade in at the top right; the page can be dragged
- [x] compact views (prototype rounds 102–112): the **Remote** for small windows (artist photo printed in red with the cover in colour by the title, or the album art whole as an option; readout; the room with the old app's cabinets; four tiles opening a sheet; controls) and the **Strip** (1240 x 76: cover, title, six cabinets with meters, four choices with popovers, controls with the readout as on the page, a pin that keeps it above other windows via a KWin script, its own minimise/close group); side panels close with Done and carry the window buttons
- [x] ☰ opens the **control centre** (round 113: the user rejected the List as small and size-changing and the Settings page as unpolished): fixed size from the window, pill tabs, Done / Esc / click outside, The Strip in its header; window buttons **appear at the top** (user's pick). Every tab rebuilt to fit without scrolling (user, 2026-10-30), in round 102's compact row style, at 440 x 720 and the default 1240 x 800 too
- [x] **Over your shoulder, mini** as a compact view (Look and feel picks it or the Remote): neon / LED wall / scoreboard, dot speakers round a ring, rows with a bottom sheet
- [x] round 117 (2026-10-30): the settings audit applied (repeats and filler removed, levels moved, rows hidden when they can't do anything); each of the seven kinds of content keeps its own sound (Sound tab kind picker, tray); several apps at once: games, then films/TV/video, then music set the card, every other app gets its own kind's EQ and stereo inside its stream, the card's effects eased while music plays beside a game
- Not done: YouTube trailers saved with yt-dlp and "Other trailers" in full screen (the spec's later trailer sources); team crests on the scoreboard (no source yet). Also not done: the split page's automatic "rows give way to the slim dock on busy covers" (the user picks Split page rows or Split page slim dock instead). Film trailers: no film source gives one yet (Playnite is games only), so the film screen's play button only appears when a trailer is known.
