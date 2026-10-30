//! What kind of thing is playing, and the pictures and details for the
//! full-window "now playing" page.
//!
//! `classify` decides the kind (studio song, live recording, film, series,
//! game, match, web video, bare stream) from what the player reports, with no
//! network. `lookup` then fetches the extras for that kind and keeps them:
//!
//! * songs: the artist's photos, logo and bio (TheAudioDB), the album's year,
//!   and other studio albums for the record shelf (MusicBrainz and the Cover
//!   Art Archive);
//! * games: Steam's own art from its local library cache (downloaded from
//!   Steam's CDN when missing), screenshots and trailer from the store API,
//!   and fan art from the user's Playnite library on the NAS;
//! * films and series: the player's art, plus posters, backdrops, logo and
//!   overview from TMDB when `SBCOMMAND_TMDB_KEY` holds a (free) API key.
//!
//! Everything fetched is kept under ~/.cache/sbcommand/meta (one JSON file per
//! request, one image per picture, named by a hash of the address), so each
//! artist, game or film is fetched once and renewed after 30 days. A first
//! lookup never takes more than about 14 seconds and returns what it has when
//! something is slow or offline. In a dry run nothing is fetched.

#![allow(dead_code)] // the now-playing page is the only caller

use crate::genre::url_encode;
use crate::media::NowPlaying;
use crate::{card, detect, system};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Fetched things are renewed after this long.
const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);
/// One lookup stops asking the network after this many seconds…
const LOOKUP_SECS: f64 = 14.0;
/// …or after this many downloads.
const MAX_DOWNLOADS: usize = 24;
/// Sites like MusicBrainz want to know who is asking.
const UA: &str = "SoundBlasterCommand/1.1 (personal Linux app for a Sound Blaster ZxR)";
/// Where the user's Playnite library lives (`SBCOMMAND_PLAYNITE` overrides it).
const PLAYNITE_DEFAULT: &str = "/mnt/nas/gaming/backups-2026-10-19/Playnite";
/// TheAudioDB's free test key.
const AUDIODB: &str = "https://www.theaudiodb.com/api/v1/json/123";
const STEAM_CDN: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";
const TMDB: &str = "https://api.themoviedb.org/3";
const TMDB_IMG: &str = "https://image.tmdb.org/t/p";

// ---------------------------------------------------------------------------
// What the page gets
// ---------------------------------------------------------------------------

/// What kind of content is playing; the page's look follows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A song from an album.
    Studio,
    /// A live recording from a small venue (or one that can't be told).
    LiveSmall,
    /// A live recording from a stadium, arena or big open-air show.
    LiveStadium,
    Film,
    /// An episode of a series.
    Tv,
    Game,
    /// A match or race.
    Sport,
    /// A web video (YouTube, Twitch…).
    Video,
    /// A bare stream or link with nothing known about it.
    Stream,
}

impl Kind {
    /// A short name for settings, CSS classes and screenshots.
    pub fn id(&self) -> &'static str {
        match self {
            Kind::Studio => "studio",
            Kind::LiveSmall => "live",
            Kind::LiveStadium => "stadium",
            Kind::Film => "film",
            Kind::Tv => "tv",
            Kind::Game => "game",
            Kind::Sport => "sport",
            Kind::Video => "video",
            Kind::Stream => "stream",
        }
    }

    /// The kind for an id from `id()`; "music" and "movie" (the demo-media
    /// names) stand for the studio and film pages.
    pub fn from_id(id: &str) -> Option<Kind> {
        Some(match id {
            "studio" | "music" => Kind::Studio,
            "live" => Kind::LiveSmall,
            "stadium" => Kind::LiveStadium,
            "film" | "movie" => Kind::Film,
            "tv" => Kind::Tv,
            "game" => Kind::Game,
            "sport" => Kind::Sport,
            "video" => Kind::Video,
            "stream" => Kind::Stream,
            _ => return None,
        })
    }

    pub fn is_music(&self) -> bool {
        matches!(self, Kind::Studio | Kind::LiveSmall | Kind::LiveStadium)
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Kind::LiveSmall | Kind::LiveStadium)
    }
}

/// Everything the page shows about what's playing. Empty fields are simply
/// left out of the page.
#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub kind: Option<Kind>,
    pub title: String,
    /// Artist, director, developer or channel.
    pub subtitle: String,
    /// "Spotify", "Steam", "Jellyfin"…
    pub kicker_app: String,
    /// e.g. ("Track", "11"), ("Album", "The Colour And The Shape (1997)"),
    /// ("Genre", "Rock"), ("Length", "4:10"); live adds ("Recorded live", "Venue, date").
    pub credits: Vec<(String, String)>,
    /// One or two sentences, at most about 220 characters, cut at a sentence end.
    pub bio: String,
    /// Short chips, e.g. "2021", "BioWare", "Steam".
    pub facts: Vec<String>,
    /// Live recordings: where and when.
    pub venue: String,
    pub date: String,
    /// The artist's or game's logo (PNG with transparency).
    pub logo: Option<PathBuf>,
    /// Album art, poster or game box art.
    pub cover: Option<PathBuf>,
    /// Pictures for behind the page. Games: official art first, fan art last.
    /// Music: artist photos. Films: the backdrop.
    pub backgrounds: Vec<PathBuf>,
    /// Games: official art, then screenshots, then fan art.
    pub pictures: Vec<PathBuf>,
    /// A playable address or file path (Steam trailers are HLS playlists).
    pub trailer: Option<String>,
    pub trailer_name: String,
    /// Music: other studio albums by the artist, for the record shelf.
    pub records: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// Telling the kind (pure)
// ---------------------------------------------------------------------------

/// Whole-word positions of `word` in `hay` (both lower-case ASCII, so byte
/// positions match the original text).
fn word_positions(hay: &str, word: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(word) {
        let at = from + i;
        let end = at + word.len();
        let before = hay[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = hay[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before && after {
            out.push(at);
        }
        from = end;
    }
    out
}

fn has_word(hay: &str, word: &str) -> bool {
    !word_positions(hay, word).is_empty()
}

/// Whether a title or album says it's a live recording. Albums count any
/// "live" word. Titles are stricter, so songs like "Live Forever" and "Live
/// and Let Die" stay studio: their "live" must be in brackets, after a dash,
/// at the end, or followed by at / in / from.
fn is_live_text(text: &str, is_title: bool) -> bool {
    let l = text.to_ascii_lowercase();
    if has_word(&l, "unplugged") || has_word(&l, "in concert") {
        return true;
    }
    word_positions(&l, "live").into_iter().any(|at| {
        if !is_title {
            return true;
        }
        let before = l[..at].trim_end();
        let after = l[at + 4..].trim_start();
        before.ends_with(['(', '[', '-', '–', '—', ':', '/', '|'])
            || ["at ", "in ", "from "].iter().any(|p| after.starts_with(p))
            || after.starts_with('@')
            || after.is_empty()
            || after.starts_with([')', ']'])
    })
}

const MONTHS: [(&str, &str); 12] = [
    ("jan", "january"), ("feb", "february"), ("mar", "march"), ("apr", "april"), ("may", "may"), ("jun", "june"),
    ("jul", "july"), ("aug", "august"), ("sep", "september"), ("oct", "october"), ("nov", "november"), ("dec", "december"),
];

/// "Jul", "July." (a whole month name or its three-letter short form, but not "Marquee").
fn is_month(w: &str) -> bool {
    let w = w.trim_matches(|c: char| c == ',' || c == '.').to_ascii_lowercase();
    w == "sept" || MONTHS.iter().any(|(short, full)| w == *short || w == *full)
}

fn is_year(w: &str) -> bool {
    w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()) && matches!(w.parse::<u32>(), Ok(1900..=2099))
}

/// The first four-digit year (1900-2099) in some text.
fn year_in(s: &str) -> Option<String> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(3)).find_map(|i| {
        let w = s.get(i..i + 4)?;
        let before = i == 0 || !b[i - 1].is_ascii_digit();
        let after = b.get(i + 4).is_none_or(|c| !c.is_ascii_digit());
        (before && after && is_year(w)).then(|| w.to_string())
    })
}

/// "Wembley Stadium, 12 July 1986" -> ("Wembley Stadium", "12 July 1986").
fn split_date(s: &str) -> (String, String) {
    let mut t: Vec<&str> = s.split_whitespace().collect();
    let mut date: Vec<&str> = Vec::new();
    let word = |w: &str| w.trim_matches(|c: char| c == ',' || c == '.').to_ascii_lowercase();
    if t.last().is_some_and(|w| is_year(w.trim_matches(','))) {
        date.insert(0, t.pop().unwrap_or_default().trim_matches(','));
        if t.last().is_some_and(|w| is_month(w)) {
            date.insert(0, t.pop().unwrap_or_default().trim_matches(','));
            let day = t.last().map(|w| word(w));
            if day.is_some_and(|d| {
                let digits = ["st", "nd", "rd", "th"].iter().find_map(|s| d.strip_suffix(*s)).unwrap_or(&d);
                !digits.is_empty() && digits.len() <= 2 && digits.chars().all(|c| c.is_ascii_digit())
            }) {
                date.insert(0, t.pop().unwrap_or_default().trim_matches(','));
            }
        }
    }
    let venue = t.join(" ").trim_end_matches([',', '-', ' ']).trim().to_string();
    (venue, date.join(" "))
}

/// Cuts a venue phrase off where the title carries on with something else
/// ("(Remastered)", " - 2011 Mix"), and splits a trailing date off it.
fn clean_venue(tail: &str) -> (String, String) {
    let mut end = tail.len();
    for cut in ["(", ")", "[", "]", " - ", " – ", " — ", " | ", " / "] {
        if let Some(i) = tail.find(cut) {
            end = end.min(i);
        }
    }
    let l = tail[..end].to_ascii_lowercase();
    for w in ["remastered", "remaster", "deluxe", "edition", "expanded", "anniversary", "bonus"] {
        if let Some(&i) = word_positions(&l, w).first() {
            end = end.min(i);
        }
    }
    split_date(tail[..end].trim())
}

/// (venue, date) from the first "Live at / in / from X" in some text.
fn live_venue(text: &str) -> Option<(String, String)> {
    let l = text.to_ascii_lowercase();
    // "Live at …", and also "Unplugged in …" / "In Concert at …".
    let words: Vec<(usize, usize)> = word_positions(&l, "live").into_iter().map(|p| (p, 4))
        .chain(word_positions(&l, "unplugged").into_iter().map(|p| (p, 9)))
        .chain(word_positions(&l, "concert").into_iter().map(|p| (p, 7))).collect();
    for (at, len) in words {
        let rest = text[at + len..].trim_start();
        let rest_l = rest.to_ascii_lowercase();
        for prep in ["at", "in", "from"] {
            if rest_l.starts_with(prep) && rest_l[prep.len()..].starts_with(char::is_whitespace) {
                let (venue, date) = clean_venue(rest[prep.len()..].trim_start());
                if !venue.is_empty() || !date.is_empty() {
                    return Some((venue, date));
                }
            }
        }
    }
    None
}

/// Whether a venue is a stadium, arena or big open-air place. "Park" and
/// "field" alone are too vague to count when the name also says theatre, club,
/// hall and so on.
fn is_stadium(venue: &str) -> bool {
    let l = venue.to_ascii_lowercase();
    let toks: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    let big = |t: &&str| {
        t.starts_with("stadi") || t.starts_with("estadi") || *t == "stade" || t.starts_with("arena")
            || *t == "wembley" || *t == "murrayfield"
            || *t == "dome" || (t.len() >= 5 && t.ends_with("dome"))
            || *t == "bowl" || (t.len() >= 5 && t.ends_with("bowl"))
    };
    let vague = toks.contains(&"park") || toks.contains(&"field");
    let small = toks.iter().any(|t| {
        ["theatre", "theater", "club", "hall", "pub", "bar", "cafe", "room", "church", "studio", "studios",
         "lounge", "ballroom", "house", "school", "college"].contains(t)
    });
    toks.iter().any(big) || (vague && !small)
}

/// What a live recording says about itself.
#[derive(Clone, Debug, PartialEq)]
struct Live {
    venue: String,
    date: String,
}

/// Some for a live recording (album first for the venue, then the title).
fn live_info(np: &NowPlaying) -> Option<Live> {
    if !(is_live_text(&np.album, false) || is_live_text(&np.title, true)) {
        return None;
    }
    let (venue, date) = live_venue(&np.album).or_else(|| live_venue(&np.title)).unwrap_or_default();
    Some(Live { venue, date })
}

/// Season and episode numbers found in a title.
#[derive(Clone, Debug, Default, PartialEq)]
struct TvNum {
    season: Option<u32>,
    episode: Option<u32>,
}

/// Up to `max` digits at `i`: (value, index after them).
fn digits_at(b: &[u8], i: usize, max: usize) -> Option<(u32, usize)> {
    let n = b.get(i..)?.iter().take(max).take_while(|c| c.is_ascii_digit()).count();
    if n == 0 {
        return None;
    }
    let v = std::str::from_utf8(&b[i..i + n]).ok()?.parse().ok()?;
    Some((v, i + n))
}

