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

/// Creative never published its SBX profile values; these are close approximations.
pub fn fx_builtin() -> Vec<(&'static str, Fx)> {
    let off = (false, 50);
    let svm = (false, 74, "Normal");
    vec![
        ("Gaming", fx((true, 67), (true, 50), (true, 50), svm, off)),
        ("Cinema", fx((true, 60), (true, 40), (true, 60), svm, (true, 50))),
        ("Concert", fx((true, 80), (true, 60), (true, 40), svm, off)),
        ("Night", fx((true, 50), off, (true, 25), (true, 60, "Night"), (true, 60))),
        ("Music", fx((false, 67), (true, 55), (true, 45), svm, off)),
        ("FPS", fx((true, 75), (true, 60), off, svm, (true, 30))),
        ("Racing", fx((true, 60), (true, 50), (true, 70), svm, off)),
        ("RPG", fx((true, 70), (true, 50), (true, 50), svm, (true, 40))),
        ("Strategy", fx((true, 50), (true, 40), off, svm, (true, 50))),
        ("Stadium", fx((true, 100), (true, 60), (true, 60), svm, off)),
    ]
}


/// Creative's EQ presets, exactly as the Linux driver has them from
/// Creative (ca0132_alt_eq_presets), rounded to the card's 1 dB band steps.
/// Bands: 31, 62, 125, 250, 500, 1k, 2k, 4k, 8k, 16k Hz.
pub fn eq_builtin() -> Vec<(&'static str, [i32; 10])> {
    vec![
        ("Flat", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        ("Acoustic", [0, 1, 2, 0, 0, 0, 0, 2, 2, 2]),     // 0, 1.1, 2, 0, 0, 0, 0, 2, 2, 2
        ("Classical", [0, 6, 6, 3, 0, 0, 0, 0, 3, 3]),    // 0, 6, 6, 3.1, …, 3.1, 3.1
        ("Country", [-1, 0, 1, 1, 1, 0, 0, 2, 3, 4]),     // -1.2, 0, 1.3, 1.3, 1.1, 0, 0, 2, 3.1, 4
        ("Dance", [-1, 2, 3, 4, -1, -1, 0, 0, 4, 4]),     // -1.2, 2, 3.1, 4.2, -1.2, -1.2, 0, 0, 4, 4
        ("Jazz", [0, 0, 1, 4, 4, 4, 0, 1, 3, 3]),         // 0, 0, 1.1, 4, 4, 4, 0, 1.1, 3.1, 3.1
        ("New Age", [0, 2, 2, 0, 0, 0, 1, 2, 2, 2]),      // 0, 2, 2, 0, 0, 0, 1.1, 2, 2, 2
        ("Pop", [-2, 0, 2, 2, 0, -1, -1, 0, 3, 6]),       // -1.6, 0, 2, 2, 0, -1.2, -1.2, 0, 3.1, 6
        ("Rock", [-1, -1, 1, 2, -1, -1, 0, 0, 4, 4]),     // -1.2, -1.2, 1.1, 2, -1.2, -1.2, 0, 0, 4, 4
        ("Vocal", [-2, -1, -1, 0, 3, 4, 3, 0, 0, 1]),     // -2, -1.2, -1.2, 0, 3.1, 4, 3.1, 0, 0, 1.1
    ]
}

pub fn eq_names() -> Vec<&'static str> {
    eq_builtin().iter().map(|(n, _)| *n).collect()
}

pub fn is_eq_builtin(name: &str) -> bool {
    eq_builtin().iter().any(|(n, _)| *n == name)
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
    /// Which speakers play: "fill", "51" or "21" (see LISTEN_MODES).
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
    /// With 5.1 speakers, where stereo plays: "front", "rear" or "all" (see STEREO_ON).
    pub stereo_on: String,
}

/// (id, button label, what it means) for where stereo plays on 5.1 speakers.
/// Measured: rears get a copy of left/right 3 dB quieter; the centre gets
/// left+right at the fronts' level. 5.1 sources are never changed.
pub const STEREO_ON: [(&str, &str, &str); 3] = [
    ("front", "Fronts", "Stereo plays on the front speakers only, exactly as mixed."),
    ("rear", "+ Rears", "Stereo also plays on the rear speakers, a little quieter."),
    ("all", "All", "Stereo also plays on the centre and rear speakers."),
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
        "game" => &["Gaming", "FPS", "Racing", "RPG", "Strategy"],
        "movie" => &["Cinema", "Night"],
        // "Pure" isn't offered: switching SBX off is the pure sound.
        _ => &["Music", "Concert", "Stadium"],
    }
}

/// (id, button label, what it means). Separate from the SBX profile.
/// (id, button label, what it means). Only about which speakers play;
/// SBX is a separate on/off choice, so any combination works.
pub const LISTEN_MODES: [(&str, &str, &str); 2] = [
    ("51", "5.1", "Every sound as mixed. Stereo plays from the front speakers."),
    ("21", "2.1", "Front left and right plus the subwoofer only, for everything."),
];

