//! Picks one of Creative's EQ presets for the song that's playing, from its
//! genre. No AI and no sound analysis: the genre comes from what the player
//! reports (music files, podcasts) or, if allowed, from a lookup of the
//! artist in MusicBrainz, a public music database. Each artist is looked up
//! once and remembered in ~/.cache/sbcommand/genres.json.

use crate::detect::Detection;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The preset chosen, and why (e.g. "punk rock" or "podcast").
#[derive(Clone, Debug, PartialEq)]
pub struct EqPick {
    pub preset: &'static str,
    pub why: String,
}

/// Genre words → Creative's EQ presets (rock gets Creative's Rock, which the
/// user prefers to the older "Rock"). Checked in this order, so
/// "pop punk" is rock and "indie pop" is Pop.
const RULES: [(&[&str], &str); 12] = [
    (&["podcast", "spoken", "audiobook", "comedy", "speech", "talk"], "Vocal"),
    (&["classical", "orchestra", "opera", "baroque", "symphon", "chamber"], "Classical"),
    (&["jazz", "blues", "swing", "bossa"], "Jazz"),
    (&["country", "folk", "bluegrass", "americana"], "Country"),
    (&["acoustic", "singer-songwriter", "unplugged"], "Acoustic"),
    (&["ambient", "new age", "chillout", "lo-fi", "lofi", "downtempo"], "Flat"),
    (&["rock", "punk", "metal", "grunge", "hardcore", "emo", "ska"], "Rock"),
    (&["hip hop", "hip-hop", "rap", "trap", "grime", "drill"], "Hip Hop"),
    (&["r&b", "rnb", "soul", "neo soul"], "R&B"),
    (&["electronic", "dance", "house", "techno", "trance", "edm", "drum and bass", "dubstep",
       "reggaeton", "garage"], "Dance"),
    (&["pop", "disco", "funk"], "Pop"),
    (&["indie", "alternative"], "Rock"),
];

/// The preset for one genre word, if any.
pub fn preset_for(genre: &str) -> Option<&'static str> {
    let g = genre.to_lowercase();
    RULES.iter().find(|(words, _)| words.iter().any(|w| g.contains(w))).map(|(_, p)| *p)
}

/// The first genre (most popular first) that maps to a preset.
pub fn pick_from(genres: &[String]) -> Option<EqPick> {
    genres.iter().find_map(|g| preset_for(g).map(|p| EqPick { preset: p, why: g.to_lowercase() }))
}

fn cache_path() -> PathBuf {
    crate::system::cache_dir().join("genres.json")
}

fn load_cache() -> BTreeMap<String, Vec<String>> {
    std::fs::read_to_string(cache_path()).ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_cache(c: &BTreeMap<String, Vec<String>>) {
    let p = cache_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_string_pretty(c).unwrap_or_default());
}

/// Percent-encodes text for a URL query.
pub fn url_encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// The artist's genre tags from a MusicBrainz search result, most popular
/// first. Only a confident match on the name is used.
pub fn tags_from_search(json: &Value, artist: &str) -> Vec<String> {
    let Some(a) = json["artists"].as_array().and_then(|list| list.iter().find(|a| {
        a["score"].as_u64().unwrap_or(0) >= 90
            && a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(artist))
    })) else {
        return vec![];
    };
    let mut tags: Vec<(String, i64)> = a["tags"].as_array().into_iter().flatten()
        .filter_map(|t| Some((t["name"].as_str()?.to_string(), t["count"].as_i64().unwrap_or(0))))
        .collect();
    tags.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    tags.into_iter().map(|(n, _)| n).collect()
}

