//! `sbcommand selftest`: drives every control in the real window, the way a
//! person would, and checks that the settings follow. The card and audio
//! system are not touched (dry-run), and settings go to a temporary folder.

use crate::model::{self, Settings, SPEAKER_CONFIGS, SPEAKER_TYPES};
use crate::media::{NowPlaying, StreamFmt};
use crate::ui::Ui;
use adw::prelude::*;
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Step = (&'static str, Box<dyn Fn(&Ui) -> Result<(), String>>);

fn check(ok: bool, what: impl Into<String>) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.into()) }
}

fn s(u: &Ui) -> std::cell::Ref<'_, Settings> {
    u.s.borrow()
}

/// The window's Ui itself (for what needs the shared handle).
fn rc() -> std::rc::Rc<Ui> {
    crate::ui::UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade())).expect("the window")
}

/// Picks an activity, as the tray or the page would (the header's tabs are gone).
fn pick(u: &Ui, id: &str) {
    u.set_activity(id);
}

fn chip(u: &Ui, name: &str) -> Option<gtk::ToggleButton> {
    u.style_btns.borrow().iter().find(|(n, _)| n == name).map(|(_, b)| b.clone())
}

fn pos(names: &[String], n: &str) -> u32 {
    names.iter().position(|x| x == n).unwrap() as u32
}

fn song() -> NowPlaying {
    NowPlaying { kind: "Song".into(), activity: "music", app: "Spotify".into(), title: "Do I Wanna Know?".into(),
        artist: "Arctic Monkeys".into(), album: "AM".into(), length: Some(272.0), position: Some(102.0), playing: true,
        stream: Some(StreamFmt { channels: 2, rate: 44100, format: "float32le".into(), map: String::new() }), ..Default::default() }
}

fn film() -> NowPlaying {
    NowPlaying { kind: "Video".into(), activity: "movie", app: "Jellyfin".into(), title: "Dune: Part Two".into(), playing: true,
        stream: Some(StreamFmt { channels: 6, rate: 48000, format: "float32le".into(), map: String::new() }), ..Default::default() }
}

