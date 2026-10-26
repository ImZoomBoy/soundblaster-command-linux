//! Works out what's playing (Game, Movie & TV or Music) from the apps that
//! are playing sound, the details media players report (MPRIS), and whether
//! the sound is 5.1. It never listens to the audio itself.

use crate::system;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Detection {
    pub activity: &'static str,
    /// What was recognised, e.g. "Spotify" or "YouTube".
    pub source: String,
    /// The track or video, if known.
    pub detail: String,
    /// From the media player, when it reports them.
    pub artist: String,
    pub title: String,
    pub url: String,
    /// Genres the player reports (music files usually have one).
    pub genres: Vec<String>,
}

impl Detection {
    /// Changes only when what's playing changes in a way that matters.
    pub fn key(&self) -> String {
        format!("{}|{}", self.activity, self.source)
    }
}

/// One stream that's playing sound.
#[derive(Clone, Debug, Default)]
pub struct Stream {
    pub index: u64,
    pub app: String,
    pub binary: String,
    pub role: String,
    pub title: String,
    pub channels: usize,
    pub pid: Option<u32>,
    /// Sample rate, sample format and channel layout as the app sends them.
    pub rate: u32,
    pub format: String,
    pub map: String,
    pub corked: bool,
}

/// A media player that reports what it's playing.
#[derive(Clone, Debug, Default)]
pub struct Player {
    pub bus: String,
    pub title: String,
    pub artist: String,
    pub url: String,
    pub genres: Vec<String>,
    pub album: String,
    pub art_url: String,
    /// Microseconds.
    pub length: Option<i64>,
    pub position: Option<i64>,
    pub track: Option<i64>,
    /// "Playing", "Paused" or "Stopped".
    pub status: String,
}

const IGNORE: [&str; 12] = [
    "pw-play", "pw-cat", "paplay", "aplay", "speech-dispatcher", "plasmashell", "kded6",
    "kdeconnectd", "kwin_wayland", "ksmserver", "org_kde_powerdevil", "sound blaster",
];
const QUIET_ROLES: [&str; 6] = ["event", "notification", "phone", "accessibility", "alarm", "a11y"];
const MUSIC_APPS: [&str; 24] = [
    "spotify", "rhythmbox", "strawberry", "elisa", "lollypop", "amarok", "clementine", "audacious",
    "deadbeef", "cmus", "feishin", "tidal", "youtube-music", "quodlibet", "cider", "tauon",
    "harmonoid", "g4music", "gapless", "amberol", "nuclear", "sublime-music", "supersonic", "mpd",
];
const VIDEO_APPS: [&str; 17] = [
    "vlc", "mpv", "celluloid", "totem", "showtime", "haruna", "smplayer", "kodi", "jellyfin",
    "plex", "stremio", "freetube", "dragon", "kaffeine", "clapper", "parole", "kmplayer",
];
const GAME_APPS: [&str; 22] = [
    "xrevive", "xemu", "xenia", "retroarch", "dolphin-emu", "pcsx2", "rpcs3", "duckstation",
    "ppsspp", "ryujinx", "cemu", "citra", "lime3ds", "azahar", "melonds", "mgba", "snes9x",
    "mame", "gamescope", "wine", "proton", "steam_app",
];
pub(crate) const BROWSERS: [&str; 15] = [
    "firefox", "zen", "librewolf", "floorp", "waterfox", "chromium", "chrome", "brave", "vivaldi",
    "opera", "edge", "epiphany", "falkon", "konqueror", "thorium",
];
const MUSIC_SITES: [(&str, &str); 8] = [
    ("music.youtube.com", "YouTube Music"), ("open.spotify.com", "Spotify"), ("soundcloud.com", "SoundCloud"),
    ("bandcamp.com", "Bandcamp"), ("music.apple.com", "Apple Music"), ("tidal.com", "Tidal"),
    ("deezer.com", "Deezer"), ("mixcloud.com", "Mixcloud"),
];
const VIDEO_SITES: [(&str, &str); 14] = [
    ("youtube.com", "YouTube"), ("youtu.be", "YouTube"), ("netflix.com", "Netflix"), ("twitch.tv", "Twitch"),
    ("primevideo.com", "Prime Video"), ("amazon.", "Prime Video"), ("disneyplus.com", "Disney+"),
    ("bbc.co.uk/iplayer", "BBC iPlayer"), ("itv.com", "ITVX"), ("channel4.com", "Channel 4"),
    ("crunchyroll.com", "Crunchyroll"), ("vimeo.com", "Vimeo"), ("plex.tv", "Plex"), ("dailymotion.com", "Dailymotion"),
];