/// "S02E05", "S2 E5", "s02.e05", "S1:E3" in lower-case text: (where it
/// starts, season, episode).
fn sxxeyy(l: &str) -> Option<(usize, u32, u32)> {
    let b = l.as_bytes();
    for i in 0..b.len() {
        if b[i] != b's' || (i > 0 && b[i - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let Some((s, mut k)) = digits_at(b, i + 1, 2) else { continue };
        if b.get(k).is_some_and(|c| matches!(c, b' ' | b'.' | b'_' | b'-' | b':')) {
            k += 1;
        }
        if b.get(k) == Some(&b'e')
            && let Some((e, end)) = digits_at(b, k + 1, 3)
            && b.get(end).is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            return Some((i, s, e));
        }
    }
    None
}

/// "S02E05", "S2 E5", "s02.e05", "Season 2", "Episode 5".
fn tv_numbers(text: &str) -> Option<TvNum> {
    let l = text.to_ascii_lowercase();
    if let Some((_, season, episode)) = sxxeyy(&l) {
        return Some(TvNum { season: Some(season), episode: Some(episode) });
    }
    let mut found = TvNum::default();
    for (word, slot) in [("season", 0), ("episode", 1)] {
        for at in word_positions(&l, word) {
            let rest = l[at + word.len()..].trim_start();
            if let Some((n, end)) = digits_at(rest.as_bytes(), 0, 3)
                && rest.as_bytes().get(end).is_none_or(|c| !c.is_ascii_alphanumeric())
            {
                if slot == 0 { found.season = Some(n) } else { found.episode = Some(n) }
            }
        }
    }
    (found != TvNum::default()).then_some(found)
}

/// Words that make a title a match or race.
const LEAGUES: [&str; 28] = [
    "premier league", "champions league", "europa league", "conference league", "la liga", "serie a",
    "bundesliga", "ligue 1", "fa cup", "world cup", "six nations", "super bowl", "nba", "nfl", "nhl",
    "mlb", "mls", "f1", "formula 1", "formula one", "formula e", "grand prix", "motogp", "ufc",
    "nascar", "wimbledon", "ryder cup", "tour de france",
];

/// Library apps hold films, and films have titles like "Batman v Superman",
/// so for them "v" alone doesn't make a title a match.
const LIBRARY_APPS: [&str; 4] = ["jellyfin", "plex", "emby", "kodi"];

/// "X v Y", "X vs Y", "X vs. Y".
fn has_versus(l: &str) -> bool {
    let t: Vec<&str> = l.split_whitespace().collect();
    t.iter().enumerate().any(|(i, w)| i > 0 && i + 1 < t.len() && matches!(*w, "v" | "vs" | "vs."))
}

fn sport_title(title: &str, versus_counts: bool) -> bool {
    let l = title.to_ascii_lowercase();
    LEAGUES.iter().any(|w| has_word(&l, w)) || (versus_counts && has_versus(&l))
}

/// Sites that carry web video.
const VIDEO_SITES: [&str; 5] = ["youtube", "youtu.be", "twitch", "vimeo", "dailymotion"];

fn is_video_site(np: &NowPlaying) -> bool {
    let (a, u) = (np.app.to_lowercase(), np.url.to_lowercase());
    VIDEO_SITES.iter().any(|s| a.contains(s) || u.contains(s))
}

fn is_ace_stream(np: &NowPlaying) -> bool {
    let a = np.app.to_lowercase();
    a.contains("acestream") || a.contains("ace stream") || a.contains("ace_stream")
}

fn is_bare_link(title: &str) -> bool {
    let t = title.trim().to_ascii_lowercase();
    !t.contains(' ') && (t.contains("://") || t.starts_with("www.") || t.starts_with("magnet:"))
}

/// The player gave a title (the app's name stands in for a missing one).
fn has_real_title(np: &NowPlaying) -> bool {
    let t = np.title.trim();
    !t.is_empty() && !t.eq_ignore_ascii_case(np.app.trim())
}

/// A browser is playing (it says so, or a web address does).
fn is_browser(np: &NowPlaying) -> bool {
    detect::has(&detect::BROWSERS, &np.app.to_lowercase()) || np.kind == "Video" || np.url.starts_with("http")
}

fn is_tv(np: &NowPlaying) -> bool {
    [&np.title, &np.extra, &np.album].iter().any(|t| tv_numbers(t).is_some())
}

/// The kind's id for settings ("music", "film", "tv", "game", "sport",
/// "video", "stream"; see model::KINDS).
pub fn kind_id(k: &Kind) -> &'static str {
    match k {
        Kind::Studio | Kind::LiveSmall | Kind::LiveStadium => "music",
        Kind::Film => "film",
        Kind::Tv => "tv",
        Kind::Game => "game",
        Kind::Sport => "sport",
        Kind::Video => "video",
        Kind::Stream => "stream",
    }
}

/// The kind of content a detected source is, from what detection knows (no
/// lookups, so the background service can use it too).
pub fn kind_of_detection(d: &crate::detect::Detection) -> &'static str {
    let np = NowPlaying {
        activity: d.activity,
        app: d.source.clone(),
        title: if d.title.is_empty() { d.detail.clone() } else { d.title.clone() },
        artist: d.artist.clone(),
        url: d.url.clone(),
        ..Default::default()
    };
    kind_id(&classify(&np))
}

/// What kind of content is playing.
pub fn classify(np: &NowPlaying) -> Kind {
    match np.activity {
        "game" => Kind::Game,
        "music" => match live_info(np) {
            Some(l) if is_stadium(&l.venue) => Kind::LiveStadium,
            Some(_) => Kind::LiveSmall,
            None => Kind::Studio,
        },
        "movie" => {
            let app = np.app.to_lowercase();
            if !has_real_title(np) {
                Kind::Stream
            } else if is_tv(np) {
                Kind::Tv
            } else if sport_title(&np.title, !LIBRARY_APPS.iter().any(|a| app.contains(a))) {
                Kind::Sport
            } else if is_video_site(np) {
                Kind::Video
            } else if is_ace_stream(np) || is_bare_link(&np.title) {
                Kind::Stream
            } else {
                Kind::Film
            }
        }
        _ => {
            if !has_real_title(np) {
                Kind::Stream
            } else if is_browser(np) {
                Kind::Video
            } else {
                Kind::Stream
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small text helpers (pure)
// ---------------------------------------------------------------------------

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Letters and digits only, lower case, for telling whether two names match
/// ("Mass Effect™ Legendary Edition" and "Mass Effect Legendary Edition").
fn norm(s: &str) -> String {
    s.replace('&', " and ").chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Like `norm`, without a leading "the" ("The Beatles" is "Beatles").
fn norm_artist(s: &str) -> String {
    let n = norm(s);
    n.strip_prefix("the").filter(|r| !r.is_empty()).map(str::to_string).unwrap_or(n)
}

/// A string field of a JSON object ("" when missing or null).
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().trim().to_string()
}

/// Turns "http://" into "https://" and drops anything else.
fn https_url(s: &str) -> Option<String> {
    let u = s.trim();
    if let Some(r) = u.strip_prefix("http://") {
        Some(format!("https://{r}"))
    } else {
        u.starts_with("https://").then(|| u.to_string())
    }
}

/// Plain text out of a little HTML: tags dropped, entities decoded, blanks collapsed.
fn clean_text(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '<' if c.get(i + 1).is_some_and(|n| n.is_alphabetic() || matches!(n, '/' | '!')) => {
                while i < c.len() && c[i] != '>' {
                    i += 1;
                }
                out.push(' ');
            }
            '&' => {
                let end = (i + 1..c.len().min(i + 10)).find(|&j| c[j] == ';');
                let ent: String = end.map(|e| c[i + 1..e].iter().collect()).unwrap_or_default();
                let decoded = match ent.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    e => e.strip_prefix('#').and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok(),
                        None => n.parse().ok(),
                    }).and_then(char::from_u32),
                };
                match (decoded, end) {
                    (Some(d), Some(e)) => {
                        out.push(d);
                        i = e;
                    }
                    _ => out.push('&'),
                }
            }
            ch => out.push(ch),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words that end in a full stop without ending the sentence.
fn is_abbreviation(word: &str) -> bool {
    let w = word.trim_start_matches(['(', '"', '\'']).to_lowercase();
    w.contains('.') || w.chars().count() == 1
        || ["mr", "mrs", "ms", "dr", "st", "jr", "sr", "vs", "no", "inc", "ltd", "co", "prof", "mt"].contains(&w.as_str())
}

fn split_sentences(t: &str) -> Vec<String> {
    let c: Vec<char> = t.chars().collect();
    let (mut out, mut start) = (Vec::new(), 0);
    for i in 0..c.len() {
        if !matches!(c[i], '.' | '!' | '?') {
            continue;
        }
        let mut j = i + 1;
        while j < c.len() && matches!(c[j], '"' | '\'' | ')' | '”' | '’') {
            j += 1;
        }
        if j < c.len() && !c[j].is_whitespace() {
            continue;
        }
        let next = c[j.min(c.len())..].iter().find(|ch| !ch.is_whitespace());
        let starts_new = next.is_none_or(|ch| ch.is_uppercase() || ch.is_ascii_digit() || matches!(ch, '"' | '“' | '\''));
        let word: String = c[start..i].iter().rev().take_while(|ch| !ch.is_whitespace()).collect::<Vec<_>>()
            .into_iter().rev().collect();
        if starts_new && !(c[i] == '.' && is_abbreviation(&word)) {
            out.push(c[start..j].iter().collect::<String>().trim().to_string());
            start = j;
        }
    }
    if start < c.len() {
        let rest = c[start..].iter().collect::<String>().trim().to_string();
        if !rest.is_empty() {
            out.push(rest);
        }
    }
    out
}

/// The first one or two sentences of a text, at most `max` characters, cut at
/// a sentence end (or at a word, with "…", when even the first is too long).
fn short_bio(raw: &str, max: usize) -> String {
    let sentences = split_sentences(&clean_text(raw));
    let mut out = String::new();
    for s in sentences.iter().take(2) {
        let next = if out.is_empty() { s.clone() } else { format!("{out} {s}") };
        if next.chars().count() > max {
            break;
        }
        out = next;
    }
    if out.is_empty()
        && let Some(first) = sentences.first()
    {
        let cut: String = first.chars().take(max.saturating_sub(1)).collect();
        let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a);
        out = format!("{}…", cut.trim_end_matches([',', ';', ':', ' ', '-', '–', '—', '&']));
    }
    out
}

/// "4:10", or "1 h 42 min" for long ones.
fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    // A film reads "2 h 46 min"; a song "4:10".
    if s >= 3600 { format!("{} h {} min", s / 3600, s % 3600 / 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// "Track 11" -> "11".
fn track_number(extra: &str) -> Option<String> {
    let t = extra.trim();
    let n = t.get(..6).filter(|p| p.eq_ignore_ascii_case("track ")).map_or(t, |_| t[6..].trim());
    (!n.is_empty()).then(|| n.to_string())
}

/// An album name without "(Deluxe Edition)" or " - Remastered" on the end.
fn clean_album(a: &str) -> String {
    let end = [" (", " [", " - ", " – "].iter().filter_map(|c| a.find(c)).min().unwrap_or(a.len());
    a[..end].trim().to_string()
}

/// "A, B" (several artists): the first one.
fn first_artist(a: &str) -> &str {
    a.split(", ").next().unwrap_or(a).trim()
}

// ---------------------------------------------------------------------------
// Fetching and keeping (curl, with limits)
// ---------------------------------------------------------------------------

fn meta_dir() -> PathBuf {
    system::cache_dir().join("meta")
}

/// Where a downloaded picture lives. The address without its "?t=…" tail is
/// the key, so renewed pictures replace the old file.
fn img_path(url: &str) -> PathBuf {
    let key = url.split('?').next().unwrap_or(url);
    let lower = key.to_ascii_lowercase();
    let ext = [".jpeg", ".jpg", ".png", ".webp", ".gif"].iter()
        .filter_map(|e| lower.rfind(e).map(|i| (i, *e))).max().map_or("jpg", |(_, e)| &e[1..]);
    meta_dir().join(format!("img-{:016x}.{ext}", fnv(key)))
}

/// A kept file that is younger than `max`.
fn younger_than(p: &Path, max: Duration) -> bool {
    match std::fs::metadata(p).and_then(|m| m.modified()) {
        // A date in the future counts as new.
        Ok(t) => SystemTime::now().duration_since(t).map_or(true, |age| age < max),
        Err(_) => false,
    }
}

/// A kept file that is under 30 days old.
fn fresh(p: &Path) -> bool {
    younger_than(p, MAX_AGE)
}

/// Writes through a temporary file so a half-written file is never read.
fn write_atomic(path: &Path, bytes: &[u8]) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = path.with_extension("part");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// How long this lookup may still use the network, and how many downloads it has left.
struct Budget {
    deadline: Instant,
    downloads: usize,
}

impl Budget {
    fn new() -> Self {
        Budget { deadline: Instant::now() + Duration::from_secs_f64(LOOKUP_SECS), downloads: MAX_DOWNLOADS }
    }
}

thread_local! {
    static BUDGET: RefCell<Budget> = RefCell::new(Budget::new());
}

fn start_budget() {
    BUDGET.with(|b| *b.borrow_mut() = Budget::new());
}

/// Seconds the next request may take: at most `cap`, and None when the lookup's time is nearly up.
fn secs_for(cap: f64) -> Option<f64> {
    let left = BUDGET.with(|b| b.borrow().deadline).saturating_duration_since(Instant::now()).as_secs_f64();
    (left >= 1.0).then_some(cap.min(left))
}

/// How many of `want` downloads are still allowed (and counts them).
fn take_downloads(want: usize) -> usize {
    BUDGET.with(|b| {
        let mut b = b.borrow_mut();
        let n = want.min(b.downloads);
        b.downloads -= n;
        n
    })
}

/// Hosts that just failed (offline, busy, rate-limited), so a run of track
/// changes doesn't keep waiting on them.
static HOSTS_DOWN: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());

fn recently(list: &Mutex<Vec<(String, Instant)>>, key: &str, within: Duration) -> bool {
    list.lock().is_ok_and(|l| l.iter().any(|(k, t)| k == key && t.elapsed() < within))
}

fn remember(list: &Mutex<Vec<(String, Instant)>>, key: &str) {
    if let Ok(mut l) = list.lock() {
        l.retain(|(_, t)| t.elapsed() < Duration::from_secs(600));
        l.push((key.to_string(), Instant::now()));
    }
}

fn host_of(url: &str) -> &str {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or_default()
}

/// A JSON answer, from the cache when it's under 30 days old, else fetched
/// once (and kept). A failed fetch falls back to an older cached copy.
fn fetch_json(url: &str) -> Option<Value> {
    let path = meta_dir().join(format!("http-{:016x}.json", fnv(url)));
    let cached: Option<Value> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok());
    if cached.is_some() && fresh(&path) {
        return cached;
    }
    let host = host_of(url);
    if recently(&HOSTS_DOWN, host, Duration::from_secs(60)) {
        return cached;
    }
    let Some(secs) = secs_for(7.0) else { return cached };
    let secs = format!("{secs:.1}");
    let Some(body) = system::run_net("curl", &["-sSfL", "--connect-timeout", "4", "--max-time", &secs,
                                           "--max-filesize", "5000000", "-A", UA, url]) else {
        remember(&HOSTS_DOWN, host);
        return cached;
    };
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => {
            write_atomic(&path, body.as_bytes());
            Some(v)
        }
        Err(_) => cached, // also what a dry run gets (it fetches nothing)
    }
}

