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

fn act(u: &Ui, id: &str) -> gtk::ToggleButton {
    u.act_btns.iter().find(|(i, _)| *i == id).unwrap().1.clone()
}

fn chip(u: &Ui, name: &str) -> Option<gtk::ToggleButton> {
    u.style_btns.borrow().iter().find(|(n, _)| n == name).map(|(_, b)| b.clone())
}

fn pos(names: &[String], n: &str) -> u32 {
    names.iter().position(|x| x == n).unwrap() as u32
}

fn listen(u: &Ui, id: &str) -> gtk::ToggleButton {
    u.listen_btns.iter().find(|(i, _)| *i == id).unwrap().1.clone()
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
        check(act(u, "music").is_active() && chip(u, "Music").is_some_and(|b| b.is_active()), "Music not lit")?;
        check(u.np_kick.label() == "NOTHING PLAYING" && !u.capsule.is_visible(), "capsule should hide with nothing playing")
    });
    step!("Every activity, its profile chips, remembered", |u: &Ui| {
        for (a, _, _, _) in model::ACTIVITIES {
            act(u, a).set_active(true);
            check(s(u).activity == a, format!("{a} didn't select"))?;
            for n in model::styles_for(a) {
                chip(u, n).ok_or(format!("no {n} chip"))?.set_active(true);
                check(s(u).cur_profile() == *n, n.to_string())?;
            }
        }
        act(u, "game").set_active(true);
        chip(u, "FPS").unwrap().set_active(true);
        act(u, "music").set_active(true);
        act(u, "game").set_active(true);
        check(s(u).cur_profile() == "FPS", "Game forgot FPS")?;
        act(u, "music").set_active(true);
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
    step!("5.1 and 2.1", |u: &Ui| {
        listen(u, "21").set_active(true);
        check(s(u).listen == "21" && u.mode_desc.label().contains("subwoofer only"), "2.1")?;
        listen(u, "51").set_active(true);
        check(s(u).listen == "51", "5.1")
    });
    step!("Stereo plays on fronts, + rears, or all", |u: &Ui| {
        let b = |id: &str| u.stereo_btns.iter().find(|(i, _)| *i == id).unwrap().1.clone();
        b("rear").set_active(true);
        check(s(u).spread_mode() == "rear" && u.mode_desc.label().contains("rear speakers"), "+ rears")?;
        u.on_now_playing(Some(song()));
        check(u.signal_path()[1].3.contains("copied to the rear") && u.path_label.label().ends_with("2.0 + rears"), "path shows the copy")?;
        b("all").set_active(true);
        check(s(u).spread_mode() == "all" && u.signal_path()[3].3 == "All six speakers", "all")?;
        listen(u, "21").set_active(true);
        check(s(u).spread_mode() == "front" && !b("all").parent().unwrap().parent().unwrap().is_sensitive(), "2.1 never spreads")?;
        listen(u, "51").set_active(true);
        b("front").set_active(true);
        check(s(u).spread_mode() == "front", "fronts")?;
        u.np_gone.set(Some(Instant::now() - Duration::from_secs(20)));
        u.on_now_playing(None);
        Ok(())
    });
    step!("Scout Mode only for games", |u: &Ui| {
        act(u, "game").set_active(true);
        check(u.scout_box.is_visible(), "Scout should show for games")?;
        u.scout_sw.set_active(true);
        check(s(u).scout, "Scout on")?;
        u.scout_sw.set_active(false);
        act(u, "music").set_active(true);
        check(!u.scout_box.is_visible() && !s(u).scout, "Scout should hide for music")
    });
    step!("Output only changes after asking", |u: &Ui| {
        u.out_switch.emit_clicked();
        let d = u.win.visible_dialog();
        check(d.is_some() && !s(u).headphones(), "should ask first")?;
        d.unwrap().force_close();
        u.set_output("headphones");
        check(u.out_label.label() == "HEADPHONES" && u.test_rows.borrow().len() == 2, "headphones")?;
        check(!listen(u, "51").is_sensitive(), "5.1/2.1 make no sense on headphones")?;
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
        check(s(u).eq_of("Rock")[0] == -1, "Rock not reverted to Creative's")?;
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
        check(u.current_page() == "mixer" && u.split.shows_sidebar(), "Mixer didn't open")?;
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
        check(!u.split.shows_sidebar() && u.tool_btns.iter().all(|(_, b)| !b.is_active()), "panel should close")
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
        check(s(u).activity == "movie" && act(u, "movie").is_active(), "reload")?;
        u.set_activity("music");
        Ok(())
    });
    step!("Every page opens", |u: &Ui| {
        for (name, id) in [("mixer", "mixer"), ("crystalvoice", "crystalvoice"), ("mic", "crystalvoice"), ("speakers", "setup"),
                           ("setup", "setup"), ("settings", "settings"), ("sbx", "home"), ("eq", "home"), ("home", "home")] {
            u.show_page(name);
            check(u.current_page() == id, format!("{name} didn't open"))?;
        }
        Ok(())
    });
    step!("The window keeps its width: every profile, Pure, stereo option and a long title", |u: &Ui| {
        // The right-hand column takes the width it asks for (the room gets the
        // rest), so any card in it whose width follows its text moves the whole
        // layout. Its top card holds the changing sentence and the volume.
        let width = |u: &Ui| {
            let mut col = u.mode_desc.parent();
            while let Some(w) = col.clone() {
                if w.width_request() == 500 {
                    break;
                }
                col = w.parent();
            }
            let nat = |w: Option<gtk::Widget>| w.map_or(-1, |w| w.measure(gtk::Orientation::Horizontal, -1).1);
            let right = nat(col);
            let card = nat(u.mode_desc.parent().and_then(|p| p.parent()));
            let win = u.win.measure(gtk::Orientation::Horizontal, -1).1;
            if std::env::var_os("SBCOMMAND_WIDTHS").is_some() {
                println!("WIDTH window {win} right column {right} top card {card} | {} {}", s(u).cur_profile(), s(u).stereo_on);
            }
            (win * 10_000 + right) * 10_000 + card
        };
        u.on_now_playing(Some(song()));
        let mut seen: Vec<(String, i32)> = vec![("start".into(), width(u))];
        for (a, _, _, _) in model::ACTIVITIES {
            act(u, a).set_active(true);
            for n in model::styles_for(a) {
                chip(u, n).ok_or(format!("no {n} chip"))?.set_active(true);
                seen.push((format!("{a}/{n}"), width(u)));
            }
        }
        act(u, "music").set_active(true);
        u.pure_btn.set_active(true);
        seen.push(("Pure".into(), width(u)));
        u.sbx_btn.set_active(true);
        for id in ["front", "rear", "all"] {
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