/// The artist's genres: from the cache, or looked up once in MusicBrainz.
fn artist_genres(artist: &str, online: bool) -> Vec<String> {
    // "A, B" (several artists): the first one decides.
    let artist = artist.split(", ").next().unwrap_or(artist).trim();
    if artist.is_empty() {
        return vec![];
    }
    let key = artist.to_lowercase();
    let mut cache = load_cache();
    if let Some(g) = cache.get(&key) {
        return g.clone();
    }
    if !online || crate::card::dry_run() {
        return vec![];
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/?query=artist:%22{}%22&limit=5&fmt=json", url_encode(artist));
    let Some(out) = crate::system::run("curl", &["-sS", "--max-time", "6", "-A",
        "SoundBlasterCommand/0.3 (personal Linux app for a Sound Blaster ZxR)", &url]) else {
        return vec![]; // offline or busy: try again next time
    };
    let Ok(json) = serde_json::from_str::<Value>(&out) else { return vec![] };
    let tags = tags_from_search(&json, artist);
    cache.insert(key, tags.clone());
    save_cache(&cache);
    tags
}

/// The EQ preset for what's playing, if it's a song or podcast and its
/// genre can be told.
pub fn pick_eq(d: &Detection, online: bool) -> Option<EqPick> {
    if d.activity != "music" {
        return None;
    }
    let url = d.url.to_lowercase();
    if url.contains("/episode/") || url.contains("/show/") || url.contains("podcast") {
        return Some(EqPick { preset: "Vocal", why: "podcast".into() });
    }
    if let Some(p) = pick_from(&d.genres) {
        return Some(p);
    }
    pick_from(&artist_genres(&d.artist, online))
}

/// What identifies a song, so each song is only handled once.
pub fn track_key(d: &Detection) -> Option<String> {
    (d.activity == "music" && !(d.artist.is_empty() && d.title.is_empty()))
        .then(|| format!("{}|{}", d.artist, d.title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genres_map_to_creatives_presets() {
        for (g, p) in [("punk rock", "Rock"), ("pop punk", "Rock"), ("thrash metal", "Rock"),
                       ("indie pop", "Pop"), ("indie", "Rock"), ("hip hop", "Hip Hop"), ("trap", "Hip Hop"),
                       ("soul", "R&B"), ("house", "Dance"), ("jazz", "Jazz"), ("Classical", "Classical"),
                       ("folk", "Country"), ("ambient", "Flat"), ("singer-songwriter", "Acoustic"),
                       ("synthpop", "Pop"), ("podcast", "Vocal")] {
            assert_eq!(preset_for(g), Some(p), "{g}");
        }
        assert_eq!(preset_for("american"), None);
        let p = pick_from(&["american".into(), "skate punk".into(), "pop".into()]).unwrap();
        assert_eq!((p.preset, p.why.as_str()), ("Rock", "skate punk"), "first mappable, most popular first");
    }

    #[test]
    fn musicbrainz_results() {
        let j: Value = serde_json::from_str(r#"{"artists":[
            {"score":100,"name":"Metallica","tags":[{"name":"american","count":7},{"name":"thrash metal","count":67},{"name":"heavy metal","count":41}]},
            {"score":60,"name":"Metallica tribute","tags":[{"name":"pop","count":99}]}]}"#).unwrap();
        assert_eq!(tags_from_search(&j, "metallica"), vec!["thrash metal", "heavy metal", "american"]);
        assert!(tags_from_search(&j, "Someone else").is_empty(), "no confident match, no guess");
        assert_eq!(url_encode("Penguins & Polarbears"), "Penguins%20%26%20Polarbears");
    }

    #[test]
    fn podcasts_files_and_non_music() {
        let mut d = Detection { activity: "music", url: "https://open.spotify.com/episode/x".into(), ..Default::default() };
        assert_eq!(pick_eq(&d, false).unwrap().preset, "Vocal");
        d.url = String::new();
        d.genres = vec!["Jazz".into()];
        assert_eq!(pick_eq(&d, false).unwrap().preset, "Jazz");
        d.activity = "movie";
        assert!(pick_eq(&d, false).is_none(), "only songs change the EQ");
        assert!(track_key(&d).is_none());
    }
}
