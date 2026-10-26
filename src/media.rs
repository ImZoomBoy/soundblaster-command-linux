//! "Now playing": what the player reports about the track or video (MPRIS),
//! the game's name and artwork (from Steam's own files), and the exact format
//! of the sound the app sends (PipeWire). Nothing here listens to the audio.

use crate::detect::{self, Detection, Player, Stream};
use crate::system;
use std::path::PathBuf;

/// The format of one app's sound stream, as PipeWire sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamFmt {
    pub channels: usize,
    pub rate: u32,
    /// e.g. "float32le", "s16le".
    pub format: String,
    /// e.g. "front-left,front-right" or "aux0,aux1".
    pub map: String,
}

impl StreamFmt {
    /// "Stereo", "5.1", "Mono"…
    pub fn layout(&self) -> String {
        match self.channels {
            1 => "Mono".into(),
            2 => "Stereo".into(),
            3 => "2.1".into(),
            4 => "Quad".into(),
            6 => "5.1".into(),
            8 => "7.1".into(),
            n => format!("{n} ch"),
        }
    }

    /// "44.1 kHz"
    pub fn rate_text(&self) -> String {
        if self.rate == 0 {
            return String::new();
        }
        let k = self.rate as f64 / 1000.0;
        if k.fract() == 0.0 { format!("{k:.0} kHz") } else { format!("{k:.1} kHz") }
    }

    /// "32-bit float", "16-bit"…
    pub fn depth_text(&self) -> String {
        let f = self.format.to_lowercase();
        let bits: String = f.chars().filter(|c| c.is_ascii_digit()).collect();
        if bits.is_empty() {
            return String::new();
        }
        if f.starts_with("float") { format!("{bits}-bit float") } else { format!("{bits}-bit") }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    /// "Song", "Podcast", "Video", "Game" or "Audio".
    pub kind: String,
    pub activity: &'static str,
    /// The app or site, e.g. "Spotify", "YouTube", "Steam".
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// e.g. "Track 2".
    pub extra: String,
    pub art: Option<PathBuf>,
    /// Seconds.
    pub length: Option<f64>,
    pub position: Option<f64>,
    pub playing: bool,
    pub stream: Option<StreamFmt>,
    pub url: String,
}

impl NowPlaying {
    /// Identifies the track, so art and genre are only fetched once.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.app, self.artist, self.title)
    }
}

/// The card's own sink name (for the level meters).
pub fn card_sink() -> Option<String> {
    let tag = system::pci_tag()?;
    system::pactl_json("sinks").iter()
        .filter_map(|s| s["name"].as_str().map(str::to_string))
        .find(|n| n.starts_with(&format!("alsa_output.pci-{tag}")))
}

fn fmt_of(s: &Stream) -> StreamFmt {
    StreamFmt { channels: s.channels, rate: s.rate, format: s.format.clone(), map: s.map.clone() }
}

/// Which player goes with a detection (and its stream).
fn match_player<'a>(players: &'a [Player], st: Option<&Stream>, d: Option<&Detection>) -> Option<&'a Player> {
    let live: Vec<&Player> = players.iter().filter(|p| p.status == "Playing").collect();
    let names: Vec<String> = [st.map(|s| s.app.to_lowercase()), st.map(|s| s.binary.to_lowercase()),
                              d.map(|d| d.source.to_lowercase())]
        .into_iter().flatten().filter(|n| !n.is_empty()).collect();
    let is_browser = names.iter().any(|n| detect::has(&detect::BROWSERS, n))
        || d.is_some_and(|d| !d.url.is_empty());
    live.iter().copied()
        .find(|p| {
            let b = p.bus.to_lowercase();
            names.iter().any(|n| b.contains(n.split_whitespace().next().unwrap_or(n)))
                || (is_browser && detect::has(&detect::BROWSERS, &b))
        })
        .or_else(|| live.first().copied())
}

/// The stream that goes with what was detected.
fn match_stream<'a>(streams: &'a [Stream], d: Option<&Detection>, p: Option<&Player>) -> Option<&'a Stream> {
    let mut v: Vec<&Stream> = streams.iter().filter(|s| !s.corked).collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.index));
    let want: Vec<String> = [d.map(|d| d.source.to_lowercase()), p.map(|p| p.bus.to_lowercase())]
        .into_iter().flatten().collect();
    v.iter().copied()
        .find(|s| {
            let a = s.app.to_lowercase();
            let b = s.binary.to_lowercase();
            want.iter().any(|w| (!a.is_empty() && w.contains(&a)) || (!b.is_empty() && w.contains(&b))
                || (detect::has(&detect::BROWSERS, &a) && detect::has(&detect::BROWSERS, w)))
        })
        .or_else(|| v.first().copied())
}