fn steps() -> Vec<Step> {
    let mut v: Vec<Step> = Vec::new();
    macro_rules! step {
        ($name:expr, $f:expr) => {
            v.push(($name, Box::new($f)));
        };
    }

    step!("Opens on Music with Creative's Music profile, nothing playing", |u: &Ui| {
        check(u.current_page() == "home", "a side panel is open")?;
        check(s(u).activity == "music" && s(u).cur_profile() == "Music", format!("{} {}", s(u).activity, s(u).cur_profile()))?;
        check(s(u).activity == "music" && chip(u, "Music").is_some_and(|b| b.is_active()), "Music not lit")?;
        check(u.np_kick.label() == "NOTHING PLAYING" && !u.capsule.is_visible(), "capsule should hide with nothing playing")
    });
    step!("Every activity, its profile chips, remembered", |u: &Ui| {
        for (a, _, _, _) in model::ACTIVITIES {
            pick(u, a);
            check(s(u).activity == a, format!("{a} didn't select"))?;
            for n in model::styles_for(a) {
                chip(u, n).ok_or(format!("no {n} chip"))?.set_active(true);
                check(s(u).cur_profile() == *n, n.to_string())?;
            }
        }
        pick(u, "game");
        chip(u, "FPS").unwrap().set_active(true);
        pick(u, "music");
        pick(u, "game");
        check(s(u).cur_profile() == "FPS", "Game forgot FPS")?;
        pick(u, "music");
        Ok(())
    });
    step!("SBX and Pure: Pure turns the SBX effects off and keeps the equaliser", |u: &Ui| {
        u.pure_btn.set_active(true);
        let eq = s(u).eq_of(&s(u).cur_eq());
        check(!s(u).sbx_enabled && model::effective_eq(&s(u)) == (eq, s(u).eq_enabled), "Pure should keep the EQ")?;
        check(!u.fx[0].col.is_sensitive(), "knobs should grey out")?;
        check(u.eq_card.is_sensitive(), "the equaliser should stay usable in Pure")?;
        check(u.mode_desc.label().starts_with("Pure 5.1"), u.mode_desc.label().to_string())?;
        u.sbx_btn.set_active(true);
        check(s(u).sbx_enabled && u.mode_desc.label().starts_with("SBX "), u.mode_desc.label().to_string())
    });
    step!("Stereo plays on the fronts or + rears", |u: &Ui| {
        let b = |id: &str| u.stereo_btns.iter().find(|(i, _)| *i == id).unwrap().1.clone();
        b("rear").set_active(true);
        check(s(u).spread_mode() == "rear" && u.mode_desc.label().contains("rear speakers"), "+ rears")?;
        u.on_now_playing(Some(song()));
        check(u.signal_path()[1].3.contains("copied to the rear") && u.path_label.label().ends_with("2.0 + rears"), "path shows the copy")?;
        b("front").set_active(true);
        check(s(u).spread_mode() == "front", "fronts")?;
        u.np_gone.set(Some(Instant::now() - Duration::from_secs(20)));
        u.on_now_playing(None);
        Ok(())
    });
    step!("Scout Mode only for games", |u: &Ui| {
        pick(u, "game");
        check(u.scout_box.is_visible(), "Scout should show for games")?;
        u.scout_sw.set_active(true);
        check(s(u).scout, "Scout on")?;
        u.scout_sw.set_active(false);
        pick(u, "music");
        check(!u.scout_box.is_visible() && !s(u).scout, "Scout should hide for music")
    });
    step!("Output only changes after asking", |u: &Ui| {
        u.out_switch.emit_clicked();
        let d = u.win.visible_dialog();
        check(d.is_some() && !s(u).headphones(), "should ask first")?;
        d.unwrap().force_close();
        u.set_output("headphones");
        check(u.out_label.label() == "HEADPHONES" && u.test_rows.borrow().len() == 2, "headphones")?;
        check(!u.stereo_row.is_sensitive(), "stereo options make no sense on headphones")?;
        u.set_output("speakers");
        check(u.test_rows.borrow().len() == 6, "speakers")
    });
    step!("SBX knobs: every profile and every effect", |u: &Ui| {
        let names: Vec<String> = model::styles_for("music").iter().map(|x| x.to_string()).collect();
        for n in &names {
            chip(u, n).unwrap().set_active(true);
            let f = s(u).fx_of(n);
            check(u.fx[0].on.is_active() == f.surround.0 && u.fx[1].level.value() as i32 == f.crystalizer.1, format!("{n}: knobs don't match"))?;
        }
        chip(u, "Music").unwrap().set_active(true);
        for (i, key) in ["surround", "crystalizer", "bass", "smart_volume", "dialog"].into_iter().enumerate() {
            let before = u.fx[i].on.is_active();
            u.fx[i].on.set_active(!before);
            u.fx[i].level.set_value(33.0);
            let p = s(u).fx_of("Music");
            let (on, lvl) = match key {
                "surround" => p.surround,
                "crystalizer" => p.crystalizer,
                "bass" => p.bass,
                "smart_volume" => (p.smart_volume.0, p.smart_volume.1),
                _ => p.dialog,
            };
            check(on == !before && lvl == 33, format!("{key}: on={on} level={lvl}"))?;
            u.fx[i].on.set_active(false);
            check(u.fx[i].value.label() == "OFF", format!("{key}: should read OFF"))?;
            u.fx[i].on.set_active(before);
        }
        u.fx[2].on.set_active(true);
        u.fx[2].xover.as_ref().unwrap().set_value(150.0);
        check(s(u).fx_of("Music").bass_xover == 150, "Bass crossover")?;
        check(model::bass_boost(&s(u))[2] > 0.0, "Bass should reach 125 Hz through the EQ on 5.1")?;
        u.fx[3].mode.as_ref().unwrap().set_selected(2);
        check(s(u).fx_of("Music").smart_volume.2 == "Night", "Smart Volume mode")?;
        check(u.profile_btns["Revert"].is_sensitive(), "Revert after editing")?;
        u.profile_btns["Revert"].emit_clicked();
        check(!s(u).fx_profiles.contains_key("Music"), "Music wasn't reverted")
    });
    step!("SBX profiles: new, rename, delete, export, import", |u: &Ui| {
        check(u.create_copy(true, "My Music"), "create failed")?;
        check(s(u).cur_profile() == "My Music" && chip(u, "My Music").is_some_and(|b| b.is_active()), "own profile should get a chip")?;
        check(u.rename_current(true, "Mine"), "rename failed")?;
        check(s(u).style_of("music") == "Mine" && chip(u, "Mine").is_some(), "rename")?;
        let path = std::env::temp_dir().join(format!("sbc-selftest-fx-{}.json", std::process::id()));
        u.export_to(true, &path)?;
        u.delete_item(true);
        check(!s(u).has_profile("Mine") && s(u).cur_profile() == "Music", "delete should fall back to Music")?;
        let n = u.import_from(true, &path)?;
        let _ = std::fs::remove_file(&path);
        check(n == "Mine" && s(u).cur_profile() == "Mine", "import")?;
        u.delete_item(true);
        Ok(())
    });
    step!("Equaliser: Creative's presets", |u: &Ui| {
        u.fx[2].on.set_active(false); // no Bass-through-EQ, to read presets plainly
        let names = s(u).eq_list();
        for (n, curve) in model::eq_builtin() {
            u.eq_preset.set_selected(pos(&names, n));
            check(s(u).cur_eq() == n && s(u).eq_enabled, format!("{n} didn't select"))?;
            for (b, sc) in u.eq_bands.iter().enumerate() {
                check(sc.value() as i32 == curve[b], format!("{n}: band {b}"))?;
            }
            check(model::effective_eq(&s(u)).0 == curve.to_vec(), format!("{n}: card curve"))?;
        }
        u.eq_on.set_active(false);
        check(!model::effective_eq(&s(u)).1, "EQ off")?;
        u.eq_on.set_active(true);
        u.profile_btns["Revert"].emit_clicked();
        Ok(())
    });
    step!("Equaliser: drag a band, revert, own presets", |u: &Ui| {
        let names = s(u).eq_list();
        u.eq_preset.set_selected(pos(&names, "Rock"));
        u.eq_bands[0].set_value(5.0);
        check(s(u).eq_of("Rock")[0] == 5 && u.eq_btns["Revert"].is_sensitive(), "band edit")?;
        u.eq_btns["Revert"].emit_clicked();
        check(s(u).eq_of("Rock")[0] == 0, "Rock not reverted to Creative's")?;
        check(u.create_copy(false, "Late"), "create failed")?;
        check(s(u).cur_eq() == "Late" && u.eq_names.borrow().contains(&"Late".to_string()), "preset list")?;
        let path = std::env::temp_dir().join(format!("sbc-selftest-eq-{}.json", std::process::id()));
        u.export_to(false, &path)?;
        u.delete_item(false);
        check(s(u).cur_eq() == "Flat", "delete should fall back to Flat")?;
        let n = u.import_from(false, &path)?;
        let _ = std::fs::remove_file(&path);
        check(n == "Late" && s(u).eq_of("Late") == s(u).eq_of("Rock"), "import")?;
        u.delete_item(false);
        Ok(())
    });
    step!("Mixer: levels, Speaker boost, recording", |u: &Ui| {
        u.show_page("mixer");
        check(u.current_page() == "mixer" && u.panel_open(), "Mixer didn't open")?;
        check(u.mix_rows.borrow().len() == 6, "5.1 should show 6 faders")?;
        let row = u.mix_rows.borrow()[0].clone();
        row.2.set_value(-1.5);
        check(s(u).trim["FL"] == -1.5, "level not saved")?;
        row.2.set_value(0.0);
        u.boost.set_value(6.0);
        check(s(u).boost_db == 6.0 && model::channel_db(&s(u), "FL") == 6.0, "Speaker boost")?;
        u.boost.set_value(0.0);
        u.mic_level.set_value(-3.0);
        u.wuh_level.set_value(-6.0);
        check(s(u).rec.mic_level == Some(-3) && s(u).rec.wuh_level == Some(-6), "recording levels")
    });
    step!("CrystalVoice", |u: &Ui| {
        u.show_page("crystalvoice");
        check(u.current_page() == "crystalvoice", "didn't open")?;
        u.cv_on.set_active(true);
        u.nr.set_active(false);
        u.msvm.set_active(true);
        u.msvm_level.set_value(50.0);
        u.focus.set_active(true);
        u.focus_angle.set_value(90.0);
        u.morph.set_selected(3);
        u.mic_boost.set_selected(2);
        u.wuh.set_active(false);
        let r = s(u).rec.clone();
        check(r.voice_clarity && !r.noise_reduction && r.smart_volume && r.svm_level == 50 && r.focus
            && r.focus_angle == 90 && r.morph == model::VOICE_MORPHS[3] && r.mic_boost == "20 dB" && !r.what_u_hear,
            format!("{r:?}"))?;
        u.cv_on.set_active(false);
        u.voice_test.emit_clicked();
        check(!u.voice_test.is_sensitive(), "Test should wait while recording")
    });
    step!("Speaker setup", |u: &Ui| {
        u.show_page("speakers");
        check(u.current_page() == "setup", "didn't open")?;
        for (i, c) in SPEAKER_CONFIGS.iter().enumerate() {
            u.cfg.set_selected(i as u32);
            check(s(u).speaker_config == *c && u.test_rows.borrow().len() == model::channels(c).len(), c.to_string())?;
        }
        u.cfg.set_selected(4);
        u.spk_type.set_selected(SPEAKER_TYPES.iter().position(|t| *t == "dtt2200").unwrap() as u32);
        check(s(u).crossover == model::DTT2200_CROSSOVER && s(u).bass_redirect, "DTT2200")?;
        u.xover.set_value(100.0);
        check(s(u).crossover == 100 && s(u).speaker_type == "custom", "crossover by hand makes it custom")?;
        u.spk_type.set_selected(SPEAKER_TYPES.iter().position(|t| *t == "dtt2200").unwrap() as u32);
        u.hp_gain.set_selected(1);
        check(s(u).hp_600ohm, "high gain")?;
        u.hp_gain.set_selected(0);
        u.test_rows.borrow()[0].2.emit_clicked();
        u.test_all.emit_clicked();
        Ok(())
    });
    step!("Settings", |u: &Ui| {
        u.show_page("settings");
        u.auto_combo.set_selected(1);
        check(s(u).auto_mode == "suggest", "suggest")?;
        u.auto_combo.set_selected(0);
        u.remember_switch.set_active(false);
        check(!s(u).remember_apps, "per-app memory off")?;
        u.remember_switch.set_active(true);
        u.genre_switch.set_active(false);
        check(!s(u).genre_online, "genre lookup off")?;
        u.genre_switch.set_active(true);
        u.tray_switch.set_active(false);
        check(!s(u).tray, "tray off")?;
        u.tray_switch.set_active(true);
        u.refresh_btn.emit_clicked();
        u.reset_btn.emit_clicked();
        let d = u.win.visible_dialog();
        check(d.is_some(), "reset should ask first")?;
        d.unwrap().force_close();
        u.defaults_btn.emit_clicked();
        let d = u.win.visible_dialog();
        check(d.is_some(), "restore should ask first")?;
        d.unwrap().force_close();
        u.show_page("home");
        check(!u.panel_open(), "panel should close")
    });
    step!("Now playing and the signal path", |u: &Ui| {
        u.on_now_playing(Some(song()));
        check(u.capsule.is_visible() && u.np_title.label() == "Do I Wanna Know?" && u.np_kick.label().starts_with("SONG"), "song shown")?;
        check(u.path_label.label().starts_with("44.1k → 48k · SBX Music · 2.0"), u.path_label.label().to_string())?;
        let p = u.signal_path();
        check(p[1].0 == "changed" && p[1].3.contains("Resampled 44.1 kHz → 48 kHz") && p[2].0 == "enhanced", format!("{p:?}"))?;
        check(p[3].3.contains("centre and rears silent"), "stereo on the fronts")?;
        u.pure_btn.set_active(true);
        let p = u.signal_path();
        check(p[2].0 == "same" && p[2].3.starts_with("Pure"), "Pure in the path")?;
        u.sbx_btn.set_active(true);
        u.on_now_playing(Some(film()));
        let p = u.signal_path();
        check(p[1].0 == "same" && p[1].3.contains("own speaker") && p[3].3 == "All six speakers", format!("{p:?}"))?;
        check(u.path_label.label().ends_with("5.1"), u.path_label.label().to_string())?;
        u.on_now_playing(None);
        check(u.capsule.is_visible() && u.np.borrow().as_ref().is_some_and(|n| !n.playing), "a short gap keeps the capsule, paused")?;
        u.np_gone.set(Some(Instant::now() - Duration::from_secs(20)));
        u.on_now_playing(None);
        check(!u.capsule.is_visible() && u.np_kick.label() == "NOTHING PLAYING", "nothing playing")
    });
    step!("What's playing: switch, undo, suggest", |u: &Ui| {
        use crate::detect::Detection;
        let yt = || Some(Detection { activity: "movie", source: "YouTube".into(), detail: "Some video".into(), ..Default::default() });
        u.set_activity("music");
        u.auto_combo.set_selected(0);
        u.on_detected(yt(), false);
        check(s(u).activity == "music", "first report must not switch")?;
        u.on_detected(yt(), true);
        check(s(u).activity == "movie", "automatic didn't switch")?;
        u.undo_switch();
        check(s(u).activity == "music", "Undo")?;
        u.auto_combo.set_selected(1);
        u.on_detected(yt(), true);
        check(s(u).activity == "music", "suggest must not switch")?;
        gtk::prelude::WidgetExt::activate_action(&u.win, "win.use-activity", Some(&"movie".to_variant())).map_err(|e| e.to_string())?;
        check(s(u).activity == "movie", "Switch button")?;
        u.auto_combo.set_selected(0);
        u.set_activity("music");
        u.on_detected(None, true);
        Ok(())
    });
    step!("Per-game memory", |u: &Ui| {
        use crate::detect::Detection;
        let game = || Some(Detection { activity: "game", source: "Cyberpunk 2077".into(), ..Default::default() });
        u.on_detected(game(), false); // it's playing: changes are remembered for it
        u.set_activity("game");
        u.set_style("FPS");
        u.apply_eq("Vocal");
        check(s(u).app_memory.get("Cyberpunk 2077").is_some_and(|m| m.profile == "FPS" && m.eq == "Vocal"), "not remembered")?;
        u.on_detected(None, true);
        u.set_activity("music");
        u.apply_eq("Rock");
        u.on_detected(game(), true);
        check(s(u).activity == "game" && s(u).cur_profile() == "FPS" && s(u).cur_eq() == "Vocal", "not put back")?;
        u.undo_switch();
        check(s(u).activity == "music" && s(u).cur_eq() == "Rock", "Undo")?;
        u.on_detected(None, true);
        Ok(())
    });
    step!("Song genres set the Music EQ", |u: &Ui| {
        use crate::genre::EqPick;
        let rock = || Some(EqPick { preset: "Rock", why: "punk rock".into() });
        u.apply_eq("Flat");
        u.on_genre(rock(), "a|1".into(), false);
        check(s(u).cur_eq() == "Flat", "first song after starting must not change the EQ")?;
        u.on_genre(rock(), "a|2".into(), true);
        check(s(u).cur_eq() == "Rock", "auto EQ")?;
        u.undo_eq();
        check(s(u).cur_eq() == "Flat", "Undo")?;
        u.on_genre(rock(), "a|2".into(), true);
        check(s(u).cur_eq() == "Flat", "after Undo the song keeps your choice")
    });
    step!("Tray changes reach the window", |u: &Ui| {
        let mut x = s(u).clone();
        x.set_activity("movie");
        x.save().map_err(|e| e.to_string())?;
        u.reload();
        check(s(u).activity == "movie", "reload")?;
        u.set_activity("music");
        Ok(())
    });
    step!("Every page opens", |u: &Ui| {
        for (name, id) in [("mixer", "mixer"), ("crystalvoice", "crystalvoice"), ("mic", "crystalvoice"), ("speakers", "setup"),
                           ("setup", "setup"), ("settings", "settings"), ("sbx", "sound"), ("eq", "sound"), ("sound", "sound"),
                           ("look", "look"), ("home", "home")] {
            u.show_page(name);
            check(u.current_page() == id, format!("{name} didn't open"))?;
        }
        Ok(())
    });
    step!("The page: what's playing, the corner, the readout", |u: &Ui| {
        u.on_now_playing(Some(song()));
        check(u.stage.field_labels() == ["Where", "Room", "Tone", "Extras"], format!("music labels {:?}", u.stage.field_labels()))?;
        check(u.stage.readout.label().starts_with("STEREO → "), format!("readout {}", u.stage.readout.label()))?;
        let mut film = song();
        film.activity = "movie";
        film.kind = "Video".into();
        film.app = "Jellyfin".into();
        film.title = "Starfall".into();
        film.stream = Some(StreamFmt { channels: 6, rate: 48000, format: "float32le".into(), map: String::new() });
        u.on_now_playing(Some(film));
        check(u.stage.field_labels() == ["Seat", "Venue", "Mix", "Extras"], format!("film labels {:?}", u.stage.field_labels()))?;
        // 5.1 on six speakers shows "5.1", not "5.1 → 5.1".
        check(u.stage.readout.label() == s(u).speaker_config, format!("5.1 readout {}", u.stage.readout.label()))?;
        // A film gets the ticket band ("Admit one") in place of the big number.
        check(u.stage.ticket_shown(), "the film's ticket band")?;
        u.on_now_playing(Some(song()));
        check(!u.stage.ticket_shown(), "no ticket band on a song")?;
        Ok(())
    });
    step!("The page's sound choices change the real settings", |u: &Ui| {
        u.set_activity("music");
        u.on_now_playing(Some(song()));
        u.page_pick("room:Concert");
        check(s(u).cur_profile() == "Concert" && s(u).sbx_enabled, "Room → like a live show")?;
        u.page_pick("room:off");
        check(!s(u).sbx_enabled, "Room → untouched turns SBX off")?;
        u.page_pick("room:Music");
        check(s(u).sbx_enabled && s(u).cur_profile() == "Music", "Room → close, like the studio")?;
        u.page_pick("tone:-1");
        let (curve, _) = model::effective_eq(&s(u));
        check(s(u).tone == -1 && curve[0] >= 2 && curve[9] <= -2, format!("Tone warmer {curve:?}"))?;
        u.update_stage();
        let shown = u.stage.field_values();
        check(shown.get(2).is_some_and(|v| v == "a little warmer"), format!("the page shows the pick {shown:?}"))?;
        u.page_pick("tone:0");
        u.page_pick("where:rear");
        check(s(u).stereo_on == "rear", "Where → all around you")?;
        u.page_pick("where:front");
        check(s(u).stereo_on == "front", "Where → in front of you")?;
        let before = s(u).fx_of(&s(u).cur_profile()).dialog.0;
        u.page_pick("extra:dialog");
        check(s(u).fx_of(&s(u).cur_profile()).dialog.0 != before, "Extras → clearer voices")?;
        u.page_pick("extra:dialog");
        check(s(u).fx_of(&s(u).cur_profile()).dialog.0 == before, "Extras → clearer voices off again")
    });
    step!("Look and feel: layouts, per-kind defaults, the compact view", |u: &Ui| {
        u.on_now_playing(Some(song()));
        check(s(u).layout_for("music") == "slimbig" && s(u).layout_for("sport") == "slim", "the defaults")?;
        // (The test song has no cover, so there's no big cover to see.)
        let (dock, rows, bar, _, _) = u.stage.shows();
        check(dock && !rows && !bar, format!("slim dock {:?}", u.stage.shows()))?;
        u.s.borrow_mut().layout = "rows".into();
        u.update_stage();
        check(u.stage.shows().1 && !u.stage.shows().0, "rows")?;
        u.s.borrow_mut().layout = "bar".into();
        u.update_stage();
        check(u.stage.shows().2, "sport-style bar")?;
        check(s(u).layout_for("sport") == "bar" && s(u).layout_for("film") == "bar", "one layout for every page")?;
        u.s.borrow_mut().layout = "split".into();
        check(s(u).layout_for("film") == "rows" && s(u).layout_for("sport") == "slim", "split only for music")?;
        u.s.borrow_mut().layout = "default".into();
        u.s.borrow_mut().layout_defaults.insert("music".into(), "rows".into());
        check(s(u).layout_for("music") == "rows" && s(u).layout_for("game") == "slimbig", "a saved default for music only")?;
        u.s.borrow_mut().layout_defaults.clear();
        u.s.borrow_mut().theme = "compact".into();
        u.update_stage();
        rc().on_window_size(u.win.width(), u.win.height());
        check(u.layout_now().1 && u.views.visible_child_name().as_deref() == Some("remote"), "the Compact theme shows the Remote")?;
        u.s.borrow_mut().theme = "oys".into();
        u.update_stage();
        rc().on_window_size(u.win.width(), u.win.height());
        u.show_page("look");
        check(u.look_page.first_child().is_some(), "the Look and feel panel has content")?;
        u.show_page("home");
        Ok(())
    });
    step!("The compact views: the Remote in a small window, the Strip in a thin one, the page again", |u: &Ui| {
        u.on_now_playing(Some(song()));
        let (w, h) = (u.win.width(), u.win.height());
        check(u.views.visible_child_name().as_deref() == Some("page"), "a full-size window shows the page")?;
        rc().on_window_size(400, 720);
        check(u.views.visible_child_name().as_deref() == Some("remote"), "a small window shows the Remote")?;
        check(u.stage.remote.tile_values().len() == 4, "the Remote's four tiles")?;
        let before = s(u).cur_profile();
        u.stage.remote.press_tile("room");
        check(u.stage.remote.sheet_open(), "a tile opens its options")?;
        u.stage.remote.press_option("Concert");
        check(s(u).cur_profile() == "Concert" && !u.stage.remote.sheet_open(), format!("picking one changes the sound ({})", s(u).cur_profile()))?;
        u.page_pick(&format!("room:{before}"));
        rc().on_window_size(1240, 76);
        check(u.views.visible_child_name().as_deref() == Some("strip") && u.views.measure(gtk::Orientation::Vertical, 1240).0 <= 76,
              format!("a thin window is the Strip, 76 px or less ({:?})", u.views.measure(gtk::Orientation::Vertical, 1240)))?;
        check(u.stage.strip.choice_words().len() == 4, "the Strip's four choices")?;
        check(crate::room::row(None, 300.0, 64.0, &u.mini_room_state()).len() == 6, "the Strip's six cabinets")?;
        rc().page_action("pin-on");
        check(s(u).strip_pin, "the pin is remembered")?;
        rc().page_action("pin-off");
        rc().on_window_size(w, h);
        check(u.views.visible_child_name().as_deref() == Some("page"), "back to the page")?;
        // The control centre keeps one size whatever tab is shown or what's pressed in it.
        u.show_page("sound");
        let size = |u: &Ui| u.side_box_size();
        let first = size(u);
        let mut sizes = vec![];
        for t in ["mixer", "crystalvoice", "setup", "look", "settings", "sound"] {
            u.show_page(t);
            sizes.push(size(u));
        }
        u.pure_btn.set_active(true);
        sizes.push(size(u));
        u.sbx_btn.set_active(true);
        sizes.push(size(u));
        check(sizes.iter().all(|s| *s == first), format!("the control centre keeps its size {first:?} {sizes:?}"))?;
        u.close_panel();
        check(!u.panel_open(), "Done closes it")?;
        // Over your shoulder, mini instead of the Remote, when picked.
        u.s.borrow_mut().compact_view = "mini".into();
        rc().on_window_size(440, 720);
        check(u.views.visible_child_name().as_deref() == Some("mini") && u.stage.shoulder.row_values().len() == 4, "the mini in a small window, four rows")?;
        u.stage.shoulder.press_row("tone");
        check(u.stage.shoulder.sheet_open(), "a row opens its options")?;
        u.stage.shoulder.press_option("-1");
        check(s(u).tone == -1 && !u.stage.shoulder.sheet_open(), "picking one changes the sound")?;
        u.page_pick("tone:0");
        u.s.borrow_mut().compact_view = "remote".into();
        rc().on_window_size(w, h);
        check(u.views.visible_child_name().as_deref() == Some("page"), "and back")
    });
    step!("The window keeps its size: every profile, Pure, stereo option, volume and a long title", |u: &Ui| {
        // The right-hand column takes the width it asks for (the room gets the
        // rest), so any card in it whose width follows its text moves the whole
        // layout. Its top card holds the changing sentence and the volume.
        let width = |u: &Ui| {
            let (right, card) = u.layout_widths();
            let win = u.win.measure(gtk::Orientation::Horizontal, -1).1;
            let (hmin, _, _, _) = u.win.measure(gtk::Orientation::Vertical, 1240);
            if std::env::var_os("SBCOMMAND_WIDTHS").is_some() {
                println!("WIDTH window {win} right column {right} top card {card} height {hmin} | {} {}", s(u).cur_profile(), s(u).stereo_on);
            }
            ((win * 10_000 + right) * 10_000 + card) * 10_000 + hmin
        };
        u.on_now_playing(Some(song()));
        let mut seen: Vec<(String, i32)> = vec![("start".into(), width(u))];
        u.stereo_btns.iter().find(|(i, _)| *i == "rear").unwrap().1.set_active(true);
        for (a, _, _, _) in model::ACTIVITIES {
            pick(u, a);
            for n in model::styles_for(a) {
                chip(u, n).ok_or(format!("no {n} chip"))?.set_active(true);
                seen.push((format!("{a}/{n} + rears"), width(u)));
            }
        }
        u.stereo_btns.iter().find(|(i, _)| *i == "front").unwrap().1.set_active(true);
        for (a, _, _, _) in model::ACTIVITIES {
            pick(u, a);
            for n in model::styles_for(a) {
                chip(u, n).ok_or(format!("no {n} chip"))?.set_active(true);
                seen.push((format!("{a}/{n}"), width(u)));
            }
        }
        pick(u, "music");
        u.pure_btn.set_active(true);
        seen.push(("Pure".into(), width(u)));
        u.sbx_btn.set_active(true);
        for id in ["front", "rear"] {
            u.stereo_btns.iter().find(|(i, _)| *i == id).unwrap().1.set_active(true);
            seen.push((format!("stereo {id}"), width(u)));
        }
        u.vol_adj.set_value(9.0);
        seen.push(("volume 9".into(), width(u)));
        u.vol_adj.set_value(100.0);
        seen.push(("volume 100".into(), width(u)));
        let mut long = song();
        long.title = "A Very Long Song Title That Goes On and On (Extended Live Remastered Version)".into();
        long.artist = "An Artist With an Unusually Long Name and the Orchestra".into();
        u.on_now_playing(Some(long));
        seen.push(("long title".into(), width(u)));
        u.on_now_playing(None);
        chip(u, "Music").ok_or("no Music chip")?.set_active(true);
        let w0 = seen[0].1;
        let moved: Vec<_> = seen.iter().filter(|(_, w)| *w != w0).collect();
        check(moved.is_empty(), format!("width {w0} changes: {moved:?}"))
    });
    v
}