/// The first bytes of a picture file say it is one (not an error page).
fn looks_like_image(head: &[u8]) -> bool {
    head.starts_with(&[0xFF, 0xD8, 0xFF]) || head.starts_with(b"\x89PNG") || head.starts_with(b"GIF8")
        || (head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP")
}

/// Downloads one picture (under 5 MB) to `dest`. Whether it worked.
fn download_one(url: &str, dest: &Path, secs: f64) -> bool {
    if let Some(d) = dest.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = dest.with_extension("part");
    let secs = format!("{secs:.1}");
    let ran = system::run_net("curl", &["-sSfL", "--connect-timeout", "4", "--max-time", &secs, "--max-filesize", "5000000",
                                    "-A", UA, "-o", &tmp.to_string_lossy(), url]).is_some();
    let mut head = [0u8; 16];
    let ok = ran && std::fs::File::open(&tmp).and_then(|mut f| f.read(&mut head)).is_ok_and(|n| looks_like_image(&head[..n]));
    if ok && std::fs::rename(&tmp, dest).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(&tmp);
    false
}

/// Pictures that failed to download leave an empty `.none` file next to where
/// they would be, so the next lookups (even after a restart) skip them for an
/// hour instead of waiting on them again. Some are just slow: the Cover Art
/// Archive takes 6 to 9 seconds to make a thumbnail the first time.
fn failed_marker(dest: &Path) -> PathBuf {
    dest.with_extension("none")
}

const FAIL_MEMORY: Duration = Duration::from_secs(3600);
/// Pictures fetched at the same time, and the longest one may take.
const PARALLEL: usize = 12;
const PICTURE_SECS: f64 = 5.0;

/// Pictures to fetch, as (address, where to keep it). Up to twelve at a time;
/// cached ones (under 30 days old) are not fetched again. The answers are in
/// the same order; an older cached copy stands in for a failed download.
fn fetch_files(jobs: &[(String, PathBuf)]) -> Vec<Option<PathBuf>> {
    let mut out: Vec<Option<PathBuf>> = jobs.iter().map(|(_, d)| (d.exists() && fresh(d)).then(|| d.clone())).collect();
    let mut todo: Vec<usize> = (0..jobs.len())
        .filter(|&i| out[i].is_none() && !younger_than(&failed_marker(&jobs[i].1), FAIL_MEMORY))
        .collect();
    todo.truncate(take_downloads(todo.len()));
    for chunk in todo.chunks(PARALLEL) {
        let Some(secs) = secs_for(PICTURE_SECS) else { break };
        let results: Vec<(usize, bool)> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk.iter().map(|&i| {
                let (url, dest) = (&jobs[i].0, &jobs[i].1);
                s.spawn(move || (i, download_one(url, dest, secs)))
            }).collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        for (i, ok) in results {
            let marker = failed_marker(&jobs[i].1);
            if ok {
                out[i] = Some(jobs[i].1.clone());
                let _ = std::fs::remove_file(marker);
            } else if !no_net() {
                let _ = std::fs::write(marker, b"");
            }
        }
    }
    for (i, (_, d)) in jobs.iter().enumerate() {
        if out[i].is_none() && d.exists() {
            out[i] = Some(d.clone());
        }
    }
    out
}

/// One picture: cached, or downloaded to `dest`.
fn fetch_file(url: &str, dest: &Path) -> Option<PathBuf> {
    fetch_files(&[(url.to_string(), dest.to_path_buf())]).pop().flatten()
}

/// Runs `f` on its own thread and waits `secs` for it. The thread is left to
/// finish by itself when it's slow, so a stuck NAS can't hold the lookup.
fn with_timeout<T: Send + 'static>(secs: f64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs_f64(secs.max(0.0))).ok()
}

/// A picture's pixel size from its header (JPEG, PNG or GIF), without loading it.
fn image_size_of<R: Read + Seek>(r: &mut R) -> Option<(u32, u32)> {
    let mut h = [0u8; 24];
    r.read_exact(&mut h).ok()?;
    if h.starts_with(b"\x89PNG") {
        return Some((u32::from_be_bytes(h[16..20].try_into().ok()?), u32::from_be_bytes(h[20..24].try_into().ok()?)));
    }
    if h.starts_with(b"GIF8") {
        return Some((u16::from_le_bytes([h[6], h[7]]) as u32, u16::from_le_bytes([h[8], h[9]]) as u32));
    }
    if !h.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    r.seek(SeekFrom::Start(2)).ok()?;
    loop {
        let mut b = [0u8; 1];
        r.read_exact(&mut b).ok()?;
        if b[0] != 0xFF {
            continue;
        }
        r.read_exact(&mut b).ok()?;
        while b[0] == 0xFF {
            r.read_exact(&mut b).ok()?;
        }
        match b[0] {
            0x01 | 0xD0..=0xD8 => {}
            0xD9 | 0xDA => return None,
            // The start-of-frame markers hold the size (not C4, C8 or CC).
            m @ 0xC0..=0xCF if !matches!(m, 0xC4 | 0xC8 | 0xCC) => {
                let mut f = [0u8; 7];
                r.read_exact(&mut f).ok()?;
                return Some((u16::from_be_bytes([f[5], f[6]]) as u32, u16::from_be_bytes([f[3], f[4]]) as u32));
            }
            _ => {
                let mut l = [0u8; 2];
                r.read_exact(&mut l).ok()?;
                r.seek(SeekFrom::Current(i64::from(u16::from_be_bytes(l)) - 2)).ok()?;
            }
        }
    }
}

fn image_size(p: &Path) -> Option<(u32, u32)> {
    image_size_of(&mut BufReader::new(std::fs::File::open(p).ok()?))
}

// ---------------------------------------------------------------------------
// Steam
// ---------------------------------------------------------------------------

/// Steam's folders (Flatpak first, as on the user's machine).
fn steam_roots() -> Vec<PathBuf> {
    let Ok(home) = std::env::var("HOME") else { return vec![] };
    let home = PathBuf::from(home);
    vec![home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"), home.join(".local/share/Steam"),
         home.join(".steam/steam")]
}

/// `"key"  "value"` from a line of Steam's text files.
fn vdf_pair(line: &str) -> Option<(&str, &str)> {
    let mut q = line.split('"');
    q.next()?;
    let key = q.next()?;
    q.next()?;
    Some((key, q.next()?))
}

/// Every library's `steamapps` folder.
fn steam_libs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !libs.contains(&p) {
            libs.push(p);
        }
    };
    for r in roots {
        add(r.join("steamapps"));
        if let Ok(vdf) = std::fs::read_to_string(r.join("steamapps/libraryfolders.vdf")) {
            for (k, v) in vdf.lines().filter_map(|l| vdf_pair(l.trim())) {
                if k == "path" {
                    add(PathBuf::from(v).join("steamapps"));
                }
            }
        }
    }
    libs
}

/// (app id, name) from an `appmanifest_*.acf` file.
fn parse_acf(text: &str) -> Option<(String, String)> {
    let (mut id, mut name) = (None, None);
    for (k, v) in text.lines().filter_map(|l| vdf_pair(l.trim())) {
        match k {
            "appid" if id.is_none() => id = Some(v.to_string()),
            "name" if name.is_none() => name = Some(v.to_string()),
            _ => {}
        }
    }
    Some((id?, name?))
}

/// The Steam app id in a process's environment.
fn appid_from_environ(env: &[u8]) -> Option<String> {
    env.split(|b| *b == 0).find_map(|kv| {
        let kv = String::from_utf8_lossy(kv);
        kv.strip_prefix("SteamAppId=").or_else(|| kv.strip_prefix("STEAM_COMPAT_APP_ID="))
            .filter(|v| !v.is_empty() && *v != "0").map(str::to_string)
    })
}

/// A running game's Steam app id, from its process.
pub fn steam_appid_for_pid(pid: u32) -> Option<String> {
    appid_from_environ(&std::fs::read(format!("/proc/{pid}/environ")).ok()?)
}

/// The Steam app id of an installed game, found by its name in the
/// `appmanifest_*.acf` files of every library.
fn steam_appid_for_name(name: &str) -> Option<String> {
    let want = norm(name);
    if want.is_empty() {
        return None;
    }
    for lib in steam_libs(&steam_roots()) {
        let Ok(dir) = std::fs::read_dir(&lib) else { continue };
        for e in dir.flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            if f.starts_with("appmanifest_") && f.ends_with(".acf")
                && let Some((id, n)) = std::fs::read_to_string(e.path()).ok().and_then(|t| parse_acf(&t))
                && norm(&n) == want
            {
                return Some(id);
            }
        }
    }
    None
}

/// One of Steam's pictures for a game, from its library cache. Newer Steam
/// keeps them in `librarycache/<id>/<hash>/`, older in `<id>/` or `<id>_name`.
fn steam_local_art(id: &str, file: &str) -> Option<PathBuf> {
    for root in steam_roots() {
        let cache = root.join("appcache/librarycache");
        let direct = [cache.join(id).join(file), cache.join(format!("{id}_{file}"))];
        if let Some(p) = direct.into_iter().find(|p| p.exists()) {
            return Some(p);
        }
        if let Ok(dir) = std::fs::read_dir(cache.join(id)) {
            for e in dir.flatten() {
                let p = e.path().join(file);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// What the store API says about a game.
#[derive(Clone, Debug, Default, PartialEq)]
struct SteamInfo {
    short: String,
    developers: Vec<String>,
    genres: Vec<String>,
    date: String,
    /// Full-size screenshot addresses.
    shots: Vec<String>,
    trailer: Option<(String, String)>,
}

/// A movie's playable address: the HLS playlist (it picks the quality by
/// itself, up to 1080p), else the biggest mp4 or webm.
fn movie_url(m: &Value) -> Option<String> {
    [&m["hls_h264"], &m["mp4"]["max"], &m["mp4"]["480"], &m["webm"]["max"], &m["webm"]["480"]]
        .into_iter().find_map(|v| v.as_str().and_then(https_url))
}

/// The best trailer: a launch trailer, then a reveal, then any trailer, then
/// the first highlight, then the first video.
fn pick_trailer(movies: &[Value]) -> Option<(String, String)> {
    let rank = |m: &Value| {
        let n = text(m, "name").to_lowercase();
        if n.contains("launch") { 0 } else if n.contains("reveal") { 1 } else if n.contains("trailer") { 2 }
        else if m["highlight"].as_bool() == Some(true) { 3 } else { 4 }
    };
    let best = movies.iter().filter(|m| movie_url(m).is_some()).min_by_key(|m| rank(m))?;
    Some((movie_url(best)?, text(best, "name")))
}

/// The pieces of a store API record (`data` of one app).
fn steam_info(d: &Value) -> SteamInfo {
    let list = |k: &str, f: &str| -> Vec<String> {
        d[k].as_array().into_iter().flatten().map(|x| if f.is_empty() { x.as_str().unwrap_or_default().trim().to_string() }
                                                         else { text(x, f) }).filter(|s| !s.is_empty()).collect()
    };
    SteamInfo {
        short: text(d, "short_description"),
        developers: list("developers", ""),
        genres: list("genres", "description"),
        date: text(&d["release_date"], "date"),
        shots: list("screenshots", "path_full").iter().filter_map(|u| https_url(u)).take(8).collect(),
        trailer: d["movies"].as_array().and_then(|m| pick_trailer(m)),
    }
}

/// The store API record for an app (cached 30 days).
fn steam_details(id: &str) -> Option<Value> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={id}\
                       &filters=basic,screenshots,movies,release_date,developers,genres");
    let v = fetch_json(&url)?;
    (v[id]["success"].as_bool() == Some(true)).then(|| v[id]["data"].clone())
}

/// Steam says some 800x450 screenshots are 1920x1080. When any screenshot is
/// really wide (1000 px or more), the narrow ones are dropped. Sizes that
/// can't be read are kept.
fn keep_big(shots: Vec<(PathBuf, Option<u32>)>) -> Vec<PathBuf> {
    let any_big = shots.iter().any(|(_, w)| w.is_some_and(|w| w >= 1000));
    shots.into_iter().filter(|(_, w)| !any_big || w.is_none_or(|w| w >= 1000)).map(|(p, _)| p).collect()
}

/// Steam's pictures, facts and trailer for an app, into `m`.
fn steam_fill(m: &mut Meta, id: &str) {
    let mut hero = steam_local_art(id, "library_hero.jpg");
    let mut logo = steam_local_art(id, "logo.png");
    let mut cover = steam_local_art(id, "library_600x900.jpg");
    let info = steam_details(id).map(|d| steam_info(&d)).unwrap_or_default();

    // Anything Steam's cache lacks comes from its CDN, with the screenshots, all at once.
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    for (slot, file) in [(&hero, "library_hero.jpg"), (&logo, "logo.png"), (&cover, "library_600x900.jpg")] {
        if slot.is_none() {
            let url = format!("{STEAM_CDN}/{id}/{file}");
            jobs.push((url.clone(), img_path(&url)));
        }
    }
    jobs.extend(info.shots.iter().map(|u| (u.clone(), img_path(u))));
    let mut got = fetch_files(&jobs).into_iter();
    for slot in [&mut hero, &mut logo, &mut cover] {
        if slot.is_none() {
            *slot = got.next().flatten();
        }
    }
    let shots = keep_big(got.flatten().map(|p| { let w = image_size(&p).map(|s| s.0); (p, w) }).collect());

    m.logo = logo;
    m.cover = cover.or(m.cover.take());
    m.backgrounds.extend(hero.clone());
    m.pictures.extend(hero);
    m.pictures.extend(shots);
    if m.backgrounds.is_empty() {
        m.backgrounds.extend(m.pictures.first().cloned());
    }
    if let Some((url, name)) = info.trailer {
        m.trailer = Some(url);
        m.trailer_name = name;
    }
    if !info.developers.is_empty() {
        m.subtitle = info.developers.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
        m.credits.push(("Developer".into(), info.developers.join(", ")));
    }
    if !info.date.is_empty() {
        m.credits.push(("Released".into(), info.date.clone()));
    }
    if !info.genres.is_empty() {
        m.credits.push(("Genre".into(), info.genres.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    m.bio = short_bio(&info.short, 220);
    m.facts = [year_in(&info.date), info.developers.first().cloned(), Some("Steam".to_string())]
        .into_iter().flatten().collect();
}

// ---------------------------------------------------------------------------
// Playnite (fan art on the NAS)
// ---------------------------------------------------------------------------

/// A game in Playnite's library: its id (as the text Playnite uses for folder
/// names) and the pictures it points at, relative to `library/files/`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct PnGame {
    id: String,
    name: String,
    bg: String,
    cover: String,
}

/// The small list kept in the cache, so the 43 MB `games.db` is read once.
#[derive(Serialize, Deserialize)]
struct PnIndex {
    root: String,
    games: Vec<PnGame>,
}

/// A GUID as C# writes it to a file: the first three fields little-endian.
fn guid_string(b: &[u8; 16]) -> String {
    format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{}",
            b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9],
            b[10..].iter().map(|x| format!("{x:02x}")).collect::<String>())
}

/// A walk along the fields of a BSON document.
struct Bson<'a> {
    b: &'a [u8],
    q: usize,
    end: usize,
}

impl<'a> Bson<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let to = self.q.checked_add(n)?;
        if to > self.end {
            return None;
        }
        let b: &'a [u8] = self.b;
        let s = &b[self.q..to];
        self.q = to;
        Some(s)
    }

    fn int(&mut self) -> Option<usize> {
        usize::try_from(i32::from_le_bytes(self.take(4)?.try_into().ok()?)).ok()
    }
}

