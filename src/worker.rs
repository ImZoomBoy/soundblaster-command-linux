//! A background thread that does all card and audio-system work, so the window
//! never waits. Bursts of changes (e.g. dragging a slider) are merged.

use crate::card;
use crate::detect;
use crate::model::{self, Settings};
use crate::system;
use std::sync::mpsc;

pub mod part {
    pub const SPEAKER: u32 = 1;
    pub const FX: u32 = 2;
    pub const EQ: u32 = 4;
    pub const REC: u32 = 8;
    pub const VOLUMES: u32 = 16;
    pub const PIPEWIRE: u32 = 32;
    /// Turn the card's processing off and on again, then resend everything.
    pub const REFRESH: u32 = 64;
    pub const CARD: u32 = SPEAKER | FX | EQ | REC | VOLUMES;
    pub const ALL: u32 = CARD | PIPEWIRE;
}

pub enum Job {
    Apply(Box<Settings>, u32),
    Test(Box<Settings>, String),
    /// Start, update or stop the extra outputs.
    Outputs(Box<Settings>),
    /// Apply the listening mode (spreading per stream, default output).
    Listen(Box<Settings>),
    Volume(u32),
    Mute(bool),
    ReadVolume,
    VoiceTest(Box<Settings>),
    ResetCard,
}

pub enum Event {
    Toast(String),
    Volume(u32, bool),
    VoiceTestDone(bool),
    CardPresent(bool),
    Busy(bool),
    /// What's playing changed. `fresh` is false for the first report after
    /// starting, which only updates the display.
    Detected(Option<detect::Detection>, bool),
    /// A new song: the EQ preset its genre suggests (if any), the song, and
    /// whether it's fresh (false for the first song after starting).
    Genre(Option<crate::genre::EqPick>, String, bool),
    /// Everything known about what's playing (sent every couple of seconds).
    NowPlaying(Option<Box<crate::media::NowPlaying>>),
}

pub fn apply(s: &Settings, parts: u32) -> Result<(), String> {
    crate::meters::set_bass_route(s);
    if !card::present() {
        return Err("card not available".into());
    }
    let mut pairs = Vec::new();
    if parts & part::SPEAKER != 0 {
        pairs.extend(model::speaker_pairs(s));
    }
    if parts & part::REFRESH != 0 {
        pairs.push(("Enable OutFX Playback Switch", model::V::B(false)));
    }
    if parts & (part::FX | part::REFRESH) != 0 {
        pairs.push(("Enable OutFX Playback Switch", model::V::B(!s.direct_mode)));
        pairs.extend(model::fx_pairs(s));
    }
    if parts & (part::EQ | part::REFRESH) != 0 {
        pairs.extend(model::eq_pairs(s));
    }
    if parts & part::REC != 0 {
        pairs.extend(model::rec_pairs(s));
    }
    if parts & (part::SPEAKER | part::FX | part::REFRESH) != 0 {
        // Last: the writes above can make the driver reset the crossover.
        pairs.extend(model::crossover_pairs(s));
    }
    let mut result = card::set(&pairs);
    if parts & part::VOLUMES != 0
        && let Err(e) = card::set_volumes(s) {
            result = Err(e);
        }
    if parts & part::PIPEWIRE != 0 {
        system::apply_pipewire(s);
    }
    result
}

/// Writes the config files, restarts what changed, then applies everything.
pub fn apply_configs(s: &Settings) {
    let wp = system::write_wireplumber_conf(s);
    let pulse = system::write_upmix_conf(s);
    system::restart(wp, pulse);
    let _ = apply(s, part::ALL);
    system::apply_outputs(s);
    system::relabel_aux_streams();
    system::spread_streams(s.spread_mode());
}

