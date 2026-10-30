//! Settings, built-in presets, and the mapping from settings to card controls.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const SPEAKER_CONFIGS: [&str; 5] = ["2.0", "2.1", "4.0", "4.1", "5.1"];
pub const SPEAKER_CONFIG_LABELS: [&str; 5] =
    ["Stereo", "2.1 Surround", "4.0 Surround", "4.1 Surround", "5.1 Surround"];
pub const SPEAKER_TYPES: [&str; 5] = ["desktop", "bookshelf", "tower", "dtt2200", "custom"];
pub const SPEAKER_TYPE_LABELS: [&str; 5] = ["Desktop", "Bookshelf", "Tower", "Cambridge DTT2200", "Custom"];
/// The DTT2200's small satellites can't play deep bass, so the subwoofer takes more.
pub const DTT2200_CROSSOVER: i32 = 140;
pub const SVM_MODES: [&str; 3] = ["Normal", "Loud", "Night"];
pub const MIC_BOOST: [&str; 4] = ["0 dB", "10 dB", "20 dB", "30 dB"];
pub const VOICE_MORPHS: [&str; 14] = [
    "Neutral", "Female2Male", "Male2Female", "ScrappyKid", "Elderly", "Orc", "Elf", "Dwarf",
    "AlienBrute", "Robot", "Marine", "Emo", "DeepVoice", "Munchkin",
];
pub const ALL_CHANNELS: [&str; 6] = ["FL", "FR", "RL", "RR", "FC", "LFE"];
/// The ZxR's processor takes 5.1 audio in Windows order; Linux sends ALSA order.
/// Confirmed by ear on 2026-10-25: this order puts every sound on the right speaker.
pub const ZXR_51_ORDER: [&str; 6] = ["FL", "FR", "FC", "LFE", "RL", "RR"];

pub fn channels(config: &str) -> &'static [&'static str] {
    match config {
        "2.0" => &["FL", "FR"],
        "2.1" => &["FL", "FR", "LFE"],
        "4.0" => &["FL", "FR", "RL", "RR"],
        "4.1" => &["FL", "FR", "RL", "RR", "LFE"],
        _ => &["FL", "FR", "RL", "RR", "FC", "LFE"],
    }
}

pub fn channel_name(ch: &str) -> &'static str {
    match ch {
        "FL" => "Front left",
        "FR" => "Front right",
        "RL" => "Rear left",
        "RR" => "Rear right",
        "FC" => "Centre",
        _ => "Subwoofer",
    }
}

pub fn pw_output(config: &str) -> &'static str {
    match config {
        "2.0" => "analog-stereo",
        "2.1" => "analog-surround-21",
        "4.0" => "analog-surround-40",
        "4.1" => "analog-surround-41",
        _ => "analog-surround-51",
    }
}

fn idx(list: &[&str], v: &str) -> usize {
    list.iter().position(|x| *x == v).unwrap_or(0)
}

// ------------------------------------------------------------------ presets

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fx {
    pub surround: (bool, i32),
    pub crystalizer: (bool, i32),
    pub bass: (bool, i32),
    #[serde(default = "default_xover")]
    pub bass_xover: i32,
    pub smart_volume: (bool, i32, String),
    pub dialog: (bool, i32),
}

fn default_xover() -> i32 {
    80
}

fn fx(sur: (bool, i32), cry: (bool, i32), bass: (bool, i32), svm: (bool, i32, &str), dlg: (bool, i32)) -> Fx {
    Fx {
        surround: sur,
        crystalizer: cry,
        bass,
        bass_xover: 80,
        smart_volume: (svm.0, svm.1, svm.2.to_string()),
        dialog: dlg,
    }
}

