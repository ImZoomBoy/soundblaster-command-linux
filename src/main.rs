//! Sound Blaster Command for Linux.
//!
//! A control panel for the Creative Sound Blaster ZxR (CA0132), modelled on
//! Creative's Windows "Sound Blaster Command".
//!
//! Usage:
//!   sbcommand [page]   open the window (page: sbx, eq, playback, recording, settings)
//!   sbcommand apply    apply the saved settings once
//!   sbcommand watch    keep the card on its saved settings (sbcommand-watch.service)
//!   sbcommand selftest click every control and check the results (no hardware changes)

mod art;
mod card;
mod dash;
mod detect;
mod fonts;
mod genre;
mod room;
mod media;
mod meta;
mod meters;
mod mini;
mod oys;
mod tray;
mod model;
mod selftest;
mod stage;
mod system;
mod themes;
mod ui;
mod worker;

use adw::prelude::*;
use gtk::glib;
use std::rc::Rc;
use std::time::Duration;

pub(crate) const APP_ID: &str = "com.local.sbcommand";

/// Sound Blaster red, with a few touches to make the Home page feel like a
/// control panel rather than a settings list.
const CSS: &str = r#"
:root { --accent-bg-color: #c8102e; --accent-color: #ff5a6e; --accent-fg-color: #ffffff; }
@define-color accent_bg_color #c8102e;
@define-color accent_color #ff5a6e;
@define-color accent_fg_color #ffffff;
@define-color window_bg_color #111114;
@define-color headerbar_bg_color #111114;
@define-color popover_bg_color #1e1e23;
@define-color card_bg_color alpha(white, 0.04);
window.sbc { background: linear-gradient(to bottom, #17171b, #0f0f12); }
window.sbc headerbar { background: transparent; box-shadow: none; border-bottom: 1px solid alpha(white, 0.06); }
.tab { padding: 7px 16px; border-radius: 10px; font-weight: 700; color: alpha(white, 0.56); background: none; }
.tab:hover { background: alpha(white, 0.04); }
.tab:checked { background: alpha(white, 0.09); color: white; }
.tab:checked image { color: #ff416c; }
.tool:checked { background: alpha(white, 0.10); }
.room-card { border-radius: 18px; border: 1px solid alpha(white, 0.07);
  background: radial-gradient(ellipse at 50% 55%, #2c0e16 0%, #1a1014 45%, #131316 75%); }
.panel { border-radius: 18px; padding: 14px 16px; background: alpha(white, 0.035); border: 1px solid alpha(white, 0.07); }
.caps { font-size: 10px; letter-spacing: 1.7px; font-weight: 800; color: alpha(white, 0.36); }
.caps-small { font-size: 10px; letter-spacing: 1.2px; font-weight: 800; color: alpha(white, 0.5); }
.dim { color: alpha(white, 0.56); }
.small { font-size: 11px; }
.big-num { font-size: 26px; font-weight: 850; font-feature-settings: "tnum"; }
.desc { font-size: 12px; color: alpha(white, 0.6); }
.pill-btn { border-radius: 999px; padding: 6px 14px; background: alpha(white, 0.06); border: 1px solid alpha(white, 0.08); }
.seg { background: alpha(black, 0.32); border-radius: 11px; padding: 3px; border: 1px solid alpha(white, 0.07); }
.seg button { padding: 6px 16px; border-radius: 8px; font-weight: 700; color: alpha(white, 0.56); background: none; min-height: 0; }
.seg button:checked { background: alpha(white, 0.12); color: white; }
.seg.red button:checked { background: linear-gradient(to bottom, #e0193f, #a50c25); box-shadow: 0 3px 12px alpha(#c8102e, 0.45); }
.chips flowboxchild { padding: 0; background: none; }
.chips button { border-radius: 999px; padding: 5px 14px; font-weight: 650; color: alpha(white, 0.6); background: alpha(white, 0.04); border: 1px solid alpha(white, 0.07); }
.chips button:checked { color: white; background: alpha(white, 0.13); border-color: alpha(white, 0.16); }
.knob-name { font-size: 10px; letter-spacing: 1px; font-weight: 800; color: alpha(white, 0.56); }
.knob-val { font-weight: 800; font-feature-settings: "tnum"; }
.knob:focus-visible { outline: 2px solid alpha(#ff416c, 0.5); outline-offset: -3px; border-radius: 50%; }
.pwr { min-width: 18px; min-height: 18px; padding: 0; border-radius: 999px; background: alpha(black, 0.42); box-shadow: inset 0 1px 2px alpha(black, 0.6); }
.pwr .led { min-width: 6px; min-height: 6px; border-radius: 999px; background: #444; }
.pwr:checked { background: alpha(black, 0.42); }
.pwr:checked .led { background: #ff416c; box-shadow: 0 0 8px #ff416c; }
.np-kick { font-size: 10.5px; letter-spacing: 2.6px; font-weight: 700; color: alpha(white, 0.55); }
.np-title { font-size: 34px; font-weight: 900; letter-spacing: -1px; text-shadow: 0 4px 30px alpha(black, 0.6); }
.np-sub { font-size: 14px; color: alpha(white, 0.75); }
progressbar.thin trough { min-height: 2px; background: alpha(white, 0.15); border-radius: 2px; }
progressbar.thin progress { min-height: 2px; border-radius: 2px; }
progressbar.white progress { background: alpha(white, 0.85); }
progressbar.pink progress { background: #ff416c; }
.capsule { border-radius: 999px; padding: 8px 14px 8px 8px; background: alpha(#16161a, 0.82);
  border: 1px solid alpha(white, 0.10); box-shadow: 0 18px 40px alpha(black, 0.55), inset 0 1px 0 alpha(white, 0.08); }
.cover { min-width: 46px; min-height: 46px; border-radius: 999px; background: linear-gradient(135deg, #3a0b12, #c8102e); box-shadow: 0 0 0 2px alpha(white, 0.12); }
.cover.spin { animation: spin 6s linear infinite; }
@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
.cap-title { font-weight: 750; font-size: 13.5px; }
.cap-artist { font-size: 11.5px; color: alpha(white, 0.56); }
.cap-time { font-size: 10.5px; color: alpha(white, 0.56); font-feature-settings: "tnum"; }
.pathm > button { border-radius: 999px; padding: 5px 10px; background: alpha(white, 0.05); border: 1px solid alpha(white, 0.08); min-height: 0; }
.pathm > button:hover { background: alpha(white, 0.09); }
.path-text { font-size: 11px; color: alpha(white, 0.8); font-feature-settings: "tnum"; }
.path-card { padding: 14px 16px 10px 16px; }
.path-head { font-weight: 800; font-size: 14px; }
.path-detail { font-size: 12px; color: alpha(white, 0.6); }
.path-legend { border-top: 1px solid alpha(white, 0.07); padding-top: 10px; }
.pdot { min-width: 10px; min-height: 10px; border-radius: 999px; }
.pdot.same { background: #e8e8ec; box-shadow: 0 0 6px alpha(white, 0.5); }
.pdot.changed { background: #ffb347; box-shadow: 0 0 6px #ffb347; }
.pdot.enhanced { background: #ff416c; box-shadow: 0 0 6px #ff416c; }
.pline { min-width: 2px; min-height: 22px; background: alpha(white, 0.16); }
.side, .cc { background: #0d0d0e; }
.cc { border: 1px solid alpha(white, 0.12); border-radius: 18px; box-shadow: 0 24px 70px alpha(black, 0.85); }
.cc-scrim { background: alpha(black, 0.38); border: none; border-radius: 0; box-shadow: none; }
.cc-ro { font-family: "Anton"; font-size: 30px; color: white; line-height: 32px; }
.cc-sub { font-size: 12px; color: alpha(white, 0.6); }
.cc-tabs button { border-radius: 999px; padding: 6px 14px; min-height: 0; background: none; box-shadow: none; font-weight: 700; font-size: 13px; color: alpha(white, 0.62); }
.cc-tabs button:hover { background: alpha(white, 0.06); color: white; }
.cc-tabs button:checked { background: white; color: black; }
.cc separator { background: alpha(white, 0.08); min-height: 1px; }
.cc preferencesgroup .header label.description { color: alpha(white, 0.55); }
.cc list.boxed-list, .cc .boxed-list { background: alpha(white, 0.035); border: 1px solid alpha(white, 0.07); box-shadow: none; border-radius: 12px; }
.cc row:hover { background: alpha(white, 0.04); }
.cc .panel { background: alpha(white, 0.03); }
.cc switch:checked { background: #e3262f; }
.side-title { font-family: "Anton"; font-size: 28px; font-weight: 400; letter-spacing: 0.3px; }
.side preferencesgroup .header label.description { color: alpha(white, 0.55); }
.side list.boxed-list, .side .boxed-list { background: alpha(white, 0.035); border: 1px solid alpha(white, 0.07); box-shadow: none; border-radius: 12px; }
.side row:hover { background: alpha(white, 0.04); }
.side .panel { background: alpha(white, 0.03); }
.side switch:checked { background: #e3262f; }
.desk { border-radius: 14px; padding: 14px 10px; background: alpha(black, 0.25); border: 1px solid alpha(white, 0.07); }
.strip-val { font-weight: 700; font-size: 11px; font-feature-settings: "tnum"; }
.strip-name { font-size: 10.5px; color: alpha(white, 0.6); }
.strip scale trough { min-width: 6px; }
.strip scale highlight { background: linear-gradient(to top, #c8102e, #ff416c); }
.cv-tile { border-radius: 14px; padding: 10px 8px; background: alpha(white, 0.04); border: 1px solid alpha(white, 0.07); }
.tile-icon { color: alpha(white, 0.4); }
.menu-box { padding: 4px; }
"#;

fn main() -> glib::ExitCode {
    // GTK's Vulkan renderer (the default since 4.16) leaves stray bits of
    // other letters above text on the user's NVIDIA card. Use GTK's OpenGL
    // renderer instead, unless something else was asked for.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: set before GTK or any other thread starts.
        unsafe { std::env::set_var("GSK_RENDERER", "gl") };
    }
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        "apply" => {
            let s = model::Settings::load();
            worker::apply_configs(&s);
            println!("{}", if card::present() { "applied" } else { "card not available" });
            glib::ExitCode::SUCCESS
        }
        "watch" => worker::watch(),
        "detect" => {
            // Read-only: shows what's playing and what it would pick.
            for s in detect::streams() {
                println!("stream #{} {:?} binary={:?} role={:?} channels={} title={:?}", s.index, s.app, s.binary, s.role, s.channels, s.title);
            }
            for p in detect::players() {
                println!("player {} : {:?} by {:?} ({})", p.bus, p.title, p.artist, p.url);
            }
            let d = detect::detect();
            match &d {
                Some(d) => println!("=> {} ({}) {}", model::activity_label(d.activity), d.source, d.detail),
                None => println!("=> nothing to go by"),
            }
            if let Some(np) = media::now_playing(d.as_ref()) {
                println!("now playing: {np:#?}");
            }
            glib::ExitCode::SUCCESS
        }
        "selftest" => {
            // Never touch the real card or the real settings while testing.
            let dir = std::env::temp_dir().join(format!("sbcommand-selftest-{}", std::process::id()));
            // SAFETY: set before any other thread starts.
            unsafe {
                std::env::set_var("SBCOMMAND_DRYRUN", "1");
                std::env::set_var("SBCOMMAND_CONFIG_DIR", &dir);
            }
            gui(None, true)
        }
        "" | "home" | "effects" | "compare" | "sbx" | "crystalvoice" | "mic" | "recording" | "scout" | "speakers" | "playback"
        | "setup" | "mixer" | "eq" | "tone" | "settings" | "sound" | "look" => {
            gui((!arg.is_empty()).then_some(arg), false)
        }
        "-h" | "--help" | "help" => {
            println!("usage: sbcommand [sound|look|crystalvoice|scout|speakers|mixer|eq|settings|apply|watch|selftest]");
            glib::ExitCode::SUCCESS
        }
        other => {
            eprintln!("sbcommand: unknown command {other:?}");
            glib::ExitCode::FAILURE
        }
    }
}

fn gui(page: Option<String>, selftest: bool) -> glib::ExitCode {
    fonts::register();
    let separate = selftest || std::env::var_os("SBCOMMAND_SHOT").is_some();
    let flags = if separate { gtk::gio::ApplicationFlags::NON_UNIQUE } else { Default::default() };
    let app = adw::Application::builder().application_id(APP_ID).flags(flags).build();
    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        let css = gtk::CssProvider::new();
        css.load_from_string(&format!("{CSS}\n{}\n{}", stage::CSS, mini::CSS));
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        gtk::Window::set_default_icon_name(APP_ID);
    });
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let (etx, erx) = async_channel::unbounded();
        let jobs = worker::spawn(etx);
        let ui = ui::build(app, jobs, erx);
        ui::UI.with(|c| *c.borrow_mut() = Some(Rc::downgrade(&ui)));
        // The tray changes settings from the background service: reload them.
        if app.lookup_action("reload").is_none() {
            let reload = gtk::gio::SimpleAction::new("reload", None);
            reload.connect_activate(|_, _| {
                if let Some(u) = ui::UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade())) {
                    u.reload();
                }
            });
            app.add_action(&reload);
        }
        if let Some(p) = &page {
            ui.show_page(p);
        }
        if let Some((w, h)) = std::env::var("SBCOMMAND_SHOT_SIZE").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        }) {
            // A thin size is the Strip: switched to first, so the window may be that thin.
            if h < 150 { ui.set_strip(true) }
            ui.win.set_default_size(w, h);
        }
        ui.win.present();
        // Keep the UI alive for as long as the window is open.
        let keep = ui.clone();
        ui.win.connect_close_request(move |_| {
            keep.stop_video();
            glib::Propagation::Proceed
        });
        if selftest {
            selftest::run(ui.clone(), app.clone());
        }
        if let Ok(path) = std::env::var("SBCOMMAND_SHOT") {
            let win = ui.win.clone();
            let app = app.clone();
            // Also the signal path card, if asked for (it's a popover: its own surface).
            let path_pop = ui.path_box.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>();
            if std::env::var_os("SBCOMMAND_SHOT_PATH").is_some()
                && let Some(p) = &path_pop {
                    let p = p.clone();
                    glib::timeout_add_local_once(Duration::from_millis(1200), move || p.popup());
                }
            let delay = std::env::var("SBCOMMAND_SHOT_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(2200);
            glib::timeout_add_local_once(Duration::from_millis(delay), move || {
                if let (Ok(pp), Some(p)) = (std::env::var("SBCOMMAND_SHOT_PATH"), &path_pop) {
                    screenshot(p, &pp);
                }
                // A frame with a layout still pending draws nothing (a video
                // that just started changes size): try again on later frames.
                let tries = std::cell::Cell::new(0);
                glib::timeout_add_local(Duration::from_millis(120), move || {
                    tries.set(tries.get() + 1);
                    if !screenshot(&win, &path) && tries.get() < 25 {
                        return glib::ControlFlow::Continue;
                    }
                    // Stop any video first: GStreamer's GL thread can crash on exit mid-video.
                    if let Some(u) = ui::UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade())) {
                        u.stop_video();
                    }
                    app.quit();
                    glib::ControlFlow::Break
                });
            });
            if let Ok(how) = std::env::var("SBCOMMAND_SHOT_FULL") {
                let wk = Rc::downgrade(&ui);
                glib::timeout_add_local_once(Duration::from_millis(delay.saturating_sub(2500)), move || {
                    if let Some(u) = wk.upgrade() {
                        u.open_full(how == "trailer");
                    }
                });
            }
        }
    });
    app.run_with_args::<&str>(&[])
}

/// Renders the window to a PNG (used for checking the layout without a screen).
pub fn screenshot(win: &impl IsA<gtk::Widget>, path: &str) -> bool {
    let win = win.as_ref();
    let (w, h) = (win.width() as f64, win.height() as f64);
    let paintable = gtk::WidgetPaintable::new(Some(win));
    let snap = gtk::Snapshot::new();
    paintable.snapshot(&snap, w, h);
    match (snap.to_node(), win.native().and_then(|n| n.renderer())) {
        (Some(node), Some(renderer)) => {
            let tex = renderer.render_texture(&node, None);
            if let Err(e) = tex.save_to_png(path) {
                eprintln!("sbcommand: screenshot failed: {e}");
            }
            true
        }
        _ => false,
    }
}