pub(crate) fn has(list: &[&str], s: &str) -> bool {
    !s.is_empty() && list.iter().any(|x| s.contains(x))
}

/// Whether a process was started as a game (Steam, Proton, Wine, Lutris…).
/// Checks the process name first, so a sandboxed app's pid can't mislead it.
pub fn pid_is_game(pid: u32, binary: &str) -> bool {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_lowercase();
    let bin = binary.to_lowercase();
    if comm.is_empty() || bin.is_empty() || !(bin.starts_with(&comm) || comm.starts_with(&bin)) {
        return false;
    }
    let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    env.split(|b| *b == 0).any(|kv| {
        let kv = String::from_utf8_lossy(kv);
        (kv.starts_with("SteamAppId=") && kv != "SteamAppId=0" && kv != "SteamAppId=")
            || kv.starts_with("STEAM_COMPAT_APP_ID=")
            || kv.starts_with("WINEPREFIX=")
            || kv.starts_with("LUTRIS_GAME_UUID=")
    })
}

fn pretty(app: &str, binary: &str) -> String {
    let n = if app.is_empty() { binary } else { app };
    let mut c = n.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn detail(p: &Player) -> String {
    match (p.artist.is_empty(), p.title.is_empty()) {
        (false, false) => format!("{} – {}", p.artist, p.title),
        (true, false) => p.title.clone(),
        _ => String::new(),
    }
}

/// What one stream is, if it can be told.
pub fn classify(st: &Stream, players: &[Player], is_game: impl Fn(&Stream) -> bool) -> Option<Detection> {
    let app = st.app.to_lowercase();
    let bin = st.binary.to_lowercase();
    let role = st.role.to_lowercase();
    if has(&IGNORE, &app) || has(&IGNORE, &bin) || QUIET_ROLES.contains(&role.as_str()) {
        return None;
    }
    let mk = |activity, source: String, detail: String| Some(Detection { activity, source, detail, ..Default::default() });
    let mkp = |activity, source: String, p: Option<&Player>| Some(Detection {
        activity,
        source,
        detail: p.map(detail).unwrap_or_default(),
        artist: p.map(|p| p.artist.clone()).unwrap_or_default(),
        title: p.map(|p| p.title.clone()).unwrap_or_default(),
        url: p.map(|p| p.url.clone()).unwrap_or_default(),
        genres: p.map(|p| p.genres.clone()).unwrap_or_default(),
    });
    if has(&GAME_APPS, &app) || has(&GAME_APPS, &bin) || bin.ends_with(".exe") || role == "game" || is_game(st) {
        return mk("game", pretty(&st.app, &st.binary), String::new());
    }
    let player_for = |names: &[&str]| players.iter().find(|p| has(names, &p.bus.to_lowercase())).cloned();
    if has(&MUSIC_APPS, &app) || has(&MUSIC_APPS, &bin) || role == "music" {
        let p = players.iter().find(|p| {
            let b = p.bus.to_lowercase();
            (!app.is_empty() && b.contains(&app)) || (!bin.is_empty() && b.contains(&bin))
        });
        return mkp("music", pretty(&st.app, &st.binary), p);
    }
    if has(&VIDEO_APPS, &app) || has(&VIDEO_APPS, &bin) || role == "video" || role == "movie" {
        return mk("movie", pretty(&st.app, &st.binary), String::new());
    }
    if has(&BROWSERS, &app) || has(&BROWSERS, &bin) {
        let p = player_for(&BROWSERS);
        let url = p.as_ref().map(|p| p.url.to_lowercase()).unwrap_or_default();
        if let Some((_, site)) = MUSIC_SITES.iter().find(|(u, _)| url.contains(u)) {
            return mkp("music", site.to_string(), p.as_ref());
        }
        if let Some((_, site)) = VIDEO_SITES.iter().find(|(u, _)| url.contains(u)) {
            return mkp("movie", site.to_string(), p.as_ref());
        }
        // No player details: the tab title often says enough.
        let tab = st.title.to_lowercase();
        if tab.contains("youtube music") {
            return mk("music", "YouTube Music".into(), String::new());
        }
        for (word, site) in [("youtube", "YouTube"), ("netflix", "Netflix"), ("twitch", "Twitch"),
                             ("prime video", "Prime Video"), ("disney+", "Disney+"), ("iplayer", "BBC iPlayer")] {
            if tab.contains(word) {
                return mk("movie", site.into(), String::new());
            }
        }
        return None;
    }
    if st.channels >= 6 {
        return mk("movie", pretty(&st.app, &st.binary), String::new());
    }
    None
}

/// Picks the newest stream that can be told, out of everything playing.
pub fn pick(streams: &[Stream], players: &[Player], is_game: impl Fn(&Stream) -> bool) -> Option<Detection> {
    let mut v: Vec<&Stream> = streams.iter().collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.index));
    v.into_iter().find_map(|s| classify(s, players, &is_game))
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Streams playing right now (paused ones and our own are left out).
pub fn streams() -> Vec<Stream> {
    all_streams().into_iter().filter(|s| !s.corked).collect()
}