/// What's playing now, with everything known about it.
pub fn now_playing(d: Option<&Detection>) -> Option<NowPlaying> {
    if crate::card::dry_run() {
        return demo(d);
    }
    let streams = detect::all_streams();
    let players = detect::all_players();
    let Some(d) = d else { return unrecognised(&streams, &players) };
    let st0 = match_stream(&streams, Some(d), None);
    if d.activity == "game" {
        let (name, art) = st0.and_then(|s| s.pid).and_then(steam_game).unwrap_or_else(|| (d.source.clone(), None));
        return Some(NowPlaying {
            kind: "Game".into(), activity: "game", app: if art.is_some() { "Steam".into() } else { d.source.clone() },
            title: name, playing: true, stream: st0.map(fmt_of), art, ..Default::default()
        });
    }
    let p = match_player(&players, st0, Some(d));
    let st = match_stream(&streams, Some(d), p).or(st0);
    let url = p.map(|p| p.url.clone()).unwrap_or_else(|| d.url.clone());
    let kind = match d.activity {
        "music" if url.contains("/episode/") || url.to_lowercase().contains("podcast") => "Podcast",
        "music" => "Song",
        "movie" => "Video",
        _ => "Audio",
    };
    let title = p.map(|p| p.title.clone()).filter(|t| !t.is_empty())
        .unwrap_or_else(|| if d.title.is_empty() { d.detail.clone() } else { d.title.clone() });
    Some(NowPlaying {
        kind: kind.into(),
        activity: d.activity,
        app: d.source.clone(),
        title: if title.is_empty() { d.source.clone() } else { title },
        artist: p.map(|p| p.artist.clone()).unwrap_or_else(|| d.artist.clone()),
        album: p.map(|p| p.album.clone()).unwrap_or_default(),
        extra: p.and_then(|p| p.track).map(|t| format!("Track {t}")).unwrap_or_default(),
        art: p.and_then(|p| art_file(&p.art_url)),
        length: p.and_then(|p| p.length).filter(|l| *l > 0).map(|l| l as f64 / 1e6),
        position: p.and_then(|p| p.position).map(|l| l as f64 / 1e6),
        playing: p.is_none_or(|p| p.status == "Playing"),
        stream: st.map(fmt_of),
        url,
    })
}

/// Something plays that isn't recognised as a game, film or song (e.g. a
/// video on a site the app doesn't know), or everything is paused: still show it.
fn unrecognised(streams: &[Stream], players: &[Player]) -> Option<NowPlaying> {
    let live: Vec<&Stream> = streams.iter().filter(|s| !s.corked && !s.app.is_empty()).collect();
    let newest = live.iter().max_by_key(|s| s.index).copied();
    let from_player = |p: &Player, playing: bool| {
        let st = newest.filter(|s| {
            let b = p.bus.to_lowercase();
            let a = s.app.to_lowercase();
            b.contains(&a) || (detect::has(&detect::BROWSERS, &a) && detect::has(&detect::BROWSERS, &b))
        });
        let app = st.map(|s| s.app.clone()).unwrap_or_else(|| {
            let n = p.bus.trim_start_matches("org.mpris.MediaPlayer2.").split('.').next().unwrap_or("Player");
            let mut c = n.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        });
        NowPlaying {
            kind: if detect::has(&detect::BROWSERS, &p.bus.to_lowercase()) { "Video".into() } else { "Audio".into() },
            activity: "other",
            app,
            title: p.title.clone(),
            artist: p.artist.clone(),
            album: p.album.clone(),
            art: art_file(&p.art_url),
            length: p.length.filter(|l| *l > 0).map(|l| l as f64 / 1e6),
            position: p.position.map(|l| l as f64 / 1e6),
            playing,
            stream: st.map(fmt_of),
            url: p.url.clone(),
            ..Default::default()
        }
    };
    if let Some(p) = players.iter().find(|p| p.status == "Playing" && !p.title.is_empty()) {
        return Some(from_player(p, true));
    }
    if let Some(s) = newest {
        return Some(NowPlaying {
            kind: "Audio".into(), activity: "other", app: s.app.clone(),
            title: if s.title.is_empty() { s.app.clone() } else { s.title.clone() },
            playing: true, stream: Some(fmt_of(s)), ..Default::default()
        });
    }
    players.iter().find(|p| p.status == "Paused" && !p.title.is_empty()).map(|p| from_player(p, false))
}