/// A game from a BSON document that starts at `p`, if it is one: it has an
/// `_id` GUID, a `Name` and a background or cover picture. Nested documents
/// (completion status, platform…) have no picture fields, so they don't count.
fn pn_doc(b: &[u8], p: usize) -> Option<PnGame> {
    let len = usize::try_from(i32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?)).ok()?;
    if !(5..=(1 << 22)).contains(&len) || p.checked_add(len)? > b.len() || *b.get(p + len - 1)? != 0 {
        return None;
    }
    let mut d = Bson { b, q: p + 4, end: p + len - 1 };
    let mut g = PnGame::default();
    let mut pictures_field = false;
    while d.q < d.end {
        let t = d.take(1)?[0];
        let rest = &b[d.q..d.end];
        let z = rest.iter().position(|c| *c == 0)?;
        let key = std::str::from_utf8(&rest[..z]).ok()?.to_string();
        d.take(z + 1)?;
        let mut s = String::new();
        let mut guid = None;
        match t {
            0x02 => {
                let l = d.int()?;
                s = String::from_utf8_lossy(d.take(l)?).trim_end_matches('\0').to_string();
            }
            0x03 | 0x04 => {
                let l = d.int()?;
                d.take(l.checked_sub(4)?)?;
            }
            0x05 => {
                let l = d.int()?;
                let sub = d.take(1)?[0];
                let data = d.take(l)?;
                if sub == 4 && l == 16 {
                    guid = Some(guid_string(data.try_into().ok()?));
                }
            }
            0x01 | 0x09 | 0x11 | 0x12 => {
                d.take(8)?;
            }
            0x10 => {
                d.take(4)?;
            }
            0x08 => {
                d.take(1)?;
            }
            0x07 => {
                d.take(12)?;
            }
            0x13 => {
                d.take(16)?;
            }
            0x0A | 0x7F | 0xFF => {}
            _ => return None,
        }
        match key.as_str() {
            "_id" => g.id = guid.unwrap_or_default(),
            "Name" => g.name = s,
            "BackgroundImage" => {
                pictures_field = true;
                g.bg = s;
            }
            "CoverImage" => {
                pictures_field = true;
                g.cover = s;
            }
            _ => {}
        }
    }
    (!g.id.is_empty() && !g.name.is_empty() && pictures_field && !(g.bg.is_empty() && g.cover.is_empty())).then_some(g)
}

/// The games in a Playnite `games.db` (LiteDB): every document that starts
/// with an `_id` GUID, read as BSON. Later copies of a game win.
fn pn_scan(db: &[u8]) -> Vec<PnGame> {
    const MARK: [u8; 10] = [0x05, b'_', b'i', b'd', 0, 0x10, 0, 0, 0, 0x04];
    let mut games: Vec<PnGame> = Vec::new();
    let mut i = 4;
    while i + MARK.len() <= db.len() {
        if db[i] == 0x05 && db[i..].starts_with(&MARK) {
            if let Some(g) = pn_doc(db, i - 4) {
                match games.iter().position(|x| x.id == g.id) {
                    Some(k) => games[k] = g,
                    None => games.push(g),
                }
            }
            i += MARK.len();
        } else {
            i += 1;
        }
    }
    games
}

/// The library game with this name (names compared without punctuation).
fn pn_find<'a>(games: &'a [PnGame], name: &str) -> Option<&'a PnGame> {
    let want = norm(name);
    if want.is_empty() {
        return None;
    }
    games.iter().filter(|g| norm(&g.name) == want)
        .max_by_key(|g| u8::from(!g.bg.is_empty()) + u8::from(!g.cover.is_empty()))
}

/// Set while the library is being read, and when that last failed, so it is
/// tried once per run (and again after ten minutes when the NAS was away).
static PN_BUSY: AtomicBool = AtomicBool::new(false);
static PN_FAILED: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());

struct Unbusy;

impl Drop for Unbusy {
    fn drop(&mut self) {
        PN_BUSY.store(false, Ordering::SeqCst);
    }
}

/// Reads `games.db` and keeps the list of games in the cache.
fn pn_build(root: &Path, index: &Path) -> Option<Vec<PnGame>> {
    let games = pn_scan(&std::fs::read(root.join("library/games.db")).ok()?);
    if games.is_empty() {
        return None;
    }
    let idx = PnIndex { root: root.to_string_lossy().into_owned(), games: games.clone() };
    write_atomic(index, serde_json::to_string(&idx).ok()?.as_bytes());
    Some(games)
}

/// The Playnite games: from the cached list, or read from the NAS once (every
/// 30 days) on a thread that is given 6 seconds: the 43 MB file took nearly 4
/// from a cold NAS. The thread finishes by itself when the NAS is slower, and
/// the next lookup finds the list.
fn pn_index(root: &Path) -> Option<Vec<PnGame>> {
    let path = meta_dir().join("playnite-index.json");
    let key = root.to_string_lossy().into_owned();
    let cached: Option<PnIndex> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok())
        .filter(|i: &PnIndex| i.root == key);
    if cached.is_some() && fresh(&path) {
        return cached.map(|i| i.games);
    }
    let wait = secs_for(6.0);
    if let Some(wait) = wait
        && !recently(&PN_FAILED, &key, Duration::from_secs(600))
        && !PN_BUSY.swap(true, Ordering::SeqCst)
    {
        let (r, p) = (root.to_path_buf(), path.clone());
        match with_timeout(wait, move || {
            let _free = Unbusy;
            pn_build(&r, &p)
        }) {
            Some(Some(games)) => return Some(games),
            Some(None) => remember(&PN_FAILED, &key),
            None => {} // still reading; the list is there next time
        }
    }
    cached.map(|i| i.games)
}

/// A Playnite game's pictures, copied from the NAS into the cache, and its
/// trailer (the file stays on the NAS).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct PnAssets {
    bg: Option<PathBuf>,
    cover: Option<PathBuf>,
    trailer: Option<String>,
}

impl PnAssets {
    fn is_empty(&self) -> bool {
        self.bg.is_none() && self.cover.is_none() && self.trailer.is_none()
    }

    fn files_exist(&self) -> bool {
        [&self.bg, &self.cover].into_iter().flatten().all(|p| p.exists())
    }
}

/// Copies one of Playnite's files into the cache (under 15 MB).
fn pn_copy(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains("..") {
        return None;
    }
    let rel = rel.replace('\\', "/");
    let src = root.join("library/files").join(&rel);
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
    let dest = meta_dir().join(format!("pn-{:016x}.{ext}", fnv(&rel)));
    if dest.exists() {
        return Some(dest);
    }
    if std::fs::metadata(&src).ok()?.len() > 15_000_000 {
        return None;
    }
    // Read and write rather than copy, so the file doesn't keep the NAS's permissions.
    write_atomic(&dest, &std::fs::read(&src).ok()?);
    dest.exists().then_some(dest)
}

fn pn_assets(root: &Path, g: &PnGame) -> PnAssets {
    let item = meta_dir().join(format!("pn-{:016x}.json", fnv(&g.id)));
    let cached: Option<PnAssets> = std::fs::read_to_string(&item).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let Some(a) = &cached
        && fresh(&item)
        && a.files_exist()
    {
        return a.clone();
    }
    let Some(wait) = secs_for(3.0) else { return cached.unwrap_or_default() };
    if recently(&PN_FAILED, &g.id, Duration::from_secs(600)) {
        return cached.unwrap_or_default();
    }
    remember(&PN_FAILED, &g.id);
    let (root, g, item2) = (root.to_path_buf(), g.clone(), item.clone());
    with_timeout(wait, move || {
        let trailer = root.join("ExtraMetadata/Games").join(&g.id).join("VideoTrailer.mp4");
        let a = PnAssets {
            bg: pn_copy(&root, &g.bg),
            cover: pn_copy(&root, &g.cover),
            trailer: trailer.is_file().then(|| trailer.to_string_lossy().into_owned()),
        };
        if !a.is_empty()
            && let Ok(t) = serde_json::to_string(&a)
        {
            write_atomic(&item2, t.as_bytes());
        }
        a
    }).unwrap_or_else(|| cached.unwrap_or_default())
}

/// The user's fan art for a game, into `m` (after Steam's art).
fn playnite_fill(m: &mut Meta, name: &str, root: &Path) {
    if no_net() {
        return;
    }
    let Some(games) = pn_index(root) else { return };
    let Some(g) = pn_find(&games, name) else { return };
    let a = pn_assets(root, g);
    if let Some(bg) = a.bg {
        m.backgrounds.push(bg.clone());
        m.pictures.push(bg);
    }
    m.cover = m.cover.take().or(a.cover);
    if m.trailer.is_none()
        && let Some(t) = a.trailer
    {
        m.trailer = Some(t);
        m.trailer_name = "Trailer".into();
    }
}

/// No network (or NAS) lookups: a dry run, unless SBCOMMAND_META_LIVE asks for real pictures.
fn no_net() -> bool {
    card::dry_run() && std::env::var_os("SBCOMMAND_META_LIVE").is_none()
}