pub fn spawn(events: async_channel::Sender<Event>) -> mpsc::Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    {
        let tx = tx.clone();
        system::watch_volume(move || {
            let _ = tx.send(Job::ReadVolume);
        });
    }
    if card::dry_run() {
        // Self-test and screenshots: an example track if asked for.
        let ev = events.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = ev.send_blocking(Event::NowPlaying(crate::media::now_playing(None).map(Box::new)));
        });
    } else {
        let ev = events.clone();
        std::thread::Builder::new()
            .name("sbcommand-detect".into())
            .spawn(move || {
                let mut tracker = detect::Tracker::new();
                let mut songs = detect::SongTracker::default();
                loop {
                    let now = detect::detect();
                    if let Some((d, fresh)) = tracker.update(now.clone())
                        && ev.send_blocking(Event::Detected(d, fresh)).is_err() {
                            break;
                        }
                    if let Some(d) = &now
                        && let Some((song, fresh)) = songs.update(crate::genre::track_key(d)) {
                            let pick = crate::genre::pick_eq(d, Settings::load().genre_online);
                            if ev.send_blocking(Event::Genre(pick, song, fresh)).is_err() {
                                break;
                            }
                        }
                    let np = crate::media::now_playing(now.as_ref()).map(Box::new);
                    if ev.send_blocking(Event::NowPlaying(np)).is_err() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            })
            .expect("start detection thread");
    }
    std::thread::Builder::new()
        .name("sbcommand-worker".into())
        .spawn(move || {
            let send = |e: Event| {
                let _ = events.send_blocking(e);
            };
            while let Ok(first) = rx.recv() {
                // Take everything queued and merge the card updates into one.
                let mut jobs = vec![first];
                while let Ok(j) = rx.try_recv() {
                    jobs.push(j);
                }
                let mut merged: Option<(Box<Settings>, u32)> = None;
                let mut volume: Option<u32> = None;
                let mut others = Vec::new();
                for j in jobs {
                    match j {
                        Job::Volume(v) => volume = Some(v),
                        Job::Apply(s, p) => {
                            let p = p | merged.as_ref().map_or(0, |m| m.1);
                            merged = Some((s, p));
                        }
                        other => others.push(other),
                    }
                }
                if let Some((s, p)) = merged {
                    match apply(&s, p) {
                        Ok(()) => send(Event::CardPresent(true)),
                        Err(e) if e == "card not available" => send(Event::CardPresent(false)),
                        Err(e) => {
                            eprintln!("sbcommand: {e}");
                            send(Event::Toast("Some settings couldn't be sent to the card".into()));
                        }
                    }
                }
                if let Some(v) = volume {
                    system::set_volume(v);
                }
                let mut read_volume = false;
                for j in others {
                    if matches!(j, Job::ReadVolume) {
                        if read_volume {
                            continue;
                        }
                        read_volume = true;
                    }
                    match j {
                        Job::Outputs(s) => {
                            send(Event::Busy(true));
                            system::apply_outputs(&s);
                            send(Event::Busy(false));
                        }
                        Job::Test(s, ch) => system::play_test(&s, &ch),
                        Job::Listen(s) => system::apply_listen(&s),
                        Job::ResetCard => {
                            send(Event::Busy(true));
                            if system::reset_card() {
                                send(Event::Toast("Card reset. Your settings come back in a few seconds.".into()));
                            } else {
                                send(Event::Toast("Reset needs the one-time admin setup first".into()));
                            }
                            send(Event::Busy(false));
                        }
                        Job::Mute(m) => system::set_mute(m),
                        Job::ReadVolume => {
                            if let Some((v, m)) = system::get_volume() {
                                send(Event::Volume(v, m));
                            }
                        }
                        Job::VoiceTest(s) => {
                            let ev = events.clone();
                            system::voice_test(&s, move |ok| {
                                let _ = ev.send_blocking(Event::VoiceTestDone(ok));
                            });
                        }
                        Job::Apply(..) | Job::Volume(..) => {}
                    }
                }
            }
        })
        .expect("start worker thread");
    tx
}