/// Creative's SBX profiles for the ZxR, exactly as Sound Blaster Command
/// 3.4.98 ships them (Product/ZxR/Profiles/*.json, the speaker settings).
/// Levels 0-100; Bass crossover 80 Hz in all of them.
pub fn fx_builtin() -> Vec<(&'static str, Fx)> {
    vec![
        ("Gaming", fx((true, 67), (true, 30), (true, 10), (true, 74, "Normal"), (false, 50))),
        ("Cinema", fx((true, 67), (true, 55), (true, 20), (true, 74, "Normal"), (false, 50))),
        ("Concert", fx((true, 67), (true, 50), (true, 40), (true, 60, "Normal"), (false, 50))),
        ("Night", fx((true, 67), (true, 55), (true, 20), (true, 74, "Night"), (true, 30))),
        // This app's own: Creative has no profile for plain studio music.
        ("Music", fx((false, 67), (true, 55), (true, 45), (false, 74, "Normal"), (false, 50))),
        ("FPS", fx((true, 12), (true, 65), (true, 20), (true, 20, "Normal"), (false, 50))),
        ("Racing", fx((true, 30), (false, 50), (true, 20), (true, 40, "Normal"), (true, 30))),
        ("RPG", fx((true, 35), (false, 50), (true, 20), (true, 30, "Normal"), (true, 15))),
        ("Strategy", fx((false, 67), (true, 30), (false, 50), (true, 50, "Normal"), (false, 50))),
        ("Stadium", fx((true, 40), (false, 50), (false, 50), (true, 74, "Normal"), (true, 30))),
        ("Adventure", fx((true, 30), (true, 20), (false, 50), (true, 30, "Normal"), (false, 50))),
        // Creative's game-specific profiles, picked when that game runs.
        ("Apex Legends", fx((true, 20), (true, 30), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Arena of Valor", fx((true, 50), (true, 30), (true, 40), (true, 30, "Normal"), (false, 50))),
        ("Battlefield 1", fx((true, 67), (true, 20), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("CoD: Infinite Warfare", fx((true, 67), (true, 20), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Counter-Strike", fx((true, 67), (true, 20), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Dota 2", fx((true, 40), (true, 20), (false, 50), (true, 30, "Normal"), (false, 50))),
        ("Fortnite", fx((true, 55), (true, 40), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("League of Legends", fx((false, 67), (false, 50), (false, 50), (false, 74, "Normal"), (true, 30))),
        ("Metal Gear Solid V", fx((true, 50), (true, 30), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Overwatch", fx((true, 67), (true, 20), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("PUBG", fx((true, 45), (true, 30), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Project CARS", fx((true, 40), (true, 50), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Rocket League", fx((true, 50), (false, 50), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("The Witcher 3", fx((true, 65), (false, 50), (false, 50), (true, 30, "Normal"), (false, 50))),
    ]
}

/// Creative's headphone settings, for the profiles where they differ.
fn fx_builtin_headphones() -> Vec<(&'static str, Fx)> {
    vec![
        ("Arena of Valor", fx((true, 25), (true, 30), (true, 40), (true, 20, "Normal"), (false, 50))),
        ("Battlefield 1", fx((true, 20), (true, 25), (true, 40), (false, 74, "Normal"), (false, 50))),
        ("CoD: Infinite Warfare", fx((true, 12), (false, 50), (true, 50), (true, 30, "Normal"), (false, 50))),
        ("Counter-Strike", fx((false, 67), (true, 30), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Dota 2", fx((false, 67), (true, 35), (false, 50), (true, 20, "Normal"), (true, 20))),
        ("Fortnite", fx((true, 25), (true, 50), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Overwatch", fx((false, 67), (true, 30), (true, 50), (false, 74, "Normal"), (false, 50))),
        ("PUBG", fx((true, 15), (true, 45), (false, 50), (false, 74, "Normal"), (false, 50))),
        ("Racing", fx((true, 30), (false, 50), (false, 50), (true, 40, "Normal"), (true, 30))),
    ]
}

/// Creative's game-specific profiles and how to recognise each game by name.
pub const GAME_PROFILES: [(&str, &[&str]); 14] = [
    ("Apex Legends", &["apex legends"]),
    ("Arena of Valor", &["arena of valor"]),
    ("Battlefield 1", &["battlefield 1", "battlefield\u{2122} 1"]),
    ("CoD: Infinite Warfare", &["infinite warfare"]),
    ("Counter-Strike", &["counter-strike"]),
    ("Dota 2", &["dota 2"]),
    ("Fortnite", &["fortnite"]),
    ("League of Legends", &["league of legends"]),
    ("Metal Gear Solid V", &["metal gear solid v"]),
    ("Overwatch", &["overwatch"]),
    ("PUBG", &["pubg", "playerunknown"]),
    ("Project CARS", &["project cars"]),
    ("Rocket League", &["rocket league"]),
    ("The Witcher 3", &["witcher 3"]),
];

/// Creative's profile for a game, from the game's name, if it has one.
pub fn game_profile_for(source: &str) -> Option<&'static str> {
    let s = source.to_lowercase();
    GAME_PROFILES.iter().find(|(_, pats)| pats.iter().any(|p| s.contains(p))).map(|(n, _)| *n)
}

pub fn is_game_profile(name: &str) -> bool {
    GAME_PROFILES.iter().any(|(n, _)| *n == name)
}


/// Creative's EQ presets, as Sound Blaster Command 3.4.98 ships them for the
/// ZxR (Product/ZxR/Presets/EQ/*.json, speaker curves; the music presets have
/// the same curve for headphones), rounded to the card's 1 dB band steps,
/// halves away from zero. Bands: 31, 62, 125, 250, 500, 1k, 2k, 4k, 8k, 16k Hz.
pub fn eq_builtin() -> Vec<(&'static str, [i32; 10])> {
    vec![
        ("Flat", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Acoustic", [0, 0, 2, 0, 0, 3, 2, 2, 2, 3]),  // 0 0 2 0 0 2.5 2 2 2 3
        ("Classical", [0, 0, 2, 2, 1, 3, 1, 0, 3, 3]),  // 0 0 1.5 2 1 3 1 0 3 3
        ("Country", [0, 0, 0, 0, 2, 3, 3, 2, 3, 3]),  // 0 0 0 0 2 2.5 2.5 2 3 3
        ("Dance", [0, 0, 1, 2, 1, 2, 2, 1, 3, 3]),  // 0 0 1 1.5 0.5 1.5 2 1 2.5 2.5
        ("Hip Hop", [0, 0, 2, 3, 2, 3, 0, -3, -1, -2]),  // 0 0 2 2.5 1.5 2.5 0 -2.5 -1 -1.5
        ("Jazz", [0, 0, 0, 3, 3, 3, 1, 1, 3, 3]),  // 0 0 0 2.5 2.5 3 1 1 3 3
        ("Pop", [0, 0, 1, 1, 0, 3, 1, 1, 3, 3]),  // 0 0 1 1 0 2.5 0.5 0.5 3 3
        ("R&B", [0, 0, 0, -2, 3, 2, 2, 0, 2, 1]),  // 0 0 0 -1.5 2.5 1.5 2 0 2 1
        ("Rock", [0, 0, 2, 1, 0, 3, 1, -1, 3, 3]),
        // The Linux driver's Rock (older Creative values), kept to compare.
        ("Rock (classic)", [-1, -1, 1, 2, -1, -1, 0, 0, 4, 4]),
        ("Vocal", [0, 0, -2, -1, 3, 3, 3, 0, 0, 1]),  // 0 0 -1.5 -0.5 3 3 3 0 0 1
        ("Gaming", [0, 0, 3, -2, 0, 3, 0, 0, 3, 2]),
        ("Cinema", [0, 0, 0, 0, 2, 3, 0, 0, 1, 1]),
        ("Concert", [0, 0, 0, 0, 0, 0, 0, -1, 3, -5]),
        ("Night", [0, 0, 0, 0, 0, 0, 0, -1, 3, -5]),
    ]
}

/// Creative's EQ for each game-specific profile (speakers). Listed only while
/// that game's profile is in use.
pub fn eq_games() -> Vec<(&'static str, [i32; 10])> {
    vec![
        ("Apex Legends", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Arena of Valor", [2, 2, 2, -2, -2, 2, 2, -3, 2, 0]),
        ("Battlefield 1", [0, -2, 0, -2, -2, 2, 2, -3, 0, 3]),
        ("CoD: Infinite Warfare", [0, 5, -2, -3, -2, 0, 3, -5, -3, -2]),
        ("Counter-Strike", [2, 3, 2, -2, 1, 2, 2, -4, -2, 2]),
        ("Dota 2", [2, 3, 2, -2, -2, 2, -3, 0, 0, 0]),
        ("Fortnite", [0, 2, -2, -2, 0, 0, 3, -2, 2, 0]),
        ("League of Legends", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Metal Gear Solid V", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Overwatch", [2, 3, 2, -2, 2, 2, 2, -4, 2, 2]),
        ("PUBG", [0, 3, -4, -4, 0, 2, 2, 0, 2, 0]),
        ("Project CARS", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Rocket League", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("The Witcher 3", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    ]
}

/// Creative's headphone curves for the game EQs.
fn eq_games_headphones() -> Vec<(&'static str, [i32; 10])> {
    vec![
        ("Apex Legends", [2, 3, -1, 0, -2, 0, 2, -3, 2, 0]),
        ("Arena of Valor", [0, 2, 2, -2, 2, 1, 2, -2, 1, 2]),
        ("Battlefield 1", [2, 3, -1, 0, 0, 3, 2, 0, 2, 2]),
        ("CoD: Infinite Warfare", [0, 3, -2, -2, -2, -2, 3, -1, 1, 2]),
        ("Counter-Strike", [-4, 3, -3, -3, -2, -2, 2, -1, 0, 3]),
        ("Dota 2", [0, 2, -2, -2, -2, 2, 0, 2, 2, 2]),
        ("Fortnite", [2, 2, -2, -2, 0, 0, 2, -2, 0, 0]),
        ("League of Legends", [-3, 0, -3, -4, -5, -1, -3, -2, 2, 0]),
        ("Metal Gear Solid V", [2, 2, -2, 1, 1, 1, 2, -1, -1, 0]),
        ("Overwatch", [2, 4, 1, -2, -1, 2, 3, 2, 2, 2]),
        ("PUBG", [3, 3, -2, -2, 0, 0, 3, -3, 0, 3]),
        ("Project CARS", [0, 3, 0, 2, 2, 0, 2, 0, -1, -1]),
        ("Rocket League", [3, 3, -2, 2, 2, 0, 2, 0, -2, 0]),
        ("The Witcher 3", [3, 4, -2, 0, 0, 2, 2, 0, -2, 0]),
    ]
}

pub fn eq_names() -> Vec<&'static str> {
    eq_builtin().iter().map(|(n, _)| *n).collect()
}

pub fn is_eq_builtin(name: &str) -> bool {
    eq_builtin().iter().chain(eq_games().iter()).any(|(n, _)| *n == name)
}

pub fn is_fx_builtin(name: &str) -> bool {
    fx_builtin().iter().any(|(n, _)| *n == name)
}


// ------------------------------------------------------------------ settings

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Slots {
    pub speakers: String,
    pub headphones: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rec {
    pub voice_clarity: bool,
    pub noise_reduction: bool,
    pub smart_volume: bool,
    pub svm_level: i32,
    pub focus: bool,
    pub focus_angle: i32,
    pub morph: String,
    pub mic_boost: String,
    pub what_u_hear: bool,
    /// Card recording levels in dB (-90..+9). None: left as they are.
    pub mic_level: Option<i32>,
    pub wuh_level: Option<i32>,
}

impl Default for Rec {
    fn default() -> Self {
        Rec {
            voice_clarity: false,
            noise_reduction: true,
            smart_volume: false,
            svm_level: 74,
            focus: false,
            focus_angle: 60,
            morph: "Neutral".into(),
            mic_boost: "0 dB".into(),
            what_u_hear: true,
            mic_level: None,
            wuh_level: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub output: String,
    pub speaker_config: String,
    pub speaker_type: String,
    pub full_range_front: bool,
    pub full_range_rear: bool,
    pub bass_redirect: bool,
    pub crossover: i32,
    pub trim: BTreeMap<String, f64>,
    pub audio_position: Option<Vec<String>>,
    pub upmix: bool,
    /// Always "51" now: stereo plays on the fronts (or + rears) and 5.1 sources
    /// on every speaker, by itself. 2.1 speakers are set in Speaker setup.
    /// Kept so older settings files still load.
    /// Separate from SBX, which is `sbx_enabled`.
    pub listen: String,
    pub safe_volume: bool,
    pub hp_600ohm: bool,
    pub direct_mode: bool,
    pub sbx_enabled: bool,
    pub lock_profile: bool,
    pub lock_eq: bool,
    pub profile: Slots,
    pub eq_enabled: bool,
    pub eq: Slots,
    pub fx_profiles: BTreeMap<String, Fx>,
    pub eq_presets: BTreeMap<String, Vec<i32>>,
    pub rec: Rec,
    /// Extra outputs (Stereo, 2.1, Enhanced) to choose per app.
    pub extra_outputs: bool,
    /// Scout Mode: makes footsteps and quiet game sounds stand out.
    pub scout: bool,
    /// Speaker distance correction.
    pub distance_on: bool,
    /// Distance from your head to each speaker, in metres.
    pub distance: BTreeMap<String, f64>,
    /// Echo cancellation and noise removal on a separate "clean" microphone.
    pub echo_cancel: bool,
    /// Hear your microphone through the speakers or headphones.
    pub mic_monitor: bool,
    /// What you're doing: "game", "movie" or "music".
    pub activity: String,
    /// Game: Scout Mode (footsteps and quiet sounds stand out).
    pub game_footsteps: bool,
    /// Show the fine-tuning pages and expert speaker options.
    pub advanced: bool,
    /// The SBX profile chosen for each activity, e.g. "game" → "FPS".
    pub styles: BTreeMap<String, String>,
    /// The EQ preset for each activity (Music can follow the song's genre).
    pub eqs: BTreeMap<String, String>,
    /// Look up a song's genre online (MusicBrainz, artist name only).
    pub genre_online: bool,
    /// When something starts playing: "auto" (switch Game / Movie & TV / Music),
    /// "suggest" (offer to switch), or "off".
    pub auto_mode: String,
    /// Extra output level on the speakers, 0..9 dB above the safe 0 dB (Windows
    /// ran the card at +9). Too much can make loud sound fuzzy.
    pub boost_db: f64,
    /// Remember the SBX profile and EQ used for each game or app.
    pub remember_apps: bool,
    /// Game or app name → what was last used with it.
    pub app_memory: BTreeMap<String, AppMemory>,
    /// Show the tray icon (from the background service).
    pub tray: bool,
    /// With 5.1 speakers, where stereo plays: "front" or "rear" (see STEREO_ON).
    pub stereo_on: String,
    /// Look and feel: the room's theme ("oys" Over your shoulder, "blueprint",
    /// "screenprint", "callouts") or "compact", the smaller view.
    pub theme: String,
    /// Look and feel: one layout for every page, or "default" so each kind of
    /// page uses its own (see LAYOUT_DEFAULTS and `layout_defaults`).
    pub layout: String,
    /// Each kind of page's own default layout, when changed from LAYOUT_DEFAULTS.
    pub layout_defaults: BTreeMap<String, String>,
    /// The Remote (the compact view): the top shows the artist photo printed in
    /// red ("photo") or the album art whole ("album").
    pub compact_top: String,
    /// The Strip stays above other windows, on every desktop (a KWin script).
    pub strip_pin: bool,
    /// What small windows show: "remote" (the tall card) or "mini" (Over your
    /// shoulder, mini: the main page made tall).
    pub compact_view: String,
    /// Tone: -2 much warmer .. 0 as mixed .. +2 much brighter, on top of the EQ.
    pub tone: i32,
    /// Look up pictures, logos and details online (TheAudioDB, Steam, TMDB).
    pub pictures_online: bool,
    /// Optional TMDB API key for film and TV pictures (free from themoviedb.org).
    pub tmdb_key: String,
    /// Playnite's library folder (game art and saved trailers), if you use it.
    pub playnite_dir: String,
}

/// The kinds of page and what each opens with (the user's choices, 2026-10-29).
pub const LAYOUT_DEFAULTS: [(&str, &str); 7] = [
    ("music", "slimbig"), ("film", "slimbig"), ("tv", "slimbig"), ("game", "slimbig"),
    ("sport", "slim"), ("video", "slim"), ("stream", "slim"),
];

/// Layouts: (id, name, what it is). Split pages are for music only; sport,
/// online video and streams take the slim dock or their own bar.
pub const LAYOUTS: [(&str, &str, &str); 7] = [
    ("slim", "Slim dock", "The sound choices on one line in the bottom dock; the cover by the title."),
    ("slimbig", "Slim dock, big cover", "The sound choices on one line in the bottom dock; the cover large at the bottom left."),
    ("rows", "Rows", "The sound choices as rows at the bottom left, with the cover beside them."),
    ("rowstitle", "Rows, cover by the title", "The sound choices as rows at the bottom left; the cover by the title."),
    ("bar", "Sport-style bar", "The broadcast bar across the bottom, as on sport pages."),
    ("split", "Split page, rows", "Music: the album cover fills the left half, with the rows over it."),
    ("splitslim", "Split page, slim dock", "Music: the album cover fills the left half; the sound choices in the dock."),
];

/// Themes for the room: (id, name).
pub const THEMES: [(&str, &str); 5] = [
    ("oys", "Over your shoulder"), ("blueprint", "Blueprint"), ("screenprint", "Screenprint"),
    ("callouts", "Callouts"), ("compact", "Compact"),
];

/// What the tone choice adds to each EQ band (dB) for one step warmer;
/// brighter is the mirror image. Two steps double it.
pub const TONE_WARMER: [i32; 10] = [2, 2, 1, 1, 0, 0, 0, -1, -1, -2];

impl Settings {
    /// The layout a kind of page uses ("music", "film", "tv", "game", "sport", "video", "stream").
    pub fn layout_for(&self, kind: &str) -> String {
        let own = self.layout_defaults.get(kind).cloned()
            .unwrap_or_else(|| LAYOUT_DEFAULTS.iter().find(|x| x.0 == kind).map_or("slim", |x| x.1).to_string());
        let pick = if self.layout.is_empty() || self.layout == "default" { own.clone() } else { self.layout.clone() };
        fit_layout(kind, &pick, &own)
    }
}

/// A layout that suits the kind of page: split pages only for music; sport,
/// video and streams only take the slim dock or their bar.
pub fn fit_layout(kind: &str, pick: &str, own: &str) -> String {
    let bar_page = matches!(kind, "sport" | "video" | "stream");
    let ok = |l: &str| if bar_page { l == "slim" || l == "bar" } else if kind == "music" { true } else { !l.starts_with("split") };
    if ok(pick) {
        return pick.into();
    }
    let near = match pick {
        "slimbig" | "splitslim" => "slim",
        "split" => if bar_page { "slim" } else { "rows" },
        "rows" | "rowstitle" if bar_page => "bar",
        _ => "",
    };
    if !near.is_empty() && ok(near) { near.into() } else if ok(own) { own.into() } else { "slim".into() }
}

/// (id, button label, what it means) for where stereo plays on 5.1 speakers.
/// Measured: rears get a copy of left/right 3 dB quieter; the centre gets
/// left+right at the fronts' level. 5.1 sources are never changed.
/// No "all": filling the centre with left + right (PipeWire's only way) sounds
/// hollow and coloured on most stereo recordings, so the centre is left to
/// real 5.1 sources.
pub const STEREO_ON: [(&str, &str, &str); 2] = [
    ("front", "Fronts", "Stereo plays on the front speakers only, exactly as mixed."),
    ("rear", "+ Rears", "Stereo also plays on the rear speakers, a little quieter."),
];

/// What was last used with one game or app.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct AppMemory {
    pub activity: String,
    pub profile: String,
    pub eq: String,
}

/// (id, name, icon, hint shown under the name)
pub const ACTIVITIES: [(&str, &str, &str, &str); 3] = [
    ("game", "Game", "input-gaming-symbolic", "Steam, emulators"),
    ("movie", "Movie & TV", "video-x-generic-symbolic", "Films, TV, YouTube"),
    ("music", "Music", "audio-x-generic-symbolic", "Spotify, albums"),
];

/// Creative's SBX profiles offered for each activity. The first is the default.
pub fn styles_for(a: &str) -> &'static [&'static str] {
    match a {
        "game" => &["Gaming", "FPS", "Racing", "RPG", "Strategy", "Adventure"],
        "movie" => &["Cinema", "Night"],
        // "Pure" isn't offered: switching SBX off is the pure sound.
        _ => &["Music", "Concert", "Stadium"],
    }
}

/// (id, button label, what it means). Separate from the SBX profile.
/// (id, button label, what it means). Only about which speakers play;
/// SBX is a separate on/off choice, so any combination works.

impl Settings {
    /// Where stereo plays with 5.1 speakers: "front" (as mixed), "rear"
    /// (fronts + rears). Always "front" in 2.1 and on
    /// headphones. 5.1 sources are never changed.
    pub fn spread_mode(&self) -> &str {
        if self.headphones() || !channels(&self.speaker_config).contains(&"RL") {
            return "front";
        }
        match self.stereo_on.as_str() {
            "rear" => &self.stereo_on,
            _ => "front",
        }
    }

    /// Stereo is copied to more speakers than the fronts.
    pub fn spreads(&self) -> bool {
        self.spread_mode() != "front"
    }

    /// SBX (the profile's effects, and Scout Mode) is on.
    pub fn sbx_active(&self) -> bool {
        self.sbx_enabled
    }
}

pub const AUTO_MODES: [&str; 3] = ["auto", "suggest", "off"];
pub const AUTO_MODE_LABELS: [&str; 3] = ["Switch automatically", "Suggest a switch", "Do nothing"];

pub fn activity_label(a: &str) -> &'static str {
    ACTIVITIES.iter().find(|x| x.0 == a).map_or("Custom", |x| x.1)
}

impl Settings {
    /// Switches to an activity and its chosen SBX profile.
    pub fn set_activity(&mut self, a: &str) {
        if !ACTIVITIES.iter().any(|x| x.0 == a) {
            return;
        }
        self.activity = a.into();
        self.direct_mode = false;
        let style = self.style_of(a);
        self.set_cur_profile(&style);
        // Each activity keeps its own EQ (the current one until it has its own).
        let eq = self.eqs.get(a).filter(|e| self.has_eq(e)).cloned().unwrap_or_else(|| self.cur_eq());
        self.eq.speakers = eq.clone();
        self.eq.headphones = eq;
        self.scout = a == "game" && self.game_footsteps;
    }

    /// The SBX profile chosen for an activity (or its default).
    pub fn style_of(&self, a: &str) -> String {
        self.styles
            .get(a)
            .filter(|s| self.has_profile(s))
            .cloned()
            .unwrap_or_else(|| styles_for(a)[0].to_string())
    }

    /// Picks an SBX profile (Creative's or your own) for the current activity.
    pub fn set_style(&mut self, style: &str) {
        if self.has_profile(style) {
            let a = self.activity.clone();
            self.styles.insert(a, style.into());
            self.set_cur_profile(style);
        }
    }

    pub fn has_profile(&self, name: &str) -> bool {
        is_fx_builtin(name) || self.fx_profiles.contains_key(name)
    }

    /// Creative's EQ presets, then your own; a game's own EQ while it's in use.
    pub fn eq_list(&self) -> Vec<String> {
        let mut v: Vec<String> = eq_names().iter().map(|n| n.to_string()).collect();
        v.extend(self.eq_presets.keys().filter(|k| !is_eq_builtin(k)).cloned());
        let cur = self.cur_eq();
        if eq_games().iter().any(|(n, _)| *n == cur) {
            v.push(cur);
        }
        v
    }

    pub fn has_eq(&self, name: &str) -> bool {
        is_eq_builtin(name) || self.eq_presets.contains_key(name)
    }

    /// A preset's bands: your edits if any, otherwise Creative's (its
    /// headphone curve on headphones, where it has one).
    pub fn eq_of(&self, name: &str) -> Vec<i32> {
        if let Some(c) = self.eq_presets.get(name) {
            return c.clone();
        }
        let hp = if self.headphones() { eq_games_headphones() } else { Vec::new() };
        hp.into_iter().chain(eq_builtin()).chain(eq_games())
            .find(|(n, _)| *n == name).map_or(vec![0; 10], |(_, c)| c.to_vec())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 4,
            output: "speakers".into(),
            speaker_config: "5.1".into(),
            speaker_type: "desktop".into(),
            full_range_front: false,
            full_range_rear: false,
            bass_redirect: true,
            crossover: 80,
            trim: ALL_CHANNELS.iter().map(|c| (c.to_string(), 0.0)).collect(),
            audio_position: Some(ZXR_51_ORDER.iter().map(|s| s.to_string()).collect()),
            upmix: true,
            listen: String::new(),
            safe_volume: true,
            hp_600ohm: false,
            direct_mode: false,
            sbx_enabled: true,
            lock_profile: true,
            lock_eq: true,
            profile: Slots { speakers: "Music".into(), headphones: "Music".into() },
            eq_enabled: true,
            eq: Slots { speakers: "Flat".into(), headphones: "Flat".into() },
            fx_profiles: BTreeMap::new(),
            eq_presets: BTreeMap::new(),
            rec: Rec::default(),
            extra_outputs: true,
            scout: false,
            distance_on: false,
            distance: ALL_CHANNELS.iter().map(|c| (c.to_string(), 1.0)).collect(),
            echo_cancel: false,
            mic_monitor: false,
            activity: "music".into(),
            game_footsteps: false,
            advanced: false,
            styles: BTreeMap::new(),
            eqs: BTreeMap::new(),
            genre_online: true,
            auto_mode: "auto".into(),
            boost_db: 0.0,
            remember_apps: true,
            app_memory: BTreeMap::new(),
            tray: true,
            stereo_on: "front".into(),
            theme: "oys".into(),
            layout: "default".into(),
            layout_defaults: BTreeMap::new(),
            compact_top: "photo".into(),
            strip_pin: false,
            compact_view: "remote".into(),
            tone: 0,
            pictures_online: true,
            tmdb_key: String::new(),
            playnite_dir: String::new(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(d) = std::env::var("SBCOMMAND_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("sbcommand")
}

pub fn state_file() -> PathBuf {
    config_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = match std::fs::read_to_string(state_file()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("sbcommand: settings file unreadable ({e}); using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        s.sanitise();
        if s.version < 3 {
            // Version 3 goes back to Creative's SBX profiles: drop the
            // "Everyday" and Boom profiles from earlier versions. Your own
            // tweaks to Creative's profiles are kept.
            s.fx_profiles.retain(|name, _| is_fx_builtin(name));
            s.version = 3;
        }
        if s.version < 4 {
            // Version 4 uses Creative's Sound Blaster Command presets. The
            // driver's Rock you used becomes "Rock (classic)"; "New Age" is
            // gone from Creative's list.
            let rename = |n: &mut String| {
                if n == "Rock" {
                    *n = "Rock (classic)".into();
                } else if n == "New Age" {
                    *n = "Flat".into();
                }
            };
            rename(&mut s.eq.speakers);
            rename(&mut s.eq.headphones);
            s.eqs.values_mut().for_each(rename);
            s.app_memory.values_mut().for_each(|m| rename(&mut m.eq));
            if let Some(c) = s.eq_presets.remove("Rock") {
                s.eq_presets.insert("Rock (classic)".into(), c);
            }
            s.version = 4;
        }
        // Always on one of the current activity's SBX profiles.
        let a = if ACTIVITIES.iter().any(|x| x.0 == s.activity) { s.activity.clone() } else { "music".to_string() };
        let cur = s.cur_profile();
        let fits = styles_for(&a).contains(&cur.as_str())
            || (a == "game" && is_game_profile(&cur))
            || (s.fx_profiles.contains_key(&cur) && !is_fx_builtin(&cur));
        if s.activity != a || !fits {
            let cur = s.cur_profile();
            if styles_for(&a).contains(&cur.as_str()) {
                s.styles.insert(a.clone(), cur);
            }
            s.set_activity(&a);
        }
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, state_file())
    }

    /// Repairs anything out of range so the card never gets an invalid value.
    pub fn sanitise(&mut self) {
        if self.output != "headphones" {
            self.output = "speakers".into();
        }
        if !SPEAKER_CONFIGS.contains(&self.speaker_config.as_str()) {
            self.speaker_config = "5.1".into();
        }
        if !SPEAKER_TYPES.contains(&self.speaker_type.as_str()) {
            self.speaker_type = "desktop".into();
        }
        self.crossover = self.crossover.clamp(50, 200);
        if self.stereo_on == "all" {
            // "All" was removed; the closest is fronts + rears.
            self.stereo_on = "rear".into();
        }
        if !STEREO_ON.iter().any(|x| x.0 == self.stereo_on) {
            self.stereo_on = "front".into();
        }
        self.boost_db = if self.boost_db.is_finite() { self.boost_db.clamp(0.0, 9.0).round() } else { 0.0 };
        for c in ALL_CHANNELS {
            let v = self.trim.entry(c.to_string()).or_insert(0.0);
            *v = v.clamp(-20.0, 0.0);
        }
        if let Some(p) = &self.audio_position {
            let mut a = p.clone();
            a.sort();
            let mut b: Vec<String> = ALL_CHANNELS.iter().map(|s| s.to_string()).collect();
            b.sort();
            if a != b {
                self.audio_position = None;
            }
        }
        for slot in [&mut self.profile.speakers, &mut self.profile.headphones] {
            if !is_fx_builtin(slot) && !self.fx_profiles.contains_key(slot.as_str()) {
                *slot = "Music".into();
            }
        }
        for f in self.fx_profiles.values_mut() {
            for l in [&mut f.surround.1, &mut f.crystalizer.1, &mut f.bass.1, &mut f.smart_volume.1, &mut f.dialog.1] {
                *l = (*l).clamp(0, 100);
            }
            f.bass_xover = f.bass_xover.clamp(10, 1000);
            if !SVM_MODES.contains(&f.smart_volume.2.as_str()) {
                f.smart_volume.2 = "Normal".into();
            }
        }
        for c in ALL_CHANNELS {
            let v = self.distance.entry(c.to_string()).or_insert(1.0);
            *v = v.clamp(0.3, 5.0);
        }
        if !ACTIVITIES.iter().any(|x| x.0 == self.activity) {
            self.activity = "music".into();
        }
        // The 5.1 / 2.1 switch was removed: 5.1 already plays each source on
        // the speakers it was made for. Older modes also carried SBX on/off.
        if matches!(self.listen.as_str(), "true51" | "true21") {
            self.sbx_enabled = false;
        }
        self.listen = "51".into();
        // The upmix file stays on; spreading is switched per stream instead,
        // so changing the listening mode never restarts the audio system.
        self.upmix = true;
        for slot in [&mut self.eq.speakers, &mut self.eq.headphones] {
            if !is_eq_builtin(slot) && !self.eq_presets.contains_key(slot.as_str()) {
                *slot = "Flat".into();
            }
        }
        for c in self.eq_presets.values_mut() {
            c.resize(10, 0);
            for v in c.iter_mut() {
                *v = (*v).clamp(-12, 12);
            }
        }
        for slot in [&mut self.profile.speakers, &mut self.profile.headphones] {
            if !is_fx_builtin(slot) && !self.fx_profiles.contains_key(slot.as_str()) {
                *slot = "Music".into();
            }
        }
        if !AUTO_MODES.contains(&self.auto_mode.as_str()) {
            self.auto_mode = "auto".into();
        }
        let r = &mut self.rec;
        r.svm_level = r.svm_level.clamp(0, 100);
        r.focus_angle = r.focus_angle.clamp(20, 180);
        if !VOICE_MORPHS.contains(&r.morph.as_str()) {
            r.morph = "Neutral".into();
        }
        if !MIC_BOOST.contains(&r.mic_boost.as_str()) {
            r.mic_boost = "0 dB".into();
        }
    }

    pub fn headphones(&self) -> bool {
        self.output == "headphones"
    }

    /// The slot in use for profiles or EQ, honouring the padlock.
    fn slot<'a>(&self, slots: &'a Slots, locked: bool) -> &'a str {
        if locked || !self.headphones() { &slots.speakers } else { &slots.headphones }
    }

    pub fn cur_profile(&self) -> String {
        self.slot(&self.profile, self.lock_profile).to_string()
    }


    pub fn set_cur_profile(&mut self, name: &str) {
        if self.lock_profile {
            self.profile.speakers = name.into();
            self.profile.headphones = name.into();
        } else if self.headphones() {
            self.profile.headphones = name.into();
        } else {
            self.profile.speakers = name.into();
        }
    }


    /// A profile's settings: your edits if any, otherwise Creative's (its
    /// headphone settings on headphones, where they differ).
    pub fn fx_of(&self, name: &str) -> Fx {
        if let Some(f) = self.fx_profiles.get(name) {
            return f.clone();
        }
        let hp = if self.headphones() { fx_builtin_headphones() } else { Vec::new() };
        hp.into_iter()
            .chain(fx_builtin())
            .find(|(n, _)| *n == name)
            .or_else(|| fx_builtin().into_iter().find(|(n, _)| *n == "Music"))
            .map(|(_, f)| f)
            .unwrap()
    }

    /// When a game starts: Creative's own profile and EQ for it if there is
    /// one, otherwise back to your usual game profile and EQ. Your per-game
    /// memory (recall_app) comes first. True if anything changed.
    pub fn follow_game(&mut self, source: &str) -> bool {
        if self.activity != "game" {
            return false;
        }
        let before = (self.cur_profile(), self.cur_eq());
        match game_profile_for(source) {
            Some(p) => {
                // Only the current profile and EQ: the usual game ones stay as
                // the defaults for every other game.
                self.set_cur_profile(p);
                self.eq.speakers = p.into();
                self.eq.headphones = p.into();
                self.eq_enabled = true;
            }
            None if is_game_profile(&self.cur_profile()) => self.set_activity("game"),
            None => {}
        }
        before != (self.cur_profile(), self.cur_eq())
    }


    pub fn has_sub(&self) -> bool {
        self.speaker_config.ends_with(".1")
    }

    pub fn pw_profile(&self) -> String {
        let out = if self.headphones() { "analog-stereo" } else { pw_output(&self.speaker_config) };
        format!("output:{out}+input:analog-stereo")
    }

    pub fn pw_sink_suffix(&self) -> &'static str {
        if self.headphones() { "analog-stereo" } else { pw_output(&self.speaker_config) }
    }
}

// ------------------------------------------------------------------ card controls

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    B(bool),
    I(i32),
    E(u32),
}

pub type Pairs = Vec<(&'static str, V)>;

pub fn speaker_pairs(s: &Settings) -> Pairs {
    vec![
        ("HP/Speaker Auto Detect Playback Switch", V::B(false)),
        ("Surround Channel Config", V::E(idx(&SPEAKER_CONFIGS, &s.speaker_config) as u32)),
        ("Output Select", V::E(u32::from(s.headphones()))),
        ("Full-Range Front Speakers", V::B(s.full_range_front)),
        ("Full-Range Rear Speakers", V::B(s.full_range_rear)),
        ("Bass Redirection", V::B(s.bass_redirect && s.has_sub())),
        ("ZxR: 600 Ohm Gain", V::B(s.hp_600ohm)),
    ]
}

/// The bass redirection crossover, written last in every batch that can reset
/// it. The stock driver reloads the X-Bass crossover into the card whenever it
/// redoes the output setup (speaker setup, Output Select, the effects master
/// switch), and it ignores a write of the value the control already holds.
/// Writing a neighbouring value first makes it send ours.
pub fn crossover_pairs(s: &Settings) -> Pairs {
    let xover = (s.crossover / 10).clamp(1, 100);
    vec![
        ("Bass Redirection Crossover", V::I(if xover > 1 { xover - 1 } else { 2 })),
        ("Bass Redirection Crossover", V::I(xover)),
    ]
}

/// Scout Mode, built for Linux: the card's Smart Volume (Loud) lifts quiet
/// sounds, Crystalizer sharpens detail, and the Footsteps EQ curve brings out
/// the 2–5 kHz range where footsteps and reloads sit. It sits on top of the
/// chosen profile and is removed cleanly when turned off.
pub fn scout_fx(mut p: Fx) -> Fx {
    p.smart_volume = (true, 85, "Loud".into());
    p.crystalizer = (true, p.crystalizer.1.max(70));
    p.bass = (p.bass.0, p.bass.1.min(30));
    p
}

pub const SCOUT_EQ: [i32; 10] = [-4, -3, -2, 0, 1, 2, 4, 5, 3, 0];

/// Delay per channel (seconds) so sound from every speaker arrives together.
pub fn channel_delays(s: &Settings) -> Vec<(&'static str, f64)> {
    let far = ALL_CHANNELS.iter().map(|c| s.distance.get(*c).copied().unwrap_or(1.0)).fold(0.0, f64::max);
    ALL_CHANNELS
        .iter()
        .map(|c| {
            let d = s.distance.get(*c).copied().unwrap_or(1.0);
            (*c, if s.distance_on { ((far - d) / 343.0).max(0.0) } else { 0.0 })
        })
        .collect()
}

pub fn fx_pairs(s: &Settings) -> Pairs {
    let p = s.fx_of(&s.cur_profile());
    let p = if s.scout && s.sbx_active() { scout_fx(p) } else { p };
    let on = s.sbx_active();
    vec![
        ("FX: Surround Playback Switch", V::B(on && p.surround.0)),
        ("FX: Surround Playback Volume", V::I(p.surround.1)),
        ("FX: Crystalizer Playback Switch", V::B(on && p.crystalizer.0)),
        ("FX: Crystalizer Playback Volume", V::I(p.crystalizer.1)),
        ("FX: X-Bass Playback Switch", V::B(on && p.bass.0)),
        ("FX: X-Bass Playback Volume", V::I(p.bass.1)),
        ("FX: X-Bass Crossover Playback Volume", V::I((p.bass_xover / 10).clamp(1, 100))),
        ("FX: Smart Volume Playback Switch", V::B(on && p.smart_volume.0)),
        ("FX: Smart Volume Playback Volume", V::I(p.smart_volume.1)),
        ("FX: Smart Volume Setting", V::E(idx(&SVM_MODES, &p.smart_volume.2) as u32)),
        ("FX: Dialog Plus Playback Switch", V::B(on && p.dialog.0)),
        ("FX: Dialog Plus Playback Volume", V::I(p.dialog.1)),
    ]
}

const EQ_BANDS: [&str; 10] = [
    "EQ Band0 Playback Volume", "EQ Band1 Playback Volume", "EQ Band2 Playback Volume",
    "EQ Band3 Playback Volume", "EQ Band4 Playback Volume", "EQ Band5 Playback Volume",
    "EQ Band6 Playback Volume", "EQ Band7 Playback Volume", "EQ Band8 Playback Volume",
    "EQ Band9 Playback Volume",
];

impl Settings {
    /// Remembers the current activity, profile and EQ for a game or app.
    pub fn remember_app(&mut self, app: &str) {
        if !self.remember_apps || app.trim().is_empty() {
            return;
        }
        let m = AppMemory { activity: self.activity.clone(), profile: self.cur_profile(), eq: self.cur_eq() };
        self.app_memory.insert(app.trim().to_string(), m);
    }

    /// Puts back what was last used with a game or app. True if anything changed.
    pub fn recall_app(&mut self, app: &str) -> bool {
        if !self.remember_apps {
            return false;
        }
        let Some(m) = self.app_memory.get(app.trim()).cloned() else { return false };
        let before = (self.activity.clone(), self.cur_profile(), self.cur_eq());
        if ACTIVITIES.iter().any(|a| a.0 == m.activity) && self.activity != m.activity {
            self.set_activity(&m.activity);
        }
        if self.has_profile(&m.profile) {
            self.set_style(&m.profile);
        }
        if self.has_eq(&m.eq) {
            self.set_eq(&m.eq);
        }
        before != (self.activity.clone(), self.cur_profile(), self.cur_eq())
    }
}

/// The EQ preset in use (one setting for speakers and headphones).
impl Settings {
    pub fn cur_eq(&self) -> String {
        if self.headphones() && !self.lock_eq { self.eq.headphones.clone() } else { self.eq.speakers.clone() }
    }

    /// Picks an EQ preset for the current activity.
    pub fn set_eq(&mut self, name: &str) {
        if self.has_eq(name) {
            self.eq.speakers = name.into();
            self.eq.headphones = name.into();
            self.eq_enabled = true;
            self.eqs.insert(self.activity.clone(), name.into());
        }
    }
}

const BAND_HZ: [f64; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// The Linux driver turns SBX Bass (X-Bass) off whenever the speakers have a
/// subwoofer (2.1, 4.1, 5.1): see ca0132_effects_set. So that the Bass slider
/// still works there, its boost is applied through the card's equaliser
/// instead: up to +8 dB below the Bass crossover, half that up to an octave
/// above. Bass redirection then sends it to the subwoofer.
pub fn bass_through_eq(s: &Settings) -> bool {
    s.sbx_active() && !s.headphones() && s.has_sub()
}

pub fn bass_boost(s: &Settings) -> Vec<f64> {
    let p = s.fx_of(&s.cur_profile());
    let p = if s.scout { scout_fx(p) } else { p };
    if !bass_through_eq(s) || !p.bass.0 {
        return vec![0.0; 10];
    }
    let gain = 8.0 * p.bass.1.clamp(0, 100) as f64 / 100.0;
    let xover = p.bass_xover.max(20) as f64;
    BAND_HZ.iter().map(|f| if *f <= xover { gain } else if *f <= xover * 2.0 { gain / 2.0 } else { 0.0 }).collect()
}

/// The EQ curve sent to the card, and whether the EQ is on. Pure (SBX off)
/// turns off the SBX effects, Bass and Scout Mode included, and keeps the
/// equaliser, as SBX and the equaliser are separate switches in Creative's app.
pub fn effective_eq(s: &Settings) -> (Vec<i32>, bool) {
    let base: Vec<i32> = if s.scout && s.sbx_active() {
        SCOUT_EQ.to_vec()
    } else if s.eq_enabled {
        s.eq_of(&s.cur_eq())
    } else {
        vec![0; 10]
    };
    let boost = if s.sbx_active() { bass_boost(s) } else { vec![0.0; 10] };
    // Tone: warmer or brighter on top of the preset (not while Scout's EQ runs).
    let tone = if s.scout && s.sbx_active() { 0 } else { s.tone.clamp(-2, 2) };
    let tilt = |i: usize| -tone * TONE_WARMER[i];
    let curve: Vec<i32> = base.iter().zip(&boost).enumerate()
        .map(|(i, (b, x))| ((*b + tilt(i)) as f64 + x).round() as i32).collect();
    let on = (s.scout && s.sbx_active()) || s.eq_enabled || tone != 0 || boost.iter().any(|x| *x != 0.0);
    (curve, on)
}

pub fn eq_pairs(s: &Settings) -> Pairs {
    let (curve, on) = effective_eq(s);
    let mut v: Pairs = vec![("FX: Equalizer Playback Switch", V::B(on))];
    for (i, name) in EQ_BANDS.iter().enumerate() {
        // The driver's band range is 0..48 for -24..+24 dB.
        v.push((name, V::I(curve.get(i).copied().unwrap_or(0).clamp(-12, 12) + 24)));
    }
    v
}

pub fn rec_pairs(s: &Settings) -> Pairs {
    let r = &s.rec;
    let mut v: Pairs = vec![
        ("Enable InFX Capture Switch", V::B(r.voice_clarity)),
        ("FX: Noise Reduction Capture Switch", V::B(r.noise_reduction)),
        ("FX: Mic SVM Capture Switch", V::B(r.smart_volume)),
        ("SVM Level Capture Volume", V::I(r.svm_level)),
        ("FX: Voice Focus Capture Switch", V::B(r.focus)),
        ("Wedge Angle Capture Volume", V::I(r.focus_angle)),
        ("VoiceFX Capture Switch", V::E(idx(&VOICE_MORPHS, &r.morph) as u32)),
        ("Mic Boost Capture Switch", V::E(idx(&MIC_BOOST, &r.mic_boost) as u32)),
        ("What U Hear Capture Switch", V::B(r.what_u_hear)),
    ];
    // Card scale: 0..99 is -90..+9 dB.
    if let Some(db) = r.mic_level {
        v.push(("Capture Volume", V::I((db + 90).clamp(0, 99))));
    }
    if let Some(db) = r.wuh_level {
        v.push(("What U Hear Capture Volume", V::I((db + 90).clamp(0, 99))));
    }
    v
}

/// Per-channel hardware level in dB: the speaker's trim (0..-20) plus Speaker
/// boost (0..+9). Without boost it never goes above 0, so the card never boosts.
pub fn channel_db(s: &Settings, ch: &str) -> f64 {
    if s.headphones() {
        0.0
    } else {
        (s.trim.get(ch).copied().unwrap_or(0.0).clamp(-20.0, 0.0) + s.boost_db.clamp(0.0, 9.0)).min(9.0)
    }
}

// ------------------------------------------------------------------ descriptions

/// What each built-in profile is for, in a few words.
pub fn profile_about(name: &str) -> &'static str {
    match name {
        "Gaming" => "Creative's: wide surround, detail and even loudness for all kinds of games",
        "Cinema" => "Creative's: wide surround, detail and even loudness for films and TV",
        "Concert" => "Creative's: a wide, lively stage for live and party music",
        "Night" => "Creative's: no sudden loud bangs, and clearer voices, for late nights",
        "Music" => "This app's own: detail and bass without surround, for studio albums (Creative has none for this)",
        "FPS" => "Creative's: sharp detail for hearing every step, with little surround",
        "Racing" => "Creative's: clear engines and voices, with a light surround",
        "RPG" => "Creative's: open worlds and clear voices for role-playing games",
        "Strategy" => "Creative's: a clear, even mix for real-time strategy",
        "Stadium" => "Creative's: a big open sound with clear voices, no added bass",
        "Adventure" => "Creative's: Adventure & Action, a light surround for action games",
        "Pure" => "No SBX effects; your equaliser stays on",
        g if is_game_profile(g) => "Creative's profile for this game, picked when it runs",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn layouts_fit_each_kind_of_page() {
        let mut s = Settings::default();
        assert_eq!((s.layout_for("music"), s.layout_for("game"), s.layout_for("sport")), ("slimbig".into(), "slimbig".into(), "slim".into()));
        s.layout = "split".into();
        assert_eq!((s.layout_for("music"), s.layout_for("film"), s.layout_for("video")), ("split".into(), "rows".into(), "slim".into()));
        s.layout = "rows".into();
        assert_eq!(s.layout_for("sport"), "bar");
        s.layout = "default".into();
        s.layout_defaults.insert("game".into(), "bar".into());
        assert_eq!((s.layout_for("game"), s.layout_for("music")), ("bar".into(), "slimbig".into()));
    }

    #[test]
    fn tone_tilts_the_equaliser() {
        let mut s = Settings::default();
        s.eq_enabled = true;
        s.sbx_enabled = false;
        let flat = effective_eq(&s).0;
        s.tone = -1;
        let warm = effective_eq(&s).0;
        assert!(warm[0] > flat[0] && warm[9] < flat[9]);
        s.tone = 2;
        let bright = effective_eq(&s).0;
        assert!(bright[0] < flat[0] && bright[9] > flat[9] + 3);
    }

    use super::*;

    #[test]
    fn old_settings_files_still_load() {
        let old = r#"{"version":2,"output":"speakers","speaker_config":"5.1","speaker_type":"dtt2200",
        "activity":"custom","profile":{"speakers":"Boom","headphones":"Boom"},"lock_profile":true,
        "fx_profiles":{"Boom":{"surround":[true,67],"crystalizer":[true,65],"bass":[true,55],
          "smart_volume":[false,74,"Normal"],"dialog":[false,50]},
          "Everyday: Music":{"surround":[false,67],"crystalizer":[true,45],"bass":[true,45],
          "smart_volume":[false,74,"Normal"],"dialog":[false,40]},
          "Music":{"surround":[true,67],"crystalizer":[true,55],"bass":[true,45],
          "smart_volume":[false,74,"Normal"],"dialog":[false,50]}},
        "enhance":true,"tone_auto":true,"fill_room":{"music":true},"upmix":true}"#;
        let dir = std::env::temp_dir().join(format!("sbc-model-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), old).unwrap();
        // SAFETY: tests in this module don't read SBCOMMAND_CONFIG_DIR concurrently.
        unsafe { std::env::set_var("SBCOMMAND_CONFIG_DIR", &dir) };
        let s = Settings::load();
        unsafe { std::env::remove_var("SBCOMMAND_CONFIG_DIR") };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s.activity, "music");
        assert_eq!(s.cur_profile(), "Music");
        assert!(s.fx_of("Music").surround.0, "your own tweak to Creative's Music is kept");
        assert!(!s.fx_profiles.contains_key("Boom") && !s.fx_profiles.contains_key("Everyday: Music"));
        assert!(s.upmix && s.version == 4, "stereo on all speakers is kept");
    }

    #[test]
    fn version_3_rock_becomes_rock_classic() {
        let old = r#"{"version":3,"activity":"music","eq":{"speakers":"Rock","headphones":"Rock"},
        "eqs":{"music":"Rock","movie":"New Age"},"app_memory":{"Spotify":{"activity":"music","profile":"Music","eq":"Rock"}}}"#;
        let dir = std::env::temp_dir().join(format!("sbc-model-test-v3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), old).unwrap();
        // SAFETY: tests in this module don't read SBCOMMAND_CONFIG_DIR concurrently.
        unsafe { std::env::set_var("SBCOMMAND_CONFIG_DIR", &dir) };
        let s = Settings::load();
        unsafe { std::env::remove_var("SBCOMMAND_CONFIG_DIR") };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s.cur_eq(), "Rock (classic)", "the Rock you had keeps sounding the same");
        assert_eq!(s.eqs.get("music").map(String::as_str), Some("Rock (classic)"));
        assert_eq!(s.eqs.get("movie").map(String::as_str), Some("Flat"), "New Age is gone from Creative's list");
        assert_eq!(s.app_memory["Spotify"].eq, "Rock (classic)");
        assert_eq!(s.version, 4);
    }

    #[test]
    fn profiles_are_creatives_exact_values() {
        let mut s = Settings::default();
        let st = s.fx_of("Stadium");
        assert_eq!((st.surround, st.crystalizer.0, st.bass.0, (st.smart_volume.0, st.smart_volume.1), st.dialog),
                   ((true, 40), false, false, (true, 74), (true, 30)), "Creative's Stadium");
        let c = s.fx_of("Concert");
        assert_eq!((c.surround, c.crystalizer, c.bass, c.smart_volume.1), ((true, 67), (true, 50), (true, 40), 60));
        assert_eq!(s.fx_of("Night").smart_volume.2, "Night");
        assert_eq!(s.fx_of("Counter-Strike").surround, (true, 67), "speakers");
        s.output = "headphones".into();
        assert_eq!(s.fx_of("Counter-Strike").surround.0, false, "Creative's headphone settings differ");
        assert_eq!(s.fx_of("Stadium").surround, (true, 40), "same for both when Creative has one set");
        assert!(styles_for("music").contains(&"Music") && !styles_for("game").contains(&"Counter-Strike"));
    }

    #[test]
    fn a_game_with_its_own_profile_gets_it_then_others_go_back() {
        assert_eq!(game_profile_for("Counter-Strike 2"), Some("Counter-Strike"));
        assert_eq!(game_profile_for("The Witcher 3: Wild Hunt"), Some("The Witcher 3"));
        assert_eq!(game_profile_for("Portal 2"), None);
        let mut s = Settings::default();
        s.set_activity("game");
        s.set_style("FPS");
        s.set_eq("Gaming");
        assert!(s.follow_game("Counter-Strike 2"));
        assert_eq!((s.cur_profile(), s.cur_eq()), ("Counter-Strike".to_string(), "Counter-Strike".to_string()));
        assert_eq!(s.eq_of("Counter-Strike"), vec![2, 3, 2, -2, 1, 2, 2, -4, -2, 2], "Creative's game EQ");
        assert!(s.eq_list().contains(&"Counter-Strike".to_string()), "listed while in use");
        assert_eq!(s.style_of("game"), "FPS", "your usual game profile stays the default");
        assert!(s.follow_game("Portal 2"), "a game without its own profile goes back");
        assert_eq!((s.cur_profile(), s.cur_eq()), ("FPS".to_string(), "Gaming".to_string()));
        assert!(!s.eq_list().contains(&"Counter-Strike".to_string()));
        s.set_activity("music");
        assert!(!s.follow_game("Counter-Strike 2"), "only in Game");
    }

    #[test]
    fn activities_use_creatives_profiles_and_remember_the_choice() {
        let mut s = Settings::default();
        s.set_activity("game");
        assert_eq!(s.cur_profile(), "Gaming");
        s.set_style("FPS");
        s.set_activity("movie");
        assert_eq!(s.cur_profile(), "Cinema");
        s.set_style("Made up"); // not a profile: ignored
        assert_eq!(s.cur_profile(), "Cinema");
        s.set_activity("game");
        assert_eq!(s.cur_profile(), "FPS", "remembered per activity");
        for (a, _, _, _) in ACTIVITIES {
            for st in styles_for(a) {
                assert!(is_fx_builtin(st), "{st} must be one of Creative's profiles");
            }
        }
    }

    #[test]
    fn scout_only_in_games() {
        let mut s = Settings::default();
        s.game_footsteps = true;
        s.set_activity("game");
        assert!(s.scout);
        assert!(fx_pairs(&s).contains(&("FX: Smart Volume Setting", V::E(1))));
        let (c, on) = effective_eq(&s);
        assert!(on && c[7] == SCOUT_EQ[7], "Scout's tone");
        s.set_activity("music");
        assert!(!s.scout);
    }

    #[test]
    fn eq_presets_are_creatives() {
        let mut s = Settings::default();
        s.output = "headphones".into(); // no Bass-through-EQ on headphones
        assert_eq!(effective_eq(&s), (vec![0; 10], true), "Flat, on");
        s.set_eq("Rock");
        assert_eq!(effective_eq(&s).0, vec![0, 0, 2, 1, 0, 3, 1, -1, 3, 3], "Creative's Rock (Sound Blaster Command)");
        s.set_eq("Rock (classic)");
        assert_eq!(effective_eq(&s).0, vec![-1, -1, 1, 2, -1, -1, 0, 0, 4, 4], "the driver's Rock, kept to compare");
        assert_eq!(eq_pairs(&s)[1].1, V::I(23), "driver offset");
        s.set_eq("Made up");
        assert_eq!(s.cur_eq(), "Rock (classic)", "unknown names are ignored");
        s.eq_presets.insert("Mine".into(), vec![3; 10]);
        s.set_eq("Mine");
        assert_eq!(effective_eq(&s).0, vec![3; 10], "your own preset");
        s.scout = true;
        assert_eq!(effective_eq(&s).0, SCOUT_EQ.to_vec());
    }

    #[test]
    fn pure_keeps_the_equaliser_but_not_bass_and_boost_raises_the_card() {
        let mut s = Settings::default();
        s.set_eq("Rock");
        s.eq_enabled = true;
        s.sbx_enabled = false;
        let rock = s.eq_of("Rock");
        assert_eq!(effective_eq(&s), (rock, true), "Pure keeps the EQ, without the SBX Bass boost");
        s.eq_enabled = false;
        assert_eq!(effective_eq(&s), (vec![0; 10], false), "Pure with the EQ off: flat");
        s.eq_enabled = true;
        s.sbx_enabled = true;
        assert!(effective_eq(&s).1);
        assert_eq!(channel_db(&s, "FL"), 0.0);
        s.boost_db = 6.0;
        s.trim.insert("RL".into(), -3.0);
        assert_eq!((channel_db(&s, "FL"), channel_db(&s, "RL")), (6.0, 3.0));
        s.boost_db = 40.0;
        s.sanitise();
        assert_eq!(s.boost_db, 9.0, "never more than +9 dB");
        s.output = "headphones".into();
        assert_eq!(channel_db(&s, "FL"), 0.0, "no boost on headphones");
    }

    #[test]
    fn stereo_can_play_on_more_speakers() {
        let mut s = Settings::default();
        s.listen = "51".into();
        assert_eq!(s.spread_mode(), "front");
        s.stereo_on = "rear".into();
        assert!(s.spreads() && s.spread_mode() == "rear");
        s.stereo_on = "all".into();
        s.sanitise();
        assert_eq!(s.spread_mode(), "rear", "the removed \"all\" becomes + Rears");
        s.speaker_config = "2.1".into();
        assert_eq!(s.spread_mode(), "front", "2.1 speakers have no rears");
        s.speaker_config = "5.1".into();
        s.output = "headphones".into();
        assert_eq!(s.spread_mode(), "front");
        s.output = "speakers".into();
        s.stereo_on = "nonsense".into();
        s.sanitise();
        assert_eq!(s.stereo_on, "front");
    }

    #[test]
    fn games_and_apps_are_remembered() {
        let mut s = Settings::default();
        s.set_activity("game");
        s.set_style("FPS");
        s.set_eq("Vocal");
        s.remember_app("Cyberpunk 2077");
        s.set_activity("music");
        s.set_eq("Rock");
        assert!(s.recall_app("Cyberpunk 2077"));
        assert_eq!((s.activity.as_str(), s.cur_profile().as_str(), s.cur_eq().as_str()), ("game", "FPS", "Vocal"));
        assert!(!s.recall_app("Cyberpunk 2077"), "nothing to change the second time");
        assert!(!s.recall_app("Never seen"));
        s.remember_apps = false;
        s.set_activity("music");
        assert!(!s.recall_app("Cyberpunk 2077"), "off means off");
    }

    #[test]
    fn sbx_off_turns_every_effect_off() {
        let mut s = Settings::default();
        s.set_activity("music");
        s.set_style("Stadium");
        s.sbx_enabled = false;
        for (n, v) in fx_pairs(&s) {
            if n.ends_with("Switch") {
                assert_eq!(v, V::B(false), "{n}");
            }
        }
    }

    #[test]
    fn bass_redirection_needs_a_subwoofer() {
        let mut s = Settings::default();
        s.speaker_config = "4.0".into();
        let p = speaker_pairs(&s);
        assert!(p.contains(&("Bass Redirection", V::B(false))));
        assert!(p.contains(&("Surround Channel Config", V::E(2))));
    }

    #[test]
    fn crossover_is_resent_with_a_different_value_first() {
        let mut s = Settings::default();
        s.crossover = 140;
        assert_eq!(crossover_pairs(&s), vec![("Bass Redirection Crossover", V::I(13)), ("Bass Redirection Crossover", V::I(14))]);
        s.crossover = 10;
        assert_eq!(crossover_pairs(&s).last().unwrap().1, V::I(1));
        assert_ne!(crossover_pairs(&s)[0].1, V::I(1));
    }

    #[test]
    fn bad_values_are_repaired() {
        let mut s = Settings::default();
        s.speaker_config = "7.1".into();
        s.trim.insert("FL".into(), 5.0);
        s.audio_position = Some(vec!["FL".into(), "FL".into()]);
        s.profile.speakers = "Deleted".into();
        s.activity = "nonsense".into();
        s.eq.speakers = "Gone".into();
        s.sanitise();
        assert_eq!(s.speaker_config, "5.1");
        assert_eq!(s.trim["FL"], 0.0);
        assert!(s.audio_position.is_none());
        assert_eq!(s.profile.speakers, "Music");
        assert_eq!(s.activity, "music");
        assert_eq!(s.eq.speakers, "Flat");
    }

    #[test]
    fn listening_modes() {
        let mut s = Settings::default();
        s.sanitise();
        assert_eq!(s.listen, "51");
        for (old_mode, new_mode, sbx) in [("sbx", "51", true), ("fill", "51", true), ("true51", "51", false), ("true21", "51", false), ("21", "51", true)] {
            let mut o = Settings::default();
            o.listen = old_mode.into();
            o.sanitise();
            assert_eq!((o.listen.as_str(), o.sbx_enabled), (new_mode, sbx), "{old_mode}");
        }
        s.set_activity("movie"); // Cinema: SBX Surround, Crystalizer, X-Bass, Dialog+ on
        let on = |s: &Settings| fx_pairs(s).iter().filter(|(n, v)| n.ends_with("Switch") && *v == V::B(true)).count();
        // Any combination of speakers and SBX works.
        for sbx in [true, false] {
            s.sbx_enabled = sbx;
            assert!(!s.spreads());
            assert_eq!(on(&s) > 0, sbx, "SBX {sbx}");
        }
        s.sbx_enabled = false;
        s.game_footsteps = true;
        s.set_activity("game");
        assert_eq!(effective_eq(&s).0, vec![0; 10], "no Scout tone without SBX");
        assert!(!styles_for("music").contains(&"Pure"), "SBX off is the pure sound");
    }

    #[test]
    fn each_activity_keeps_its_own_eq() {
        let mut s = Settings::default();
        s.set_activity("music");
        s.set_eq("Rock");
        s.set_activity("game");
        assert_eq!(s.cur_eq(), "Rock", "a new activity starts from the current EQ");
        s.set_eq("Flat");
        s.set_activity("music");
        assert_eq!(s.cur_eq(), "Rock");
        s.set_activity("game");
        assert_eq!(s.cur_eq(), "Flat");
    }

    #[test]
    fn sbx_bass_works_on_5_1_through_the_eq() {
        let mut s = Settings::default(); // 5.1 speakers, Music profile: Bass on (45, below 80 Hz)
        s.set_activity("music");
        assert!(bass_through_eq(&s));
        let (c, on) = effective_eq(&s);
        assert!(on, "the EQ carries the Bass boost even on Flat");
        assert!(c[0] >= 3 && c[1] >= 3 && c[2] >= 1 && c[3] == 0, "boost below the crossover: {c:?}");
        s.sbx_enabled = false;
        assert_eq!(effective_eq(&s).0, vec![0; 10], "no SBX, no Bass");
        s.sbx_enabled = true;
        s.output = "headphones".into();
        assert!(!bass_through_eq(&s), "headphones use the card's own X-Bass");
        s.output = "speakers".into();
        s.speaker_config = "2.0".into();
        assert!(!bass_through_eq(&s), "without a subwoofer the card's own X-Bass works");
    }
}