fn playnite_root() -> PathBuf {
    std::env::var_os("SBCOMMAND_PLAYNITE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(PLAYNITE_DEFAULT))
}

// ---------------------------------------------------------------------------
// Music (TheAudioDB, MusicBrainz, Cover Art Archive)
// ---------------------------------------------------------------------------

/// The artist's TheAudioDB record, when its name is the one asked for.
fn audiodb_artist(name: &str) -> Option<Value> {
    let v = fetch_json(&format!("{AUDIODB}/search.php?s={}", url_encode(name)))?;
    v["artists"].as_array()?.iter().find(|a| norm_artist(&text(a, "strArtist")) == norm_artist(name)).cloned()
}

/// The album's TheAudioDB record (its year, genre and cover).
fn audiodb_album(artist: &str, album: &str) -> Option<Value> {
    let v = fetch_json(&format!("{AUDIODB}/searchalbum.php?s={}&a={}", url_encode(artist), url_encode(album)))?;
    v["album"].as_array()?.iter().find(|a| norm(&text(a, "strAlbum")) == norm(album)).cloned()
}

/// A MusicBrainz id: 36 characters of hex and dashes (it goes into an address).
fn is_mbid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// (release group id, title) of the artist's studio albums from a MusicBrainz
/// search, oldest first, without the one that's playing.
fn studio_albums(mb: &Value, playing: &str) -> Vec<(String, String)> {
    let skip = norm(playing);
    let mut v: Vec<(String, String, String)> = mb["release-groups"].as_array().into_iter().flatten()
        .filter(|r| r["secondary-types"].as_array().is_none_or(|t| t.is_empty()))
        .map(|r| (Some(text(r, "first-release-date")).filter(|d| !d.is_empty()).unwrap_or_else(|| "9999".into()),
                  text(r, "id"), text(r, "title")))
        .filter(|(_, id, title)| is_mbid(id) && !title.is_empty() && norm(title) != skip)
        .collect();
    v.sort();
    v.dedup_by(|a, b| norm(&a.2) == norm(&b.2));
    v.into_iter().map(|(_, id, title)| (id, title)).collect()
}

/// `n` items spread over the whole list (all of it when it's shorter).
fn spread<T: Clone>(v: &[T], n: usize) -> Vec<T> {
    if v.len() <= n {
        return v.to_vec();
    }
    (0..n).map(|i| v[i * (v.len() - 1) / (n - 1).max(1)].clone()).collect()
}

/// The Cover Art Archive pictures to try for the record shelf: other studio
/// albums by the artist, found through the artist's MusicBrainz id. Some
/// albums have no cover art, so this offers a few more than the shelf holds.
fn record_urls(mbid: &str, playing: &str) -> Vec<String> {
    let q = format!("arid:{mbid} AND primarytype:album AND NOT secondarytype:*");
    let Some(mb) = fetch_json(&format!("https://musicbrainz.org/ws/2/release-group/?query={}&fmt=json&limit=40", url_encode(&q)))
    else { return vec![] };
    spread(&studio_albums(&mb, playing), 6).into_iter()
        .map(|(id, _)| format!("https://coverartarchive.org/release-group/{id}/front-500")).collect()
}

/// What a picture to fetch is for.
#[derive(Clone, Copy)]
enum Pic {
    Photo,
    Logo,
    Cover,
    Record,
}

/// Songs: the artist's photos, logo and bio, the album's year, other albums.
fn music_fill(m: &mut Meta, np: &NowPlaying, live: Option<&Live>) {
    let artist = first_artist(&np.artist);
    let album = clean_album(&np.album);
    let podcast = np.kind == "Podcast"; // the "artist" is a show, which isn't in a music database
    let info = (!podcast && !artist.is_empty()).then(|| audiodb_artist(artist)).flatten();
    let alb = (info.is_some() && !album.is_empty()).then(|| audiodb_album(artist, &album)).flatten();

    // Photos, logo, record covers and (without the player's own) cover, all at once.
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    let mut pics: Vec<Pic> = Vec::new();
    let mut want = |pic: Pic, url: &str| {
        if let Some(u) = https_url(url) {
            jobs.push((u.clone(), img_path(&u)));
            pics.push(pic);
        }
    };
    if let Some(a) = &info {
        for k in ["strArtistFanart", "strArtistFanart2", "strArtistFanart3", "strArtistFanart4"] {
            want(Pic::Photo, &text(a, k));
        }
        want(Pic::Logo, &text(a, "strArtistLogo"));
    }
    if m.cover.is_none()
        && let Some(al) = &alb
    {
        want(Pic::Cover, &text(al, "strAlbumThumb"));
    }
    if let Some(mbid) = info.as_ref().map(|a| text(a, "strMusicBrainzID")).filter(|id| is_mbid(id)) {
        for url in record_urls(&mbid, &album) {
            want(Pic::Record, &url);
        }
    }
    for (pic, got) in pics.iter().zip(fetch_files(&jobs)) {
        let Some(p) = got else { continue };
        match pic {
            Pic::Photo => m.backgrounds.push(p),
            Pic::Logo => m.logo = Some(p),
            Pic::Cover => m.cover = Some(p),
            Pic::Record if m.records.len() < 5 => m.records.push(p),
            Pic::Record => {}
        }
    }

    let year = alb.as_ref().map(|a| text(a, "intYearReleased")).filter(|y| is_year(y));
    let genre = alb.as_ref().map(|a| text(a, "strGenre")).filter(|g| !g.is_empty())
        .or_else(|| info.as_ref().map(|a| text(a, "strGenre")).filter(|g| !g.is_empty()));
    if let Some(t) = track_number(&np.extra) {
        m.credits.push(("Track".into(), t));
    }
    if !album.is_empty() {
        m.credits.push(("Album".into(), year.map_or(album.clone(), |y| format!("{album} ({y})"))));
    }
    if let Some(g) = &genre {
        m.credits.push(("Genre".into(), g.clone()));
    }
    if let Some(l) = np.length.filter(|l| *l > 0.0) {
        m.credits.push(("Length".into(), clock(l)));
    }
    if let Some(l) = live {
        m.venue = l.venue.clone();
        m.date = l.date.clone();
        let place = [l.venue.as_str(), l.date.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(", ");
        if !place.is_empty() {
            m.credits.push(("Recorded live".into(), place));
        }
    }
    if let Some(a) = &info {
        // The English text is "strBiography" (an older name was "strBiographyEN").
        let bio = Some(text(a, "strBiographyEN")).filter(|b| !b.is_empty()).unwrap_or_else(|| text(a, "strBiography"));
        m.bio = short_bio(&bio, 220);
        let formed = text(a, "intFormedYear");
        m.facts = [Some(formed).filter(|y| is_year(y)).map(|y| format!("Formed {y}")),
                   Some(text(a, "strGenre")).filter(|g| !g.is_empty()),
                   Some(text(a, "strCountry")).filter(|c| !c.is_empty())].into_iter().flatten().collect();
    }
}

// ---------------------------------------------------------------------------
// Films and series (TMDB, only with a key)
// ---------------------------------------------------------------------------

/// What TMDB knows about a title. Picture fields are paths on TMDB's image server.
#[derive(Clone, Debug, Default, PartialEq)]
struct TmdbInfo {
    overview: String,
    year: String,
    /// The director, or for a series who created it.
    maker: String,
    poster: Option<String>,
    backdrops: Vec<String>,
    logo: Option<String>,
}

/// The show or film name to search for, and its year when known:
/// "Low Tide - S02E05" is the show before the numbers; "Starfall (2025).mkv"
/// is "Starfall", 2025.
fn tmdb_query(np: &NowPlaying, kind: &Kind) -> (String, Option<String>) {
    let mut t = np.title.trim().to_string();
    let mut year = year_in(&np.artist).filter(|_| np.artist.trim().len() == 4);
    if *kind == Kind::Tv {
        match sxxeyy(&t.to_ascii_lowercase()) {
            Some((i, ..)) if i > 0 => t.truncate(i),
            _ if !np.artist.trim().is_empty() && year.is_none() => t = np.artist.trim().to_string(),
            _ => {}
        }
        t = t.trim_end_matches([' ', '-', '–', '.', '_', ':']).to_string();
    }
    for ext in [".mkv", ".mp4", ".avi", ".m4v", ".webm"] {
        if t.to_ascii_lowercase().ends_with(ext) {
            t.truncate(t.len() - ext.len());
        }
    }
    if let Some(i) = t.rfind(['(', '['])
        && let Some(y) = year_in(&t[i..])
    {
        year = year.or(Some(y));
        t.truncate(i);
    }
    (t.trim().to_string(), year)
}

/// The useful parts of a TMDB movie or series record (fetched with credits and images).
fn tmdb_parse(d: &Value, tv: bool) -> TmdbInfo {
    let names = |v: &Value, job: Option<&str>| -> String {
        v.as_array().into_iter().flatten().filter(|p| job.is_none_or(|j| p["job"].as_str() == Some(j)))
            .map(|p| text(p, "name")).filter(|n| !n.is_empty()).take(2).collect::<Vec<_>>().join(", ")
    };
    let mut backdrops: Vec<String> = Vec::new();
    for p in std::iter::once(&d["backdrop_path"]).chain(d["images"]["backdrops"].as_array().into_iter().flatten().take(3).map(|b| &b["file_path"])) {
        if let Some(p) = p.as_str().filter(|p| p.starts_with('/')) && !backdrops.iter().any(|b| b == p) {
            backdrops.push(p.to_string());
        }
    }
    TmdbInfo {
        overview: text(d, "overview"),
        year: year_in(&text(d, if tv { "first_air_date" } else { "release_date" })).unwrap_or_default(),
        maker: if tv { names(&d["created_by"], None) } else { names(&d["credits"]["crew"], Some("Director")) },
        poster: d["poster_path"].as_str().filter(|p| p.starts_with('/')).map(str::to_string),
        backdrops,
        logo: d["images"]["logos"].as_array().into_iter().flatten()
            .filter_map(|l| l["file_path"].as_str()).find(|p| p.starts_with('/') && p.ends_with(".png")).map(str::to_string),
    }
}

/// HOOK: films and series from TMDB (themoviedb.org). Returns None unless the
/// SBCOMMAND_TMDB_KEY environment variable holds a free API key, and in a dry
/// run. With a key it searches by title (and year), then reads the record with
/// its credits and English or text-free images.
fn tmdb(np: &NowPlaying, kind: &Kind) -> Option<TmdbInfo> {
    let key = TMDB_KEY.lock().ok().map(|k| k.clone()).filter(|k| !k.is_empty())
        .or_else(|| std::env::var("SBCOMMAND_TMDB_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))?;
    if no_net() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let tv = *kind == Kind::Tv;
    let (what, name_key, year_key) = if tv { ("tv", "name", "first_air_date_year") } else { ("movie", "title", "year") };
    let (query, year) = tmdb_query(np, kind);
    if query.is_empty() {
        return None;
    }
    let mut url = format!("{TMDB}/search/{what}?api_key={key}&include_adult=false&query={}", url_encode(&query));
    if let Some(y) = year {
        url.push_str(&format!("&{year_key}={y}"));
    }
    let found = fetch_json(&url)?;
    let results = found["results"].as_array()?;
    let hit = results.iter().find(|r| norm(&text(r, name_key)) == norm(&query)).or_else(|| results.first())?;
    let id = hit["id"].as_u64()?;
    let d = fetch_json(&format!("{TMDB}/{what}/{id}?api_key={key}&append_to_response=credits,images&include_image_language=en,null"))?;
    Some(tmdb_parse(&d, tv))
}

/// TMDB's pictures and details into `m`, ahead of the player's own art.
fn tmdb_fill(m: &mut Meta, info: TmdbInfo, kind: &Kind) {
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    let mut add = |size: &str, path: &str| {
        let url = format!("{TMDB_IMG}/{size}{path}");
        jobs.push((url.clone(), img_path(&url)));
    };
    if let Some(p) = &info.poster {
        add("w500", p);
    }
    let n_back = info.backdrops.len();
    for b in &info.backdrops {
        add("w1280", b);
    }
    if let Some(l) = &info.logo {
        add("w500", l);
    }
    let mut got = fetch_files(&jobs).into_iter();
    if info.poster.is_some() {
        m.cover = got.next().flatten().or(m.cover.take());
    }
    let mut back: Vec<PathBuf> = got.by_ref().take(n_back).flatten().collect();
    // The stills also take turns on the cinema screen in the room.
    m.pictures = back.clone();
    back.append(&mut m.backgrounds);
    m.backgrounds = back;
    if info.logo.is_some() {
        m.logo = got.next().flatten();
    }
    m.bio = short_bio(&info.overview, 220);
    if !info.maker.is_empty() {
        let label = if *kind == Kind::Tv { "Created by" } else { "Director" };
        m.credits.push((label.into(), info.maker.clone()));
        if m.subtitle.is_empty() || is_year(&m.subtitle) {
            m.subtitle = info.maker;
        }
    }
    if !info.year.is_empty() && !m.facts.contains(&info.year) {
        m.facts.insert(0, info.year);
    }
}

// ---------------------------------------------------------------------------
// Putting it together
// ---------------------------------------------------------------------------

/// What's known without asking anyone.
fn base(np: &NowPlaying, kind: &Kind) -> Meta {
    let title = np.title.trim();
    Meta {
        kind: Some(kind.clone()),
        title: if title.is_empty() { np.app.clone() } else { title.to_string() },
        subtitle: np.artist.trim().to_string(),
        kicker_app: np.app.clone(),
        cover: np.art.clone().filter(|p| p.exists()),
        ..Default::default()
    }
}

fn fill(m: &mut Meta, np: &NowPlaying, kind: &Kind, playnite: &Path) {
    match kind {
        Kind::Studio | Kind::LiveSmall | Kind::LiveStadium => music_fill(m, np, live_info(np).as_ref()),
        Kind::Game => {
            if let Some(id) = steam_appid_for_name(&np.title) {
                steam_fill(m, &id);
            }
            playnite_fill(m, &np.title, playnite);
            if m.cover.is_none() {
                m.cover = np.art.clone();
            }
        }
        Kind::Film | Kind::Tv => {
            m.backgrounds.extend(m.cover.clone());
            if let Some(y) = year_in(&np.artist).filter(|_| np.artist.trim().len() == 4) {
                m.facts.push(y);
            }
            if *kind == Kind::Tv {
                let n = [&np.title, &np.extra, &np.album].iter().find_map(|t| tv_numbers(t)).unwrap_or_default();
                if let Some(s) = n.season {
                    m.credits.push(("Season".into(), s.to_string()));
                }
                if let Some(e) = n.episode {
                    m.credits.push(("Episode".into(), e.to_string()));
                }
            }
            if let Some(l) = np.length.filter(|l| *l > 0.0) {
                m.credits.push(("Length".into(), clock(l)));
            }
            if let Some(info) = tmdb(np, kind) {
                tmdb_fill(m, info, kind);
            }
        }
        Kind::Sport | Kind::Video | Kind::Stream => {}
    }
}

/// What's playing, with its pictures and details: from the cache, else
/// fetched (at most about 14 seconds). Blocks, so call it on the worker
/// thread. Never panics; it returns what it has when something fails.
pub fn lookup(np: &NowPlaying) -> Meta {
    lookup_in(np, &playnite_root())
}

/// The TMDB key from Settings (the environment variable still works too).
static TMDB_KEY: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Settings that shape a lookup (from the app's Settings page).
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Look things up online at all. Off: only what the player and the game's
    /// own files say.
    pub online: bool,
    pub tmdb_key: String,
    /// Playnite's library folder; empty: the default (or SBCOMMAND_PLAYNITE).
    pub playnite: String,
}

/// `lookup` with the app's settings.
pub fn lookup_with(np: &NowPlaying, o: &Options) -> Meta {
    if !o.online {
        let kind = classify(np);
        return base(np, &kind);
    }
    if let Ok(mut k) = TMDB_KEY.lock() {
        *k = o.tmdb_key.trim().to_string();
    }
    let root = if o.playnite.trim().is_empty() { playnite_root() } else { PathBuf::from(o.playnite.trim()) };
    lookup_in(np, &root)
}

/// `lookup` with the Playnite library folder given.
pub fn lookup_in(np: &NowPlaying, playnite: &Path) -> Meta {
    start_budget();
    let kind = classify(np);
    let mut m = base(np, &kind);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fill(&mut m, np, &kind, playnite)));
    if r.is_err() {
        eprintln!("sbcommand: now-playing lookup failed; showing what is known");
    }
    m
}

// ---------------------------------------------------------------------------
// Dry-run demo
// ---------------------------------------------------------------------------

/// A demo picture: `SBCOMMAND_DEMO_ART`, else `assets/demo/<file>` next to where the app runs.
fn demo_art(file: &str) -> Option<PathBuf> {
    std::env::var("SBCOMMAND_DEMO_ART").ok().map(PathBuf::from).filter(|p| p.exists())
        .or_else(|| Some(Path::new("assets/demo").join(file)).filter(|p| p.exists()))
}

/// Made-up pages for screenshots and the self-test: invented names only, no network.
pub fn demo(kind: &str) -> Meta {
    let kind = Kind::from_id(kind).unwrap_or(Kind::Studio);
    let art = demo_art(match &kind {
        k if k.is_music() => "music.png",
        Kind::Game => "game.png",
        _ => "film.png",
    });
    let pics = |n: usize| art.iter().cycle().take(n).cloned().collect::<Vec<_>>();
    let c = |k: &str, v: &str| (k.to_string(), v.to_string());
    let facts = |f: &[&str]| f.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let music_bio = "The Midnight Static are a four-piece synth-rock band from a fictional harbour town. \
                     Their music mixes drum machines with live guitars.";
    let mut m = Meta {
        kind: Some(kind.clone()),
        cover: art.clone(),
        backgrounds: pics(1),
        ..Default::default()
    };
    match kind {
        Kind::Studio | Kind::LiveSmall | Kind::LiveStadium => {
            m.title = "Neon Rain".into();
            m.subtitle = "The Midnight Static".into();
            m.kicker_app = "Spotify".into();
            m.bio = music_bio.into();
            m.facts = facts(&["Formed 2011", "Synthwave", "Harbour Town"]);
            m.records = pics(5);
            m.credits = vec![c("Track", "1"), c("Album", "Afterglow (2019)"), c("Genre", "Synthwave"), c("Length", "4:32")];
            match kind {
                Kind::LiveSmall => {
                    m.title = "Neon Rain (Live)".into();
                    (m.venue, m.date) = ("The Copper Kettle Club".into(), "14 March 2019".into());
                }
                Kind::LiveStadium => {
                    m.title = "Neon Rain (Live)".into();
                    (m.venue, m.date) = ("Northgate Park Stadium".into(), "9 July 2022".into());
                }
                _ => {}
            }
            if kind.is_live() {
                m.credits.push(c("Recorded live", &format!("{}, {}", m.venue, m.date)));
            }
        }
        Kind::Film => {
            m.title = "Starfall".into();
            m.subtitle = "Ines Calloway".into();
            m.kicker_app = "Jellyfin".into();
            m.bio = "A lighthouse keeper on a drifting world finds a signal that was meant for someone else.".into();
            m.facts = facts(&["2025", "Science fiction", "2h 46m"]);
            m.credits = vec![c("Director", "Ines Calloway"), c("Length", "2 h 46 min")];
            m.pictures = pics(3);
        }
        Kind::Tv => {
            m.title = "Low Tide".into();
            m.subtitle = "The Lighthouse Years".into();
            m.kicker_app = "Jellyfin".into();
            m.bio = "A storm cuts the island off, and the ferry crew take shelter in the old signal station.".into();
            m.facts = facts(&["2023", "Drama"]);
            m.credits = vec![c("Season", "2"), c("Episode", "5"), c("Length", "52:10")];
            m.pictures = pics(3);
        }
        Kind::Game => {
            m.title = "Night Circuit".into();
            m.subtitle = "Paper Moon Studio".into();
            m.kicker_app = "Steam".into();
            m.bio = "Race a neon city after dark, one rain-slick lap at a time.".into();
            m.facts = facts(&["2024", "Paper Moon Studio", "Steam"]);
            m.credits = vec![c("Developer", "Paper Moon Studio"), c("Released", "3 May, 2024"), c("Genre", "Racing")];
            m.pictures = pics(4);
        }
        Kind::Sport => {
            m.title = "Harbour City v Northgate United".into();
            m.subtitle = "Coastal League".into();
            m.kicker_app = "Twitch".into();
            m.facts = facts(&["Matchday 12"]);
        }
        Kind::Video => {
            m.title = "Building a Cabin in the Hills".into();
            m.subtitle = "Fell & Fern".into();
            m.kicker_app = "YouTube".into();
            m.facts = facts(&["18:42"]);
        }
        Kind::Stream => {
            m.title = "Live channel".into();
            m.kicker_app = "Ace Stream".into();
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn np(activity: &'static str, app: &str, title: &str) -> NowPlaying {
        NowPlaying { activity, app: app.into(), title: title.into(), ..Default::default() }
    }

    fn song(title: &str, album: &str) -> NowPlaying {
        NowPlaying { album: album.into(), artist: "Someone".into(), ..np("music", "Spotify", title) }
    }

    #[test]
    fn games_are_games() {
        assert_eq!(classify(&np("game", "Steam", "Rocket League")), Kind::Game);
        assert_eq!(classify(&np("game", "Steam", "")), Kind::Game);
    }

    #[test]
    fn songs_are_studio_unless_they_say_live() {
        for (t, a) in [("Everlong", "The Colour And The Shape"), ("Live Forever", "Definitely Maybe"),
                       ("Live and Let Die", "Band on the Run"), ("Alive", "Ten"), ("Lively Up Yourself", "Natty Dread"),
                       ("Olive Tree", "Delivery")] {
            assert_eq!(classify(&song(t, a)), Kind::Studio, "{t} / {a}");
        }
        for (t, a) in [("Song", "Live at Wembley"), ("Song (Live)", "Greatest Hits"), ("Song - Live at Reading", ""),
                       ("Song [Live]", ""), ("Song live", ""), ("Hey", "MTV Unplugged in New York"),
                       ("Hey", "Unplugged"), ("Hey (In Concert)", ""), ("Hey", "Tour In Concert 1994"),
                       ("Hey", "Live 8"), ("Song (live in Paris)", "")] {
            assert!(classify(&song(t, a)).is_live(), "{t} / {a}");
        }
    }

    #[test]
    fn venues_are_read_from_album_or_title() {
        assert_eq!(live_venue("Live at Wembley Stadium (Remastered)"), Some(("Wembley Stadium".into(), String::new())));
        assert_eq!(live_venue("Live in Paris, 1990"), Some(("Paris".into(), "1990".into())));
        assert_eq!(live_venue("Live from the Royal Albert Hall - Deluxe"), Some(("the Royal Albert Hall".into(), String::new())));
        assert_eq!(live_venue("Song - Live at Wembley Stadium, 12 July 1986"),
                   Some(("Wembley Stadium".into(), "12 July 1986".into())));
        assert_eq!(live_venue("Live at Reading Festival Deluxe Edition"), Some(("Reading Festival".into(), String::new())));
        assert_eq!(live_venue("Live Forever"), None);
        assert_eq!(live_venue("Unplugged"), None);
        assert_eq!(live_venue("MTV Unplugged In New York"), Some(("New York".into(), String::new())));
        let mut n = song("Song", "Live at Donington 1992");
        assert_eq!(live_info(&n), Some(Live { venue: "Donington".into(), date: "1992".into() }));
        n.album = String::new();
        n.title = "Song (Live at The Marquee)".into();
        assert_eq!(live_info(&n).unwrap().venue, "The Marquee");
        n.title = "Song (Live)".into();
        assert_eq!(live_info(&n), Some(Live { venue: String::new(), date: String::new() }));
        assert_eq!(live_info(&song("Song", "Greatest Hits")), None);
    }

    #[test]
    fn stadiums_and_small_rooms() {
        for v in ["Wembley Stadium", "Estadio Azteca", "Stadion Wankdorf", "Stade de France", "Wembley Arena", "Wembley",
                  "Tokyo Dome", "Houston Astrodome", "Rose Bowl", "Hollywood Bowl", "Wrigley Field", "Soldier Field",
                  "Hyde Park", "Finsbury Park", "Murrayfield", "O2 Arena", "Allstate Arena, Chicago"] {
            assert!(is_stadium(v), "{v}");
        }
        for v in ["", "The Marquee", "Royal Albert Hall", "Parklife Festival", "Sheffield", "Madison Park Theatre",
                  "The Fields Club", "Reading Festival", "Paris", "Freedom Hall", "Syndrome Room"] {
            assert!(!is_stadium(v), "{v}");
        }
        assert_eq!(classify(&song("Song", "Live at Wembley Stadium")), Kind::LiveStadium);
        assert_eq!(classify(&song("Song", "Live in Hyde Park")), Kind::LiveStadium);
        assert_eq!(classify(&song("Song", "Live at the Marquee Club")), Kind::LiveSmall);
        assert_eq!(classify(&song("Song", "Live at Parklife")), Kind::LiveSmall);
        assert_eq!(classify(&song("Song", "Live in Sheffield")), Kind::LiveSmall);
        assert_eq!(classify(&song("Song (Live)", "")), Kind::LiveSmall, "no venue: small");
        assert_eq!(classify(&song("Hey", "MTV Unplugged in New York")), Kind::LiveSmall);
    }

    #[test]
    fn series_are_told_by_their_numbers() {
        for t in ["Show S01E03", "Show - s02e10 - Title", "Show S1 E3", "Show.S01.E03.1080p", "The Show: Season 2",
                  "Show Episode 5", "Show s01.e03", "Show S1:E3"] {
            assert_eq!(classify(&np("movie", "Jellyfin", t)), Kind::Tv, "{t}");
        }
        let mut n = np("movie", "Jellyfin", "Low Tide");
        n.extra = "Season 2 Episode 5".into();
        assert_eq!(classify(&n), Kind::Tv, "numbers in the extra field count");
        n.extra = String::new();
        n.album = "S02E05".into();
        assert_eq!(classify(&n), Kind::Tv, "numbers in the album field count");
        for t in ["Seasons Greetings", "Passengers", "Es01", "Star Wars Episode IV", "Alien"] {
            assert_ne!(classify(&np("movie", "Jellyfin", t)), Kind::Tv, "{t}");
        }
        assert_eq!(tv_numbers("Show S02E05"), Some(TvNum { season: Some(2), episode: Some(5) }));
        assert_eq!(tv_numbers("Season 3"), Some(TvNum { season: Some(3), episode: None }));
        assert_eq!(tv_numbers("Episode 12"), Some(TvNum { season: None, episode: Some(12) }));
        assert_eq!(tv_numbers("Alien"), None);
    }

    #[test]
    fn matches_and_races_are_sport() {
        for t in ["Arsenal v Chelsea", "Arsenal vs Chelsea", "Arsenal vs. Chelsea", "Premier League: Highlights",
                  "NBA Finals Game 4", "F1 2025 Monaco", "Formula 1 Qualifying", "British Grand Prix", "UFC 300",
                  "Champions League Final", "NFL Week 3"] {
            assert_eq!(classify(&np("movie", "Zen", t)), Kind::Sport, "{t}");
        }
        assert_eq!(classify(&np("movie", "YouTube", "Arsenal v Chelsea")), Kind::Sport, "before the video-site rule");
        assert_eq!(classify(&np("movie", "Jellyfin", "Batman v Superman")), Kind::Film, "library films keep their v");
        assert_eq!(classify(&np("movie", "Jellyfin", "Premier League: Arsenal v Chelsea")), Kind::Sport);
        for t in ["Vsevolod", "Dave", "Nbarbecue", "F10 Keys", "Serie B", "Series A"] {
            assert_ne!(classify(&np("movie", "Zen", t)), Kind::Sport, "{t}");
        }
    }

    #[test]
    fn video_sites_streams_and_films() {
        assert_eq!(classify(&np("movie", "YouTube", "Cabin build")), Kind::Video);
        assert_eq!(classify(&np("movie", "Twitch", "Some streamer")), Kind::Video);
        let mut n = np("movie", "Zen", "A clip");
        n.url = "https://vimeo.com/123".into();
        assert_eq!(classify(&n), Kind::Video);
        n.url = "https://www.dailymotion.com/video/x".into();
        assert_eq!(classify(&n), Kind::Video);
        assert_eq!(classify(&np("movie", "Jellyfin", "Starfall")), Kind::Film);
        assert_eq!(classify(&np("movie", "Netflix", "Alien")), Kind::Film);
        assert_eq!(classify(&np("movie", "Jellyfin", "")), Kind::Stream, "no title");
        assert_eq!(classify(&np("movie", "Jellyfin", "Jellyfin")), Kind::Stream, "the app name is no title");
        assert_eq!(classify(&np("movie", "Ace Stream", "Some channel")), Kind::Stream);
        assert_eq!(classify(&np("movie", "AceStream", "Some channel")), Kind::Stream);
        assert_eq!(classify(&np("movie", "VLC", "acestream://abcdef")), Kind::Stream, "a bare link");
        assert_eq!(classify(&np("movie", "mpv", "https://example.com/live.m3u8")), Kind::Stream);
        assert_eq!(classify(&np("movie", "mpv", "Film about a link")), Kind::Film);
    }

    #[test]
    fn other_activities() {
        let mut n = np("other", "Firefox", "Some page with sound");
        assert_eq!(classify(&n), Kind::Video, "a browser");
        n.app = "Zen".into();
        assert_eq!(classify(&n), Kind::Video);
        n = np("other", "Some app", "Some audio");
        n.kind = "Video".into();
        assert_eq!(classify(&n), Kind::Video, "the kind says it came from a browser");
        n = np("other", "Firefox", "");
        assert_eq!(classify(&n), Kind::Stream, "no title");
        assert_eq!(classify(&np("other", "Firefox", "Firefox")), Kind::Stream);
        assert_eq!(classify(&np("other", "Some app", "Some audio")), Kind::Stream);
        assert_eq!(classify(&np("", "", "")), Kind::Stream);
    }

    #[test]
    fn kind_ids() {
        let all = [Kind::Studio, Kind::LiveSmall, Kind::LiveStadium, Kind::Film, Kind::Tv, Kind::Game, Kind::Sport,
                   Kind::Video, Kind::Stream];
        let ids: Vec<&str> = all.iter().map(Kind::id).collect();
        assert_eq!(ids, ["studio", "live", "stadium", "film", "tv", "game", "sport", "video", "stream"]);
        for k in &all {
            assert_eq!(Kind::from_id(k.id()).as_ref(), Some(k));
        }
        assert_eq!(Kind::from_id("music"), Some(Kind::Studio));
        assert_eq!(Kind::from_id("movie"), Some(Kind::Film));
        assert_eq!(Kind::from_id("nonsense"), None);
    }

    #[test]
    fn text_helpers() {
        assert_eq!(norm("Mass Effect™ Legendary Edition"), norm("mass effect: legendary edition"));
        assert_eq!(norm("Earth & Fire"), norm("Earth and Fire"));
        assert_eq!(norm_artist("The Beatles"), norm_artist("Beatles"));
        assert_eq!(norm_artist("The"), "the");
        assert_eq!(clock(250.0), "4:10");
        assert_eq!(clock(6128.0), "1 h 42 min");
        assert_eq!(track_number("Track 11"), Some("11".into()));
        assert_eq!(track_number(""), None);
        assert_eq!(clean_album("The Colour And The Shape (Deluxe Edition)"), "The Colour And The Shape");
        assert_eq!(clean_album("Abbey Road - Remastered 2009"), "Abbey Road");
        assert_eq!(first_artist("Foo Fighters, Someone"), "Foo Fighters");
        assert_eq!(year_in("14 May, 2021"), Some("2021".into()));
        assert_eq!(year_in("May 14, 2021"), Some("2021".into()));
        assert_eq!(year_in("Coming soon 123456"), None);
        assert_eq!(https_url("http://a.b/c"), Some("https://a.b/c".into()));
        assert_eq!(https_url("ftp://a"), None);
        assert_eq!(host_of("https://store.steampowered.com/api/x?y=1"), "store.steampowered.com");
    }

    #[test]
    fn bios_are_short_and_end_cleanly() {
        let long = "Foo Fighters are an American rock band formed in 1994 in Seattle, Washington. The band was founded by former \
                    Nirvana drummer Dave Grohl as a one-man project following the dissolution of Nirvana after the suicide of Kurt Cobain.";
        assert_eq!(short_bio(long, 220), "Foo Fighters are an American rock band formed in 1994 in Seattle, Washington.");
        assert_eq!(short_bio("One short. Two short. Three short.", 220), "One short. Two short.");
        assert_eq!(short_bio("Dr. Smith met Mr. Jones in St. Louis. They talked. Then left.", 220),
                   "Dr. Smith met Mr. Jones in St. Louis. They talked.");
        assert_eq!(short_bio("J. K. Rowling wrote it. It sold.", 220), "J. K. Rowling wrote it. It sold.");
        assert_eq!(short_bio("Made in the U.S. in 1999. Fun.", 220), "Made in the U.S. in 1999. Fun.");
        let cut = short_bio(&format!("{} and more words follow here.", "word ".repeat(60)), 100);
        assert!(cut.ends_with('…') && cut.chars().count() <= 100, "{cut}");
        assert_eq!(short_bio("a a a a a packs — remastered and more.", 22), "a a a a a packs…", "no dangling dash before the ellipsis");
        assert_eq!(short_bio("", 220), "");
        assert_eq!(short_bio("Say &quot;hi&quot; &amp; <b>wave</b> &#39;bye&#39; &#x41;.<br/>Next one.", 220),
                   "Say \"hi\" & wave 'bye' A. Next one.");
        assert_eq!(clean_text("a < b and c > d"), "a < b and c > d");
    }

    #[test]
    fn steam_files_are_read() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"252950\"\n\t\"Universe\"\t\t\"1\"\n\t\"name\"\t\t\"Rocket League\"\n\
                   \t\"UserConfig\"\n\t{\n\t\t\"name\"\t\t\"Other\"\n\t}\n}\n";
        assert_eq!(parse_acf(acf), Some(("252950".into(), "Rocket League".into())));
        assert_eq!(parse_acf("\"AppState\"\n{\n}"), None);
        assert_eq!(vdf_pair("\t\t\"path\"\t\t\"/mnt/games\""), Some(("path", "/mnt/games")));
        assert_eq!(vdf_pair("{"), None);
        let env = b"HOME=/home/x\0SteamAppId=1328670\0PATH=/bin\0";
        assert_eq!(appid_from_environ(env), Some("1328670".into()));
        assert_eq!(appid_from_environ(b"STEAM_COMPAT_APP_ID=252950\0"), Some("252950".into()));
        assert_eq!(appid_from_environ(b"SteamAppId=0\0STEAM_COMPAT_APP_ID=\0"), None);
        assert_eq!(appid_from_environ(b""), None);
    }

    #[test]
    fn steam_records_give_facts_screenshots_and_the_best_trailer() {
        let d: Value = serde_json::from_str(r#"{
            "short_description": "Remastered &quot;trilogy&quot; of three games. More text.",
            "developers": ["BioWare"], "genres": [{"id":"1","description":"Action"},{"id":"3","description":"RPG"}],
            "release_date": {"coming_soon": false, "date": "14 May, 2021"},
            "screenshots": [{"id":0,"path_thumbnail":"http://x/t0.jpg","path_full":"http://x/ss_a.1920x1080.jpg?t=1"},
                            {"id":1,"path_thumbnail":"https://x/t1.jpg","path_full":"https://x/ss_b.1920x1080.jpg?t=1"}],
            "movies": [{"name":"Accolades Trailer","highlight":true,"hls_h264":"https://v/acc/hls.m3u8"},
                       {"name":"Comparison","highlight":true,"hls_h264":"https://v/cmp/hls.m3u8"},
                       {"name":"Reveal trailer","highlight":true,"hls_h264":"https://v/rev/hls.m3u8?t=1"}]}"#).unwrap();
        let i = steam_info(&d);
        assert_eq!(i.developers, ["BioWare"]);
        assert_eq!(i.genres, ["Action", "RPG"]);
        assert_eq!(i.date, "14 May, 2021");
        assert_eq!(i.shots, ["https://x/ss_a.1920x1080.jpg?t=1", "https://x/ss_b.1920x1080.jpg?t=1"], "http is upgraded");
        assert_eq!(i.trailer, Some(("https://v/rev/hls.m3u8?t=1".into(), "Reveal trailer".into())));
        assert_eq!(short_bio(&i.short, 220), "Remastered \"trilogy\" of three games. More text.");

        let m = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        let pick = |movies: Vec<Value>| pick_trailer(&movies).map(|t| t.1);
        assert_eq!(pick(vec![m(r#"{"name":"Gameplay","hls_h264":"https://a"}"#), m(r#"{"name":"Launch Trailer","mp4":{"max":"http://b"}}"#)]),
                   Some("Launch Trailer".into()));
        assert_eq!(pick(vec![m(r#"{"name":"Gameplay","hls_h264":"https://a"}"#), m(r#"{"name":"Tour","highlight":true,"webm":{"480":"https://b"}}"#)]),
                   Some("Tour".into()), "no trailer word: the first highlight");
        assert_eq!(pick(vec![m(r#"{"name":"Gameplay","hls_h264":"https://a"}"#), m(r#"{"name":"Tour","hls_h264":"https://b"}"#)]),
                   Some("Gameplay".into()), "else the first");
        assert_eq!(pick(vec![m(r#"{"name":"Launch Trailer"}"#)]), None, "no playable address");
        assert_eq!(movie_url(&m(r#"{"hls_h264":"https://h","mp4":{"max":"https://m"}}"#)), Some("https://h".into()));
        assert_eq!(movie_url(&m(r#"{"mp4":{"480":"http://small","max":"http://big"}}"#)), Some("https://big".into()));
        assert_eq!(steam_info(&Value::Null), SteamInfo::default());
    }

    /// A JPEG header with the given size, and a PNG one.
    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        v.extend([0u8; 14]);
        v.extend([0xFF, 0xE1, 0x00, 0x08, 1, 2, 3, 4, 5, 6]); // an EXIF-like block to skip
        v.extend([0xFF, 0xC0, 0x00, 0x11, 8]);
        v.extend(h.to_be_bytes());
        v.extend(w.to_be_bytes());
        v.extend([3, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 6, 0, 0, 0]);
        v
    }

    #[test]
    fn picture_sizes_come_from_headers() {
        assert_eq!(image_size_of(&mut Cursor::new(jpeg(1920, 1080))), Some((1920, 1080)));
        assert_eq!(image_size_of(&mut Cursor::new(jpeg(800, 450))), Some((800, 450)));
        assert_eq!(image_size_of(&mut Cursor::new(png(640, 360))), Some((640, 360)));
        assert_eq!(image_size_of(&mut Cursor::new(b"GIF89a\x20\x03\x58\x02 and then some more bytes".to_vec())), Some((800, 600)));
        assert_eq!(image_size_of(&mut Cursor::new(b"<html>not a picture at all, sorry</html>".to_vec())), None);
        assert_eq!(image_size_of(&mut Cursor::new(vec![0xFF, 0xD8, 0xFF, 0xE0])), None, "cut short");
        assert!(looks_like_image(&jpeg(1, 1)) && looks_like_image(&png(1, 1)) && !looks_like_image(b"<!DOCTYPE html>"));
    }

    #[test]
    fn narrow_screenshots_are_dropped_when_wide_ones_exist() {
        let p = |s: &str| PathBuf::from(s);
        let keep = keep_big(vec![(p("a"), Some(800)), (p("b"), Some(1920)), (p("c"), None), (p("d"), Some(1000))]);
        assert_eq!(keep, [p("b"), p("c"), p("d")]);
        let all_small = keep_big(vec![(p("a"), Some(800)), (p("b"), Some(600))]);
        assert_eq!(all_small, [p("a"), p("b")], "nothing bigger: keep what there is");
        assert!(keep_big(vec![]).is_empty());
    }

    #[test]
    fn picture_files_are_named_by_address() {
        let a = img_path("https://x.test/a/ss_1.1920x1080.jpg?t=1");
        assert_eq!(a, img_path("https://x.test/a/ss_1.1920x1080.jpg?t=2"), "the ?t= tail doesn't count");
        assert_ne!(a, img_path("https://x.test/a/ss_2.1920x1080.jpg"));
        assert_eq!(a.extension().unwrap(), "jpg");
        assert_eq!(img_path("https://x.test/logo.png").extension().unwrap(), "png");
        assert_eq!(img_path("https://x.test/album/thumb/abc.jpg/medium").extension().unwrap(), "jpg");
        assert_eq!(img_path("https://x.test/noextension").extension().unwrap(), "jpg");
        assert!(a.starts_with(system::cache_dir().join("meta")));
    }

    // Hand-made BSON, like LiteDB writes it.
    fn b_str(k: &str, v: &str) -> Vec<u8> {
        let mut o = vec![0x02];
        o.extend(k.as_bytes());
        o.push(0);
        o.extend(((v.len() + 1) as i32).to_le_bytes());
        o.extend(v.as_bytes());
        o.push(0);
        o
    }

    fn b_guid(k: &str, g: [u8; 16]) -> Vec<u8> {
        let mut o = vec![0x05];
        o.extend(k.as_bytes());
        o.push(0);
        o.extend(16i32.to_le_bytes());
        o.push(4);
        o.extend(g);
        o
    }

    fn b_doc(fields: Vec<Vec<u8>>) -> Vec<u8> {
        let body: Vec<u8> = fields.concat();
        let mut o = ((body.len() + 5) as i32).to_le_bytes().to_vec();
        o.extend(body);
        o.push(0);
        o
    }

    fn b_sub(k: &str, doc: Vec<u8>) -> Vec<u8> {
        let mut o = vec![0x03];
        o.extend(k.as_bytes());
        o.push(0);
        o.extend(doc);
        o
    }

    const SAMPLE_GUID: [u8; 16] = [0xa8, 0x76, 0xed, 0x8d, 0x29, 0x77, 0xdf, 0x4f, 0x93, 0x20, 0xf2, 0x68, 0x19, 0x2e, 0x91, 0xcf];

    #[test]
    fn playnite_guids_read_like_csharp_writes_them() {
        assert_eq!(guid_string(&SAMPLE_GUID), "8ded76a8-7729-4fdf-9320-f268192e91cf");
    }

    #[test]
    fn playnite_games_are_found_in_the_database() {
        let status = b_doc(vec![b_guid("_id", [9; 16]), b_str("Name", "Not Played")]);
        let game = b_doc(vec![
            b_guid("_id", SAMPLE_GUID),
            b_str("BackgroundImage", "8ded76a8-7729-4fdf-9320-f268192e91cf\\bg.jpg"),
            b_str("Description", "A game."),
            vec![0x10, b'N', b'u', b'm', 0, 7, 0, 0, 0],
            vec![0x0A, b'I', b'c', b'o', b'n', 0],
            b_str("CoverImage", "8ded76a8-7729-4fdf-9320-f268192e91cf\\cover.png"),
            b_sub("CompletionStatus", status.clone()),
            b_str("Name", "Rescue Mission"),
        ]);
        let no_pictures = b_doc(vec![b_guid("_id", [7; 16]), b_str("Name", "Platform")]);
        let null_bg = b_doc(vec![b_guid("_id", [5; 16]), vec![0x0A, b'B', b'a', b'c', b'k', b'g', b'r', b'o', b'u', b'n', b'd', b'I', b'm', b'a', b'g', b'e', 0],
                                 b_str("Name", "Pictureless")]);
        let mut db = vec![0xAB; 37];
        for d in [&status, &game, &no_pictures, &null_bg] {
            db.extend(d.iter());
            db.extend([0u8; 13]);
        }
        db.extend(&game[..game.len() - 5]); // a document cut off by the end of the file
        let games = pn_scan(&db);
        assert_eq!(games, [PnGame { id: "8ded76a8-7729-4fdf-9320-f268192e91cf".into(), name: "Rescue Mission".into(),
                                    bg: "8ded76a8-7729-4fdf-9320-f268192e91cf\\bg.jpg".into(),
                                    cover: "8ded76a8-7729-4fdf-9320-f268192e91cf\\cover.png".into() }]);
        assert!(pn_scan(&[]).is_empty() && pn_scan(&[0x05; 5000]).is_empty());
        assert!(pn_doc(&db, 0).is_none() && pn_doc(&db, db.len() + 5).is_none());

        let list = vec![
            PnGame { id: "1".into(), name: "Mass Effect™ Legendary Edition".into(), bg: String::new(), cover: String::new() },
            PnGame { id: "2".into(), name: "Mass Effect: Legendary Edition".into(), bg: "b".into(), cover: "c".into() },
            PnGame { id: "3".into(), name: "Doom".into(), bg: "b".into(), cover: String::new() },
        ];
        assert_eq!(pn_find(&list, "mass effect legendary edition").unwrap().id, "2", "the one with pictures");
        assert_eq!(pn_find(&list, "DOOM").unwrap().id, "3");
        assert!(pn_find(&list, "Doom Eternal").is_none() && pn_find(&list, "").is_none());
    }

    #[test]
    fn a_stuck_thread_is_not_waited_for() {
        assert_eq!(with_timeout(2.0, || 7), Some(7));
        let t = Instant::now();
        assert_eq!(with_timeout(0.2, || { std::thread::sleep(Duration::from_millis(800)); 1 }), None);
        assert!(t.elapsed() < Duration::from_millis(600));
    }

    #[test]
    fn pictures_are_fetched_kept_and_failures_remembered() {
        if card::dry_run() {
            return; // a dry run fetches nothing
        }
        // curl reads local files too, so this needs no network.
        let dir = std::env::temp_dir().join(format!("sbmeta-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.png"), png(640, 360)).unwrap();
        std::fs::write(dir.join("page.html"), "<html>an error page, not a picture</html>").unwrap();
        let url = |name: &str| format!("file://{}", dir.join(name).display());
        let jobs = vec![
            (url("good.png"), dir.join("a.png")),
            (url("good.png"), dir.join("b.png")),
            (url("page.html"), dir.join("c.png")),
            (url("missing.png"), dir.join("d.png")),
        ];
        start_budget();
        let got = fetch_files(&jobs);
        assert_eq!(got, [Some(dir.join("a.png")), Some(dir.join("b.png")), None, None]);
        assert_eq!(image_size(&dir.join("a.png")), Some((640, 360)));
        assert!(!dir.join("c.png").exists() && !dir.join("c.part").exists(), "an error page is not kept");
        assert!(failed_marker(&dir.join("c.png")).exists() && failed_marker(&dir.join("d.png")).exists());
        assert_eq!(BUDGET.with(|b| b.borrow().downloads), MAX_DOWNLOADS - 4);

        // The second time nothing is fetched: kept pictures and remembered failures cost no downloads.
        start_budget();
        assert_eq!(fetch_files(&jobs), got);
        assert_eq!(BUDGET.with(|b| b.borrow().downloads), MAX_DOWNLOADS);

        // A picture that turns up later is fetched once its failure marker is gone, and the marker goes.
        std::fs::write(dir.join("missing.png"), png(10, 10)).unwrap();
        let _ = std::fs::remove_file(failed_marker(&dir.join("d.png")));
        assert_eq!(fetch_file(&url("missing.png"), &dir.join("d.png")), Some(dir.join("d.png")));
        assert!(!failed_marker(&dir.join("d.png")).exists());
        assert!(fetch_files(&[]).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn music_services_answers_are_filtered() {
        let mb: Value = serde_json::from_str(r#"{"release-groups":[
            {"id":"7822977a-984d-358d-823f-426139c0eaba","title":"The Colour and the Shape","first-release-date":"1997-05-10"},
            {"id":"440c9ce8-8ce4-3103-a745-0b385ae70a34","title":"Foo Fighters","first-release-date":"1995-06-26"},
            {"id":"91825bcf-e51d-4357-986c-ac2a49b4d205","title":"Wasting Light","first-release-date":"2011-04-08","secondary-types":[]},
            {"id":"0c437c9c-3b61-4f82-a792-e5555e6a910a","title":"Live at Wembley","first-release-date":"2008","secondary-types":["Live"]},
            {"id":"not-an-id","title":"Broken","first-release-date":"2000"},
            {"id":"91825bcf-e51d-4357-986c-ac2a49b4d206","title":"Wasting Light","first-release-date":"2012"}]}"#).unwrap();
        let t: Vec<String> = studio_albums(&mb, "The Colour and the Shape").into_iter().map(|a| a.1).collect();
        assert_eq!(t, ["Foo Fighters", "Wasting Light"], "oldest first, not the playing one, no live ones, no duplicates");
        assert_eq!(spread(&[1, 2, 3], 7), [1, 2, 3]);
        assert_eq!(spread(&(0..13).collect::<Vec<_>>(), 5), [0, 3, 6, 9, 12]);
        assert_eq!(spread::<u8>(&[], 5), Vec::<u8>::new());
        assert!(is_mbid("67f66c07-6e61-4026-ade5-7e782fad3a5d") && !is_mbid("x&y=1") && !is_mbid(""));
    }

    #[test]
    fn tmdb_records_and_searches() {
        let d: Value = serde_json::from_str(r#"{"overview":"A keeper finds a signal.","release_date":"2025-03-01",
            "poster_path":"/p.jpg","backdrop_path":"/b1.jpg",
            "credits":{"crew":[{"name":"Someone","job":"Producer"},{"name":"Ines Calloway","job":"Director"}]},
            "images":{"backdrops":[{"file_path":"/b1.jpg"},{"file_path":"/b2.jpg"}],
                      "logos":[{"file_path":"/l.svg"},{"file_path":"/l.png"}]}}"#).unwrap();
        let i = tmdb_parse(&d, false);
        assert_eq!((i.overview.as_str(), i.year.as_str(), i.maker.as_str()), ("A keeper finds a signal.", "2025", "Ines Calloway"));
        assert_eq!(i.poster.as_deref(), Some("/p.jpg"));
        assert_eq!(i.backdrops, ["/b1.jpg", "/b2.jpg"]);
        assert_eq!(i.logo.as_deref(), Some("/l.png"));
        let s: Value = serde_json::from_str(r#"{"first_air_date":"2023-09-01","created_by":[{"name":"A"},{"name":"B"},{"name":"C"}]}"#).unwrap();
        let t = tmdb_parse(&s, true);
        assert_eq!((t.year.as_str(), t.maker.as_str()), ("2023", "A, B"));
        assert_eq!(tmdb_parse(&Value::Null, false), TmdbInfo::default());

        let mut n = np("movie", "Jellyfin", "Starfall (2025).mkv");
        assert_eq!(tmdb_query(&n, &Kind::Film), ("Starfall".into(), Some("2025".into())));
        n.title = "Starfall".into();
        n.artist = "2025".into();
        assert_eq!(tmdb_query(&n, &Kind::Film), ("Starfall".into(), Some("2025".into())));
        n = np("movie", "Jellyfin", "The Lighthouse Years - S02E05 - Low Tide");
        assert_eq!(tmdb_query(&n, &Kind::Tv), ("The Lighthouse Years".into(), None));
        n = np("movie", "Jellyfin", "Low Tide");
        n.artist = "The Lighthouse Years".into();
        n.extra = "S02E05".into();
        assert_eq!(tmdb_query(&n, &Kind::Tv), ("The Lighthouse Years".into(), None), "the series is the artist");
    }

    #[test]
    fn without_a_key_tmdb_does_nothing() {
        // The test environment has no key; with one, the lookup would go online.
        if std::env::var_os("SBCOMMAND_TMDB_KEY").is_none() {
            assert!(tmdb(&np("movie", "Jellyfin", "Starfall"), &Kind::Film).is_none());
        }
    }

    #[test]
    fn the_budget_runs_out() {
        start_budget();
        assert!(secs_for(6.0).is_some_and(|s| s <= 6.0));
        assert_eq!(take_downloads(10), 10);
        assert_eq!(take_downloads(20), MAX_DOWNLOADS - 10);
        assert_eq!(take_downloads(1), 0);
        BUDGET.with(|b| b.borrow_mut().deadline = Instant::now() + Duration::from_millis(500));
        assert!(secs_for(6.0).is_none(), "under a second left: no more requests");
        start_budget();
    }

    #[test]
    fn plain_kinds_need_no_network() {
        // Sport, video and stream pages come from the player's own details.
        let mut n = np("movie", "YouTube", "Cabin build");
        n.artist = "Fell & Fern".into();
        let m = lookup(&n);
        assert_eq!((m.kind, m.title.as_str(), m.subtitle.as_str(), m.kicker_app.as_str()),
                   (Some(Kind::Video), "Cabin build", "Fell & Fern", "YouTube"));
        assert!(m.cover.is_none() && m.backgrounds.is_empty() && m.trailer.is_none());
        let m = lookup(&np("movie", "Ace Stream", ""));
        assert_eq!((m.kind, m.title.as_str()), (Some(Kind::Stream), "Ace Stream"), "the app stands in for a missing title");
    }

    #[test]
    fn the_demo_pages_use_made_up_names() {
        for id in ["studio", "live", "stadium", "film", "tv", "game", "sport", "video", "stream", "music", "movie", "nonsense"] {
            let m = demo(id);
            assert!(m.kind.is_some() && !m.title.is_empty() && !m.kicker_app.is_empty(), "{id}");
            assert!(m.trailer.is_none(), "{id}: nothing to play offline");
        }
        assert_eq!(demo("live").kind, Some(Kind::LiveSmall));
        assert_eq!(demo("stadium").kind, Some(Kind::LiveStadium));
        assert_eq!(demo("movie").kind, Some(Kind::Film));
        assert_eq!(demo("nonsense").kind, Some(Kind::Studio));
        let s = demo("stadium");
        assert!(is_stadium(&s.venue) && s.credits.iter().any(|(k, _)| k == "Recorded live"));
        assert!(!is_stadium(&demo("live").venue));
        assert!(demo("studio").bio.chars().count() <= 220);
        assert!(demo("music").facts.iter().all(|f| !f.contains("Foo")));
    }

    // Manual network checks. Run with:
    //   HOME=/some/scratch/dir cargo test --release meta::tests::network -- --ignored --nocapture
    // (a scratch HOME keeps the downloaded pictures out of the real cache).

    #[test]
    #[ignore = "needs the network"]
    fn network_steam_gives_screenshots_and_a_trailer() {
        start_budget();
        let mut m = Meta::default();
        steam_fill(&mut m, "1328670");
        println!("subtitle {:?}\ncredits {:?}\nfacts {:?}\nbio {:?}", m.subtitle, m.credits, m.facts, m.bio);
        println!("trailer {:?} ({})", m.trailer, m.trailer_name);
        println!("logo {:?}\ncover {:?}\nbackgrounds {:?}", m.logo, m.cover, m.backgrounds);
        for p in &m.pictures {
            println!("picture {} {:?}", p.display(), image_size(p));
        }
        assert!(m.pictures.len() >= 4, "official art and screenshots");
        assert!(m.pictures[1..].iter().all(|p| image_size(p).is_some_and(|s| s.0 >= 1000)));
        assert!(m.trailer.as_deref().is_some_and(|t| t.starts_with("https://") && t.contains(".m3u8")));
        assert_eq!(m.subtitle, "BioWare");
        assert!(m.facts.contains(&"2021".to_string()));
        assert!(m.logo.is_some() && m.cover.is_some());
    }

    #[test]
    #[ignore = "needs the network"]
    fn network_audiodb_gives_fanart_for_foo_fighters() {
        start_budget();
        let a = audiodb_artist("Foo Fighters").expect("TheAudioDB knows the Foo Fighters");
        let urls: Vec<String> = ["strArtistFanart", "strArtistFanart2", "strArtistFanart3", "strArtistFanart4"]
            .iter().map(|k| text(&a, k)).filter(|u| !u.is_empty()).collect();
        println!("fanart {urls:#?}");
        assert!((3..=4).contains(&urls.len()));
        assert!(audiodb_artist("Zzzz Not A Real Band Qqqq").is_none());
    }

    #[test]
    #[ignore = "needs the network"]
    fn network_a_song_gets_its_whole_page() {
        let n = NowPlaying { kind: "Song".into(), activity: "music", app: "Spotify".into(), title: "Everlong".into(),
                             artist: "Foo Fighters".into(), album: "The Colour And The Shape (Deluxe Edition)".into(),
                             extra: "Track 11".into(), length: Some(250.0), ..Default::default() };
        let t = Instant::now();
        let m = lookup(&n);
        println!("{:?} in {:?}\n{m:#?}", m.kind, t.elapsed());
        assert_eq!(m.kind, Some(Kind::Studio));
        assert!(m.backgrounds.len() >= 3 && m.logo.is_some() && m.cover.is_some());
        assert!(m.records.len() >= 3 && m.records.len() <= 5);
        assert!(m.bio.chars().count() <= 220 && m.bio.ends_with('.'));
        assert_eq!(m.credits[0], ("Track".to_string(), "11".to_string()));
        assert_eq!(m.credits[1], ("Album".to_string(), "The Colour And The Shape (1997)".to_string()));
        assert!(m.credits.contains(&("Length".to_string(), "4:10".to_string())));
        let again = Instant::now();
        let m2 = lookup(&n);
        println!("cached lookup took {:?}", again.elapsed());
        assert!(again.elapsed() < Duration::from_secs(2), "cached: {:?}", again.elapsed());
        assert_eq!(m2.backgrounds, m.backgrounds);
    }

    #[test]
    #[ignore = "needs the NAS"]
    fn network_playnite_fan_art_comes_from_the_nas() {
        let root = playnite_root();
        let t = Instant::now();
        start_budget();
        let first = pn_index(&root);
        println!("first call {:?} in {:?}", first.as_ref().map(Vec::len), t.elapsed());
        // The reading thread may still be going; wait for it like the app's own process would.
        let games = (0..40).find_map(|_| {
            pn_index(&root).or_else(|| {
                std::thread::sleep(Duration::from_millis(500));
                None
            })
        }).expect("the library list was read");
        println!("{} games with pictures after {:?}", games.len(), t.elapsed());
        assert!(games.len() > 1000);
        assert!(games.iter().all(|g| g.id.len() == 36 && !g.name.is_empty()));
        let g = pn_find(&games, "rescue mission").expect("a game from the library");
        println!("{g:?}");
        start_budget();
        let a = pn_assets(&root, g);
        println!("{a:?}");
        assert!(a.bg.as_ref().is_some_and(|p| p.exists() && image_size(p).is_some()), "{a:?}");
        let again = Instant::now();
        let a2 = pn_assets(&root, g);
        assert_eq!(a2, a);
        assert!(again.elapsed() < Duration::from_millis(100), "kept: {:?}", again.elapsed());
        let mut m = Meta::default();
        playnite_fill(&mut m, "Rescue Mission", &root);
        assert_eq!(m.backgrounds.len(), 1);
        assert_eq!(m.pictures.last(), m.backgrounds.last(), "fan art goes last");
        assert!(m.cover.is_some());
        // A game with a trailer file on the NAS gets it as its trailer.
        let with_trailer = games.iter()
            .find(|g| root.join("ExtraMetadata/Games").join(&g.id).join("VideoTrailer.mp4").is_file()).expect("a game with a trailer");
        start_budget();
        let t = pn_assets(&root, with_trailer);
        println!("{} -> {:?}", with_trailer.name, t.trailer);
        assert!(t.trailer.as_deref().is_some_and(|p| p.ends_with(&format!("{}/VideoTrailer.mp4", with_trailer.id))));
        let mut tm = Meta::default();
        playnite_fill(&mut tm, &with_trailer.name, &root);
        assert_eq!((tm.trailer.is_some(), tm.trailer_name.as_str()), (true, "Trailer"));
        let mut has = Meta { trailer: Some("https://steam/trailer.m3u8".into()), ..Default::default() };
        playnite_fill(&mut has, &with_trailer.name, &root);
        assert_eq!(has.trailer.as_deref(), Some("https://steam/trailer.m3u8"), "Steam's trailer comes first");
        // The folder is a parameter: a wrong one is quietly skipped.
        let mut none = Meta::default();
        playnite_fill(&mut none, "Rescue Mission", Path::new("/nonexistent/playnite"));
        assert!(none.backgrounds.is_empty());
    }

    #[test]
    #[ignore = "needs the network and the NAS"]
    fn network_an_installed_game_gets_art_and_fan_art() {
        let n = NowPlaying { kind: "Game".into(), activity: "game", app: "Steam".into(), title: "Rocket League".into(),
                             playing: true, ..Default::default() };
        let t = Instant::now();
        let m = lookup(&n);
        println!("{:?} in {:?}\n{m:#?}", m.kind, t.elapsed());
        assert!(m.logo.is_some() && !m.backgrounds.is_empty() && !m.pictures.is_empty());
    }
}