/// Every app stream, paused or not (our own are left out).
pub fn all_streams() -> Vec<Stream> {
    system::pactl_json("sink-inputs")
        .iter()
        .filter_map(|s| {
            let p = &s["properties"];
            if str_of(&p["node.name"]).starts_with("sbcommand") {
                return None;
            }
            Some(Stream {
                index: s["index"].as_u64().unwrap_or(0),
                app: str_of(&p["application.name"]),
                binary: str_of(&p["application.process.binary"]),
                role: str_of(&p["media.role"]),
                title: str_of(&p["media.name"]),
                channels: s["channel_map"].as_str().map_or(2, |m| m.split(',').count()),
                pid: p["application.process.id"].as_str().and_then(|x| x.parse().ok()),
                rate: spec_rate(&str_of(&s["sample_specification"])),
                format: str_of(&s["sample_specification"]).split_whitespace().next().unwrap_or_default().to_string(),
                map: str_of(&s["channel_map"]),
                corked: s["corked"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn spec_rate(spec: &str) -> u32 {
    spec.split_whitespace().find_map(|w| w.strip_suffix("Hz")?.parse().ok()).unwrap_or(0)
}

/// Media players that are playing, with what they report.
pub fn players() -> Vec<Player> {
    all_players().into_iter().filter(|p| p.status == "Playing").collect()
}

/// Every media player (playing, paused or stopped), with what it reports.
pub fn all_players() -> Vec<Player> {
    let Some(list) = system::run("busctl", &["--user", "list", "--no-legend"]) else { return vec![] };
    let mut out = Vec::new();
    for bus in list.lines().filter_map(|l| l.split_whitespace().next()) {
        if !bus.starts_with("org.mpris.MediaPlayer2.") || bus.contains("kdeconnect") {
            continue;
        }
        let get = |prop: &str| {
            system::run("busctl", &["--user", "--json=short", "get-property", bus, "/org/mpris/MediaPlayer2",
                                    "org.mpris.MediaPlayer2.Player", prop])
                .and_then(|o| serde_json::from_str::<Value>(&o).ok())
        };
        let status = get("PlaybackStatus").and_then(|v| v["data"].as_str().map(str::to_string)).unwrap_or_default();
        let md = get("Metadata").map(|v| v["data"].clone()).unwrap_or(Value::Null);
        let artist = md["xesam:artist"]["data"].as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        let genres = md["xesam:genre"]["data"].as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        out.push(Player {
            bus: bus.to_string(),
            title: str_of(&md["xesam:title"]["data"]),
            artist,
            url: str_of(&md["xesam:url"]["data"]),
            genres,
            album: str_of(&md["xesam:album"]["data"]),
            art_url: str_of(&md["mpris:artUrl"]["data"]),
            length: md["mpris:length"]["data"].as_i64().or_else(|| md["mpris:length"]["data"].as_u64().map(|x| x as i64)),
            position: if status == "Stopped" { None } else { get("Position").and_then(|v| v["data"].as_i64()) },
            track: md["xesam:trackNumber"]["data"].as_i64(),
            status,
        });
    }
    out
}

/// What's playing right now, if it can be told.
pub fn detect() -> Option<Detection> {
    if crate::card::dry_run() {
        return None;
    }
    let st = streams();
    if st.is_empty() {
        return None;
    }
    let pl = players();
    pick(&st, &pl, |s| s.pid.is_some_and(|p| pid_is_game(p, &s.binary)))
}

/// Calls `found` when what's playing changes and has stayed the same for two
/// checks in a row (so notification sounds and quick skips are ignored).
/// The first result after starting is reported with `fresh = false`, so
/// opening the app never switches anything by itself.
pub struct Tracker {
    candidate: Option<Option<Detection>>,
    last_key: Option<String>,
    started: bool,
}

impl Tracker {
    pub fn new() -> Self {
        Tracker { candidate: None, last_key: None, started: false }
    }

    /// Returns Some((detection, fresh)) when there's something new to report.
    pub fn update(&mut self, d: Option<Detection>) -> Option<(Option<Detection>, bool)> {
        let stable = self.candidate.as_ref() == Some(&d);
        self.candidate = Some(d.clone());
        if !stable {
            return None;
        }
        let key = d.as_ref().map(Detection::key).unwrap_or_default();
        if self.last_key.as_deref() == Some(key.as_str()) {
            return None;
        }
        self.last_key = Some(key);
        let fresh = self.started;
        self.started = true;
        Some((d, fresh))
    }
}

/// Reports a new song once it has been playing for two checks in a row.
/// The first song after starting is reported with `fresh = false`.
#[derive(Default)]
pub struct SongTracker {
    candidate: Option<String>,
    last: Option<String>,
    started: bool,
}

impl SongTracker {
    pub fn update(&mut self, key: Option<String>) -> Option<(String, bool)> {
        let stable = key.is_some() && self.candidate == key;
        self.candidate = key.clone();
        let key = key?;
        if !stable || self.last.as_deref() == Some(key.as_str()) {
            return None;
        }
        self.last = Some(key.clone());
        let fresh = self.started;
        self.started = true;
        Some((key, fresh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(index: u64, app: &str, bin: &str, role: &str, title: &str, ch: usize) -> Stream {
        Stream { index, app: app.into(), binary: bin.into(), role: role.into(), title: title.into(), channels: ch, ..Default::default() }
    }
    fn pl(bus: &str, title: &str, artist: &str, url: &str) -> Player {
        Player { bus: bus.into(), title: title.into(), artist: artist.into(), url: url.into(), status: "Playing".into(), ..Default::default() }
    }
    fn no(_: &Stream) -> bool {
        false
    }

    #[test]
    fn spotify_is_music_with_track() {
        let p = [pl("org.mpris.MediaPlayer2.spotify", "Penguins & Polarbears", "Millencolin", "https://open.spotify.com/track/x")];
        let d = classify(&st(1, "Spotify", "", "music", "Spotify", 2), &p, no).unwrap();
        assert_eq!((d.activity, d.source.as_str()), ("music", "Spotify"));
        assert_eq!(d.detail, "Millencolin – Penguins & Polarbears");
        assert_eq!((d.artist.as_str(), d.title.as_str()), ("Millencolin", "Penguins & Polarbears"));
    }

    #[test]
    fn browser_uses_the_site() {
        let yt = [pl("org.mpris.MediaPlayer2.firefox.instance_1_53", "I Own 4000 Video Games", "James Channel", "https://www.youtube.com/")];
        let d = classify(&st(1, "Zen", "zen", "", "Home / X", 2), &yt, no).unwrap();
        assert_eq!((d.activity, d.source.as_str()), ("movie", "YouTube"));
        let ytm = [pl("org.mpris.MediaPlayer2.firefox.instance_1", "Song", "Band", "https://music.youtube.com/watch")];
        assert_eq!(classify(&st(1, "Zen", "zen", "", "", 2), &ytm, no).unwrap().activity, "music");
        // Unknown site and no title hint: don't guess.
        assert!(classify(&st(1, "Zen", "zen", "", "Home / X", 2), &[], no).is_none());
        assert_eq!(classify(&st(1, "Zen", "zen", "", "Funny cats - YouTube", 2), &[], no).unwrap().activity, "movie");
    }

    #[test]
    fn games_and_video_players() {
        assert_eq!(classify(&st(1, "wine64-preloader", "wine64-preloader", "", "", 6), &[], no).unwrap().activity, "game");
        assert_eq!(classify(&st(1, "Game.exe", "Game.exe", "", "", 2), &[], no).unwrap().activity, "game");
        assert_eq!(classify(&st(1, "xrevive", "xrevive", "", "", 2), &[], no).unwrap().activity, "game");
        assert_eq!(classify(&st(1, "Some game", "somegame", "", "", 2), &[], |_| true).unwrap().activity, "game");
        assert_eq!(classify(&st(1, "mpv", "mpv", "", "film.mkv", 6), &[], no).unwrap().activity, "movie");
        assert_eq!(classify(&st(1, "Unknown", "unknown", "", "", 6), &[], no).unwrap().activity, "movie");
        assert!(classify(&st(1, "Unknown", "unknown", "", "", 2), &[], no).is_none());
    }

    #[test]
    fn notifications_and_our_sounds_are_ignored() {
        assert!(classify(&st(9, "plasmashell", "plasmashell", "event", "", 2), &[], no).is_none());
        assert!(classify(&st(9, "pw-play", "pw-play", "", "", 1), &[], no).is_none());
        assert!(classify(&st(9, "Sound Blaster Command – test song", "", "", "", 2), &[], no).is_none());
    }

    #[test]
    fn newest_stream_wins() {
        let s = [st(1, "Spotify", "", "music", "", 2), st(5, "mpv", "mpv", "", "", 2), st(9, "plasmashell", "", "event", "", 2)];
        assert_eq!(pick(&s, &[], no).unwrap().activity, "movie");
    }

    #[test]
    fn tracker_waits_and_never_acts_on_start() {
        let music = Some(Detection { activity: "music", source: "Spotify".into(), ..Default::default() });
        let movie = Some(Detection { activity: "movie", source: "YouTube".into(), ..Default::default() });
        let mut t = Tracker::new();
        assert_eq!(t.update(music.clone()), None, "needs two checks");
        assert_eq!(t.update(music.clone()), Some((music.clone(), false)), "first report isn't fresh");
        assert_eq!(t.update(music.clone()), None, "no repeats");
        assert_eq!(t.update(movie.clone()), None);
        assert_eq!(t.update(movie.clone()), Some((movie.clone(), true)));
        assert_eq!(t.update(None), None);
        assert_eq!(t.update(None), Some((None, true)));
    }

    #[test]
    fn songs_are_reported_once_and_never_on_start() {
        let mut t = SongTracker::default();
        assert_eq!(t.update(Some("a".into())), None);
        assert_eq!(t.update(Some("a".into())), Some(("a".into(), false)));
        assert_eq!(t.update(Some("a".into())), None);
        assert_eq!(t.update(Some("b".into())), None);
        assert_eq!(t.update(Some("b".into())), Some(("b".into(), true)));
        assert_eq!(t.update(None), None);
        assert_eq!(t.update(Some("b".into())), None, "the same song after a pause isn't new");
    }
}