impl Settings {
    /// Where stereo plays with 5.1 speakers: "front" (as mixed), "rear"
    /// (fronts + rears) or "all" (centre too). Always "front" in 2.1 and on
    /// headphones. 5.1 sources are never changed.
    pub fn spread_mode(&self) -> &str {
        if self.headphones() || self.listen == "21" || !channels(&self.speaker_config).contains(&"RL") {
            return "front";
        }
        match self.stereo_on.as_str() {
            "rear" | "all" => &self.stereo_on,
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

    /// Creative's EQ presets, then your own.
    pub fn eq_list(&self) -> Vec<String> {
        let mut v: Vec<String> = eq_names().iter().map(|n| n.to_string()).collect();
        v.extend(self.eq_presets.keys().filter(|k| !is_eq_builtin(k)).cloned());
        v
    }

    pub fn has_eq(&self, name: &str) -> bool {
        is_eq_builtin(name) || self.eq_presets.contains_key(name)
    }

    /// A preset's bands: your edits if any, otherwise Creative's.
    pub fn eq_of(&self, name: &str) -> Vec<i32> {
        if let Some(c) = self.eq_presets.get(name) {
            return c.clone();
        }
        eq_builtin().into_iter().find(|(n, _)| *n == name).map_or(vec![0; 10], |(_, c)| c.to_vec())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 3,
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
        // Always on one of the current activity's SBX profiles.
        let a = if ACTIVITIES.iter().any(|x| x.0 == s.activity) { s.activity.clone() } else { "music".to_string() };
        if s.activity != a || !styles_for(&a).contains(&s.cur_profile().as_str()) {
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
        // Older listening modes mixed SBX on/off with the speaker choice.
        match self.listen.as_str() {
            // "Fill the room" (stereo spread over every speaker) was removed.
            "sbx" | "fill" => self.listen = "51".into(),
            "true51" => (self.listen, self.sbx_enabled) = ("51".into(), false),
            "true21" => (self.listen, self.sbx_enabled) = ("21".into(), false),
            m if LISTEN_MODES.iter().any(|x| x.0 == m) => {}
            // Older still: "stereo on every speaker" meant Fill the room.
            _ => self.listen = "51".into(),
        }
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


    pub fn fx_of(&self, name: &str) -> Fx {
        if let Some(f) = self.fx_profiles.get(name) {
            return f.clone();
        }
        fx_builtin()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, f)| f)
            .unwrap_or_else(|| fx_builtin().pop().unwrap().1)
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
    let curve: Vec<i32> = base.iter().zip(&boost).map(|(b, x)| (*b as f64 + x).round() as i32).collect();
    let on = (s.scout && s.sbx_active()) || s.eq_enabled || boost.iter().any(|x| *x != 0.0);
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
        "Gaming" => "Wider sound, crisp detail and fuller bass for all kinds of games",
        "Cinema" => "Wide surround, extra bass and clearer voices for films and TV",
        "Concert" => "A big, wide stage for live music",
        "Night" => "No sudden loud bangs, clear voices and gentle bass for late nights",
        "Music" => "Extra detail and bass for music",
        "FPS" => "Wide sound and sharp detail for shooters, without booming bass",
        "Racing" => "Engine rumble and a wide track for racing games",
        "RPG" => "Wide worlds and clear voices for role-playing games",
        "Strategy" => "Clear voices and a calm mix for strategy games",
        "Stadium" => "The biggest, widest sound",
        "Pure" => "No SBX effects; your equaliser stays on",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
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
        assert!(s.upmix && s.version == 3, "stereo on all speakers is kept");
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
        assert_eq!(effective_eq(&s).0, vec![-1, -1, 1, 2, -1, -1, 0, 0, 4, 4], "Creative's Rock");
        assert_eq!(eq_pairs(&s)[1].1, V::I(23), "driver offset");
        s.set_eq("Made up");
        assert_eq!(s.cur_eq(), "Rock", "unknown names are ignored");
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
        assert_eq!(s.spread_mode(), "all");
        s.listen = "21".into();
        assert_eq!(s.spread_mode(), "front", "2.1 is fronts and sub only");
        s.listen = "51".into();
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
        for (old_mode, new_mode, sbx) in [("sbx", "51", true), ("fill", "51", true), ("true51", "51", false), ("true21", "21", false)] {
            let mut o = Settings::default();
            o.listen = old_mode.into();
            o.sanitise();
            assert_eq!((o.listen.as_str(), o.sbx_enabled), (new_mode, sbx), "{old_mode}");
        }
        s.set_activity("movie"); // Cinema: SBX Surround, Crystalizer, X-Bass, Dialog+ on
        let on = |s: &Settings| fx_pairs(s).iter().filter(|(n, v)| n.ends_with("Switch") && *v == V::B(true)).count();
        // Any combination of speakers and SBX works.
        for m in ["51", "21"] {
            for sbx in [true, false] {
                s.listen = m.into();
                s.sbx_enabled = sbx;
                assert!(!s.spreads());
                assert_eq!(on(&s) > 0, sbx, "{m} with SBX {sbx}");
            }
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