/// `sbcommand watch`: re-applies the saved settings whenever the card is
/// re-created (start-up, wake, reset), and keeps PipeWire on the right profile.
pub fn watch() -> ! {
    use std::time::{Duration, Instant};
    let mut last = None;
    let mut last_pw = Instant::now();
    let mut tracker = detect::Tracker::new();
    eprintln!("sbcommand: watching the sound card");
    // New streams get the listening mode as soon as they start.
    system::watch_new_streams(|| {
        std::thread::sleep(Duration::from_millis(60));
        system::relabel_aux_streams();
        system::spread_streams(Settings::load().spread_mode());
    });
    let mut songs = detect::SongTracker::default();
    let mut tray = if Settings::load().tray { crate::tray::start() } else { None };
    loop {
        // Keep the tray in step with changes made in the window.
        let cur = Settings::load();
        match (&tray, cur.tray) {
            (Some(h), true) => {
                h.update(|t| if t.s != cur { t.s = cur.clone() });
            }
            (Some(h), false) => {
                h.shutdown().wait();
                tray = None;
            }
            (None, true) => tray = crate::tray::start(),
            (None, false) => {}
        }
        if cur.auto_mode != "off" {
            let now = detect::detect();
            if let Some((Some(d), true)) = tracker.update(now.clone()) {
                background_switch(d);
            }
            if let Some(d) = &now
                && let Some((_, true)) = songs.update(crate::genre::track_key(d)) {
                    background_eq(d);
                }
        }
        let sig = card::signature();
        if sig.is_some() && sig != last {
            std::thread::sleep(Duration::from_secs(2));
            let s = Settings::load();
            if last.is_none() {
                apply_configs(&s);
            } else {
                let _ = apply(&s, part::ALL);
            }
            if let Err(e) = card::wake_surround() {
                eprintln!("sbcommand: could not wake the surround outputs: {e}");
            }
            eprintln!("sbcommand: settings applied ({})", if last.is_none() { "start" } else { "card reset" });
            last = card::signature();
            last_pw = Instant::now();
        } else if sig.is_some() && last_pw.elapsed() > Duration::from_secs(15) {
            system::apply_pipewire(&Settings::load());
            last_pw = Instant::now();
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Is the window open? Then it handles what's playing itself.
fn window_open() -> bool {
    system::run("busctl", &["--user", "status", "com.local.sbcommand"]).is_some()
}

/// Switches activity from the background service and saves it.
fn switch_to(activity: &str) -> bool {
    let mut s = Settings::load();
    if s.activity == activity {
        return false;
    }
    s.set_activity(activity);
    if s.save().is_err() {
        return false;
    }
    let _ = apply(&s, part::FX | part::EQ | part::SPEAKER);
    true
}

fn notify(args: &[&str]) -> Option<String> {
    let mut a = vec!["-a", "Sound Blaster Command", "-i", "com.local.sbcommand"];
    a.extend_from_slice(args);
    system::run("notify-send", &a)
}

/// With the window closed: switch (and say so), or offer to switch.
fn background_switch(d: detect::Detection) {
    if window_open() {
        return;
    }
    // Per-game memory: put back what was last used with this game or app.
    let mut s = Settings::load();
    if s.auto_mode == "auto" && s.recall_app(&d.source) && s.save().is_ok() {
        let _ = apply(&s, part::FX | part::EQ | part::SPEAKER);
        let _ = notify(&["-t", "3000", "-u", "low", "-h", "boolean:transient:true",
            &format!("{}: your settings for it", d.source), &format!("SBX {} · EQ {}", s.cur_profile(), s.cur_eq())]);
        return;
    }
    if s.activity == d.activity || s.auto_mode == "off" {
        // Same activity: a game may still have its own Creative profile.
        if s.auto_mode == "auto" && s.follow_game(&d.source) && s.save().is_ok() {
            let _ = apply(&s, part::FX | part::EQ);
            let _ = notify(&["-t", "3000", "-u", "low", "-h", "boolean:transient:true",
                &format!("{} is playing", d.source), &format!("SBX {} · EQ {}", s.cur_profile(), s.cur_eq())]);
        }
        return;
    }
    let label = crate::model::activity_label(d.activity);
    let body = format!("{} is playing", d.source);
    if s.auto_mode == "auto" {
        if switch_to(d.activity) {
            let mut s = Settings::load();
            if s.follow_game(&d.source) && s.save().is_ok() {
                let _ = apply(&s, part::FX | part::EQ);
            }
            eprintln!("sbcommand: switched to {label} for {}", d.source);
            let _ = notify(&["-t", "3000", "-u", "low", "-h", "boolean:transient:true", &format!("Switched to {label}"), &body]);
        }
    } else {
        // Ask without blocking the watcher; act only if the button is pressed.
        let activity = d.activity;
        std::thread::spawn(move || {
            let action = format!("switch=Switch to {label}");
            let answer = notify(&["-t", "15000", "-A", &action, "-w", &body, &format!("Switch to the {label} sound?")]);
            if answer.is_some_and(|a| a.trim() == "switch") && switch_to(activity) {
                eprintln!("sbcommand: switched to {label} (suggested)");
            }
        });
    }
}

/// With the window closed and Automatic on: set the Music EQ for the song's
/// genre, quietly (no notification for every song).
fn background_eq(d: &detect::Detection) {
    if window_open() {
        return;
    }
    let mut s = Settings::load();
    if s.auto_mode != "auto" || s.activity != "music" {
        return;
    }
    if let Some(pick) = crate::genre::pick_eq(d, s.genre_online)
        && s.cur_eq() != pick.preset {
            s.set_eq(pick.preset);
            if s.save().is_ok() {
                let _ = apply(&s, part::EQ);
                eprintln!("sbcommand: EQ {} for {}", pick.preset, pick.why);
            }
        }
}