/// A local file for the player's cover image (downloaded once and kept).
fn art_file(url: &str) -> Option<PathBuf> {
    if let Some(path) = url.strip_prefix("file://") {
        let p = PathBuf::from(percent_decode(path));
        return p.exists().then_some(p);
    }
    if !url.starts_with("https://") {
        return None;
    }
    let dir = system::cache_dir().join("art");
    let file = dir.join(format!("{:016x}.img", fnv(url)));
    if file.exists() {
        return Some(file);
    }
    let _ = std::fs::create_dir_all(&dir);
    let tmp = file.with_extension("part");
    system::run("curl", &["-sSfL", "--max-time", "6", "--max-filesize", "5000000", "-o", &tmp.to_string_lossy(), url])?;
    std::fs::rename(&tmp, &file).ok()?;
    Some(file)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A Steam game's name and header image, from its process.
fn steam_game(pid: u32) -> Option<(String, Option<PathBuf>)> {
    let env = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let id = env.split(|b| *b == 0).find_map(|kv| {
        let kv = String::from_utf8_lossy(kv);
        kv.strip_prefix("SteamAppId=").or_else(|| kv.strip_prefix("STEAM_COMPAT_APP_ID="))
            .filter(|v| !v.is_empty() && *v != "0").map(str::to_string)
    })?;
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    let roots = [home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"), home.join(".local/share/Steam"),
                 home.join(".steam/steam")];
    let mut libs: Vec<PathBuf> = Vec::new();
    for r in &roots {
        libs.push(r.join("steamapps"));
        if let Ok(vdf) = std::fs::read_to_string(r.join("steamapps/libraryfolders.vdf")) {
            for line in vdf.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("\"path\"") {
                    libs.push(PathBuf::from(rest.trim().trim_matches('"')).join("steamapps"));
                }
            }
        }
    }
    let name = libs.iter().find_map(|l| {
        let acf = std::fs::read_to_string(l.join(format!("appmanifest_{id}.acf"))).ok()?;
        acf.lines().find_map(|line| {
            let l = line.trim();
            l.strip_prefix("\"name\"").map(|v| v.trim().trim_matches('"').to_string())
        })
    })?;
    let art = roots.iter().flat_map(|r| {
        let c = r.join("appcache/librarycache");
        [c.join(&id).join("header.jpg"), c.join(format!("{id}_header.jpg")), c.join(&id).join("library_600x900.jpg")]
    }).find(|p| p.exists());
    Some((name, art))
}

/// For the self-test and screenshots: a believable track, no system access.
fn demo(d: Option<&Detection>) -> Option<NowPlaying> {
    let which = std::env::var("SBCOMMAND_DEMO_MEDIA").ok().unwrap_or_default();
    let act = d.map(|d| d.activity).unwrap_or(match which.as_str() {
        "movie" => "movie",
        "game" => "game",
        "" => return None,
        _ => "music",
    });
    let art = std::env::var("SBCOMMAND_DEMO_ART").ok().map(PathBuf::from).filter(|p| p.exists());
    Some(match act {
        "movie" => NowPlaying { kind: "Video".into(), activity: "movie", app: "Jellyfin".into(), title: "Starfall".into(),
            artist: "2025".into(), album: String::new(), extra: String::new(), art, length: Some(9960.0), position: Some(3912.0),
            playing: true, stream: Some(StreamFmt { channels: 6, rate: 48000, format: "float32le".into(), map: String::new() }), url: String::new() },
        "game" => NowPlaying { kind: "Game".into(), activity: "game", app: "Steam".into(), title: "Night Circuit".into(),
            art, playing: true, stream: Some(StreamFmt { channels: 6, rate: 48000, format: "float32le".into(), map: String::new() }),
            ..Default::default() },
        _ => NowPlaying { kind: "Song".into(), activity: "music", app: "Spotify".into(), title: "Neon Rain".into(),
            artist: "The Midnight Static".into(), album: "Afterglow".into(), extra: "Track 1".into(), art, length: Some(272.0), position: Some(102.0),
            playing: true, stream: Some(StreamFmt { channels: 2, rate: 44100, format: "float32le".into(), map: "aux0,aux1".into() }),
            url: String::new() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_read_well() {
        let f = StreamFmt { channels: 2, rate: 44100, format: "float32le".into(), map: String::new() };
        assert_eq!((f.layout(), f.rate_text(), f.depth_text()), ("Stereo".into(), "44.1 kHz".into(), "32-bit float".into()));
        let g = StreamFmt { channels: 6, rate: 48000, format: "s16le".into(), map: String::new() };
        assert_eq!((g.layout(), g.rate_text(), g.depth_text()), ("5.1".into(), "48 kHz".into(), "16-bit".into()));
        assert_eq!(percent_decode("/a%20b.png"), "/a b.png");
    }

    #[test]
    fn the_right_player_and_stream_are_matched() {
        let st = |i, app: &str| Stream { index: i, app: app.into(), channels: 2, rate: 48000, ..Default::default() };
        let pl = |bus: &str, title: &str| Player { bus: bus.into(), title: title.into(), status: "Playing".into(), ..Default::default() };
        let streams = [st(1, "Spotify"), st(7, "Zen")];
        let players = [pl("org.mpris.MediaPlayer2.firefox.instance_1_9", "A video"), pl("org.mpris.MediaPlayer2.spotify", "A song")];
        let d = Detection { activity: "music", source: "Spotify".into(), ..Default::default() };
        let s = match_stream(&streams, Some(&d), None).unwrap();
        assert_eq!(s.app, "Spotify");
        assert_eq!(match_player(&players, Some(s), Some(&d)).unwrap().title, "A song");
        let d = Detection { activity: "movie", source: "YouTube".into(), url: "https://www.youtube.com/watch".into(), ..Default::default() };
        let p = match_player(&players, Some(&streams[1]), Some(&d)).unwrap();
        assert_eq!(p.title, "A video");
        assert_eq!(match_stream(&streams, Some(&d), Some(p)).unwrap().app, "Zen");
    }
}