pub fn run(ui: Rc<Ui>, app: adw::Application) {
    // Measure how long the window ever goes without responding.
    let worst = Rc::new(Cell::new(Duration::ZERO));
    let last = Rc::new(Cell::new(Instant::now()));
    {
        let (worst, last) = (worst.clone(), last.clone());
        glib::timeout_add_local(Duration::from_millis(10), move || {
            let gap = last.get().elapsed();
            if gap > worst.get() {
                worst.set(gap);
            }
            last.set(Instant::now());
            glib::ControlFlow::Continue
        });
    }
    let steps = Rc::new(steps());
    let idx = Rc::new(Cell::new(0usize));
    let failures = Rc::new(RefCell::new(Vec::<String>::new()));
    let total = steps.len();
    glib::timeout_add_local(Duration::from_millis(60), move || {
        let i = idx.get();
        if i < total {
            let (name, f) = &steps[i];
            if i == 0 {
                // Startup (GPU setup, first frame) is reported apart from freezes.
                println!("INFO  first frame after {} ms", worst.get().max(last.get().elapsed()).as_millis());
                worst.set(Duration::ZERO);
                last.set(Instant::now());
            }
            let t0 = Instant::now();
            let r = f(&ui);
            if t0.elapsed() > Duration::from_millis(150) {
                println!("SLOW  {name}: {} ms", t0.elapsed().as_millis());
            }
            match r {
                Ok(()) => println!("PASS  {name}"),
                Err(e) => {
                    println!("FAIL  {name}: {e}");
                    failures.borrow_mut().push(format!("{name}: {e}"));
                }
            }
            idx.set(i + 1);
            return glib::ControlFlow::Continue;
        }
        if i == total {
            // Wait for the debounced save, then compare the file with the state.
            idx.set(i + 1);
            return glib::ControlFlow::Continue;
        }
        if i < total + 8 {
            idx.set(i + 1);
            return glib::ControlFlow::Continue;
        }
        let saved = Settings::load();
        if saved != *ui.s.borrow() {
            println!("FAIL  settings file matches the window");
            failures.borrow_mut().push("settings file".into());
        } else {
            println!("PASS  settings file matches the window");
        }
        let w = worst.get();
        if w > Duration::from_millis(250) {
            println!("FAIL  window stayed responsive (longest freeze {} ms)", w.as_millis());
            failures.borrow_mut().push("responsiveness".into());
        } else {
            println!("PASS  window stayed responsive (longest pause {} ms)", w.as_millis());
        }
        let n = failures.borrow().len();
        println!("\n{} checks, {} failed", total + 2, n);
        let _ = std::fs::remove_dir_all(crate::model::config_dir());
        if n > 0 {
            std::process::exit(1);
        }
        app.quit();
        glib::ControlFlow::Break
    });
}
