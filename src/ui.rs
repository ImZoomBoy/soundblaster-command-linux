//! The window (design "N6 / Studio split"): your room with live speakers on
//! the left, with what's playing across the top and a floating capsule with
//! the signal path; volume, SBX/Pure, 5.1/2.1, the SBX profile knobs and the
//! equaliser on the right; Mixer, CrystalVoice, Speaker setup and Settings
//! slide in from the side. It only edits settings and hands work to the
//! background worker, so it never waits on the card or PipeWire.

use crate::dash;
use crate::media::NowPlaying;
use crate::model::{self, Settings, MIC_BOOST, SPEAKER_CONFIGS, SPEAKER_CONFIG_LABELS, SPEAKER_TYPES,
                   SPEAKER_TYPE_LABELS, SVM_MODES, VOICE_MORPHS};
use crate::room;
use crate::worker::{part, Event, Job};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const BAND_LABELS: [&str; 10] = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
const HP_GAINS: [&str; 2] = ["Normal (up to 300 Ω)", "High (up to 600 Ω)"];
/// The card's output runs at 48 kHz.
const CARD_RATE: u32 = 48000;

/// Wraps a signal handler so it holds a weak reference to the UI and does
/// nothing while the UI is being filled in from the settings.
macro_rules! on {
    ($ui:expr, |$u:ident $(, $a:pat_param)*| $body:expr) => {{
        let wk: Weak<Ui> = Rc::downgrade(&$ui);
        move |$($a),*| {
            if let Some($u) = wk.upgrade() {
                if !$u.loading.get() {
                    $body
                }
            }
        }
    }};
}

/// One SBX effect: power LED, turning knob, value; Bass and Smart Volume also
/// have their crossover / mode in the profile menu.
pub struct FxRow {
    pub key: &'static str,
    pub col: gtk::Box,
    pub on: gtk::ToggleButton,
    pub level: gtk::Adjustment,
    pub knob: gtk::DrawingArea,
    pub value: gtk::Label,
    pub xover: Option<gtk::Scale>,
    pub mode: Option<gtk::DropDown>,
}

pub struct Ui {
    pub s: RefCell<Settings>,
    pub loading: Cell<bool>,
    jobs: Sender<Job>,
    timers: RefCell<HashMap<&'static str, glib::SourceId>>,
    pub card_ok: Cell<bool>,

    pub win: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    pub spinner: adw::Spinner,

    // ---- header
    pub act_btns: Vec<(&'static str, gtk::ToggleButton)>,
    pub tool_btns: Vec<(&'static str, gtk::ToggleButton)>,
    pub split: adw::OverlaySplitView,
    side_stack: gtk::Stack,
    side_title: gtk::Label,

    // ---- the room and what's playing
    pub room: gtk::DrawingArea,
    room_lit: RefCell<Option<String>>,
    pub levels: Cell<[f32; 6]>,
    meter: RefCell<Option<crate::meters::Meter>>,
    t0: Instant,
    np_head: gtk::Box,
    pub np_kick: gtk::Label,
    pub np_title: gtk::Label,
    pub np_sub: gtk::Label,
    np_prog: gtk::ProgressBar,
    pub capsule: gtk::Box,
    cap_cover: gtk::Box,
    cap_pic: gtk::Image,
    cap_icon: gtk::Image,
    pub cap_title: gtk::Label,
    cap_artist: gtk::Label,
    cap_time: gtk::Label,
    cap_prog: gtk::ProgressBar,
    path_dots: gtk::DrawingArea,
    pub path_label: gtk::Label,
    pub path_box: gtk::Box,
    pub np: RefCell<Option<NowPlaying>>,
    np_at: Cell<Instant>,
    /// When what's playing went away (it stays shown for a little while).
    pub np_gone: Cell<Option<Instant>>,
    path_kinds: RefCell<Vec<&'static str>>,
    playing_source: RefCell<Option<String>>,

    // ---- output card
    pub vol_adj: gtk::Adjustment,
    pub vol_label: gtk::Label,
    pub out_label: gtk::Label,
    pub mute: gtk::ToggleButton,
    vol_loading: Cell<bool>,
    pub sbx_btn: gtk::ToggleButton,
    pub pure_btn: gtk::ToggleButton,
    pub listen_btns: Vec<(&'static str, gtk::ToggleButton)>,
    pub stereo_btns: Vec<(&'static str, gtk::ToggleButton)>,
    stereo_row: gtk::Box,
    pub mode_desc: gtk::Label,
    pub out_switch: gtk::Button,

    // ---- SBX profile card
    chips: gtk::FlowBox,
    pub style_btns: RefCell<Vec<(String, gtk::ToggleButton)>>,
    chips_key: RefCell<String>,
    pub scout_box: gtk::Box,
    pub scout_sw: gtk::Switch,
    pub profile_btns: HashMap<&'static str, gtk::Button>,
    profile_card: gtk::Box,
    pub fx: Vec<FxRow>,

    // ---- equaliser card
    pub eq_on: gtk::Switch,
    pub eq_preset: gtk::DropDown,
    pub eq_names: RefCell<Vec<String>>,
    pub eq_btns: HashMap<&'static str, gtk::Button>,
    eq_card: gtk::Box,
    pub eq_bands: Vec<gtk::Scale>,
    eq_view: dash::EqCurve,

    // ---- Mixer
    mix_box: gtk::Box,
    mix_layout: RefCell<String>,
    pub mix_rows: RefCell<Vec<(String, gtk::Box, gtk::Scale)>>,
    mix_meters: RefCell<Vec<(usize, gtk::DrawingArea)>>,
    pub boost: gtk::Scale,
    boost_label: gtk::Label,
    pub mic_level: gtk::Scale,
    pub wuh_level: gtk::Scale,

    // ---- CrystalVoice
    pub cv_on: adw::SwitchRow,
    pub nr: gtk::ToggleButton,
    pub msvm: gtk::ToggleButton,
    pub msvm_level: gtk::Adjustment,
    pub focus: gtk::ToggleButton,
    pub focus_angle: gtk::Adjustment,
    cv_tiles: gtk::Box,
    cv_vals: (gtk::Label, gtk::Label),
    pub morph: gtk::DropDown,
    pub mic_boost: adw::ComboRow,
    pub wuh: adw::SwitchRow,
    pub voice_test: gtk::Button,

    // ---- Speaker setup
    pub cfg: adw::ComboRow,
    pub spk_type: adw::ComboRow,
    pub fr_front: adw::SwitchRow,
    pub fr_rear: adw::SwitchRow,
    pub bass_redir: adw::SwitchRow,
    pub xover: gtk::Scale,
    xover_row: adw::ActionRow,
    pub hp_gain: adw::ComboRow,
    test_group: adw::PreferencesGroup,
    test_layout: RefCell<String>,
    pub test_rows: RefCell<Vec<(String, adw::ActionRow, gtk::Button)>>,
    pub test_all: gtk::Button,

    // ---- Settings
    pub auto_combo: adw::ComboRow,
    pub remember_switch: adw::SwitchRow,
    pub genre_switch: adw::SwitchRow,
    pub tray_switch: adw::SwitchRow,
    pub refresh_btn: gtk::Button,
    pub reset_btn: gtk::Button,
    pub defaults_btn: gtk::Button,

    // ---- songs and switching
    song: RefCell<Option<String>>,
    song_pick: RefCell<Option<crate::genre::EqPick>>,
    /// The song for which you picked the EQ yourself (left alone until it changes).
    eq_override: RefCell<Option<String>>,
    /// The EQ before an automatic genre change, for Undo.
    eq_undo: RefCell<Option<String>>,
    /// Settings before an automatic switch, for Undo.
    undo_snapshot: RefCell<Option<Settings>>,
}

// ------------------------------------------------------------------ widget helpers

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(classes.to_vec()).build()
}

fn caps(text: &str) -> gtk::Label {
    label(&text.to_uppercase(), &["caps"])
}

fn group(title: &str, desc: Option<&str>) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::builder().title(title).build();
    if let Some(d) = desc {
        g.set_description(Some(d));
    }
    g
}

fn switch(title: &str, sub: &str) -> adw::SwitchRow {
    let r = adw::SwitchRow::builder().title(title).subtitle(sub).build();
    r.set_use_markup(false);
    r
}

fn combo(title: &str, items: &[&str]) -> adw::ComboRow {
    let r = adw::ComboRow::builder().title(title).model(&gtk::StringList::new(items)).build();
    r.set_use_markup(false);
    r
}

fn hscale(lo: f64, hi: f64, step: f64, fmt: fn(f64) -> String) -> gtk::Scale {
    let sc = gtk::Scale::with_range(gtk::Orientation::Horizontal, lo, hi, step);
    sc.set_size_request(200, -1);
    sc.set_valign(gtk::Align::Center);
    sc.set_draw_value(true);
    sc.set_value_pos(gtk::PositionType::Right);
    sc.set_format_value_func(move |_, v| fmt(v));
    sc
}

fn button(label: &str) -> gtk::Button {
    gtk::Button::builder().label(label).valign(gtk::Align::Center).build()
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).tooltip_text(tip).valign(gtk::Align::Center).css_classes(["flat", "circular"]).build()
}

/// Two or more toggle buttons that act as one choice.
fn segmented(items: &[&str], red: bool) -> (gtk::Box, Vec<gtk::ToggleButton>) {
    let b = gtk::Box::builder().css_classes(if red { vec!["seg", "red"] } else { vec!["seg"] }).homogeneous(true).build();
    let mut v: Vec<gtk::ToggleButton> = Vec::new();
    for it in items {
        let t = gtk::ToggleButton::with_label(it);
        if let Some(f) = v.first() {
            t.set_group(Some(f));
        }
        b.append(&t);
        v.push(t);
    }
    (b, v)
}

/// A ⋯ menu of plain buttons (and any extra widgets).
fn menu(labels: &[&'static str], extra: Option<&gtk::Widget>, tip: &str) -> (gtk::MenuButton, HashMap<&'static str, gtk::Button>) {
    let bx = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).css_classes(["menu-box"]).build();
    let pop = gtk::Popover::builder().child(&bx).build();
    let mut map = HashMap::new();
    if let Some(e) = extra {
        bx.append(e);
        bx.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    }
    for l in labels {
        let b = gtk::Button::builder().label(*l).css_classes(["flat"]).build();
        if let Some(lbl) = b.child().and_downcast::<gtk::Label>() {
            lbl.set_xalign(0.0);
        }
        let p = pop.clone();
        b.connect_clicked(move |_| p.popdown());
        bx.append(&b);
        map.insert(*l, b);
    }
    let mb = gtk::MenuButton::builder().icon_name("view-more-symbolic").tooltip_text(tip)
        .valign(gtk::Align::Center).css_classes(["flat", "circular"]).popover(&pop).build();
    (mb, map)
}

fn fmt_hz(v: f64) -> String {
    format!("{} Hz", v.round() as i64)
}
fn fmt_db(v: f64) -> String {
    format!("{v:+.1}")
}
fn fmt_db_int(v: f64) -> String {
    format!("{:+} dB", v.round() as i64)
}
fn mmss(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn set_strings(dd: &gtk::DropDown, cache: &RefCell<Vec<String>>, names: Vec<String>) {
    if *cache.borrow() != names {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        dd.set_model(Some(&gtk::StringList::new(&refs)));
        *cache.borrow_mut() = names;
    }
}

/// A vertical fader with its value, an optional live meter and its name.
fn strip(name: &str, lo: f64, hi: f64, step: f64, fmt: fn(f64) -> String, meter: Option<&gtk::DrawingArea>) -> (gtk::Box, gtk::Scale) {
    let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).css_classes(["strip"]).build();
    let val = label("", &["strip-val"]);
    val.set_xalign(0.5);
    let sc = gtk::Scale::with_range(gtk::Orientation::Vertical, lo, hi, step);
    sc.set_inverted(true);
    sc.set_size_request(-1, 200);
    sc.set_draw_value(false);
    if lo < 0.0 && hi >= 0.0 {
        sc.add_mark(0.0, gtk::PositionType::Right, None);
    }
    {
        let val = val.clone();
        let upd = move |s: &gtk::Scale| val.set_label(&fmt(s.value()));
        upd(&sc);
        sc.connect_value_changed(upd);
    }
    let row = gtk::Box::builder().spacing(4).halign(gtk::Align::Center).build();
    row.append(&sc);
    if let Some(m) = meter {
        row.append(m);
    }
    let nm = label(name, &["strip-name"]);
    nm.set_xalign(0.5);
    nm.set_wrap(true);
    nm.set_justify(gtk::Justification::Center);
    col.append(&val);
    col.append(&row);
    col.append(&nm);
    (col, sc)
}

fn meter_area(idx: usize) -> gtk::DrawingArea {
    let da = gtk::DrawingArea::builder().content_width(6).content_height(200).build();
    da.set_draw_func(move |_, cr, w, h| {
        let lv = UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade()).map(|u| u.levels.get()[idx]).unwrap_or(0.0));
        dash::draw_vmeter(cr, w as f64, h as f64, lv as f64);
    });
    da
}

fn ch_index(ch: &str) -> usize {
    crate::meters::ORDER.iter().position(|c| *c == ch).unwrap_or(0)
}

// ------------------------------------------------------------------ build

pub fn build(app: &adw::Application, jobs: Sender<Job>, events: async_channel::Receiver<Event>) -> Rc<Ui> {
    let s = Settings::load();
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("Sound Blaster Command")
        .default_width(1240)
        .default_height(800)
        .build();
    win.set_size_request(1040, 700);
    win.add_css_class("sbc");
    let toasts = adw::ToastOverlay::new();

    // ================================================================ header
    let brand = gtk::Box::builder().spacing(10).margin_start(4).build();
    let bl = gtk::Label::new(None);
    bl.set_markup("<b>Sound Blaster</b> <span alpha='60%'>Command</span>");
    brand.append(&bl);
    let tabs = gtk::Box::builder().spacing(2).build();
    let mut act_btns: Vec<(&'static str, gtk::ToggleButton)> = Vec::new();
    for (id, l, icon, hint) in model::ACTIVITIES {
        let b = gtk::Box::builder().spacing(8).build();
        b.append(&gtk::Image::from_icon_name(icon));
        b.append(&gtk::Label::new(Some(l)));
        let tb = gtk::ToggleButton::builder().child(&b).css_classes(["tab"]).tooltip_text(hint).build();
        if let Some((_, f)) = act_btns.first() {
            tb.set_group(Some(f));
        }
        tabs.append(&tb);
        act_btns.push((id, tb));
    }
    let header = adw::HeaderBar::new();
    header.pack_start(&brand);
    header.set_title_widget(Some(&tabs));
    let spinner = adw::Spinner::new();
    spinner.set_visible(false);
    let mut tool_btns: Vec<(&'static str, gtk::ToggleButton)> = Vec::new();
    for (id, icon, tip) in [("settings", "emblem-system-symbolic", "Settings"),
                            ("setup", "preferences-system-symbolic", "Speaker setup"),
                            ("crystalvoice", "audio-input-microphone-symbolic", "CrystalVoice (microphone)"),
                            ("mixer", "view-media-equalizer-symbolic", "Mixer")] {
        let b = gtk::ToggleButton::builder().icon_name(icon).tooltip_text(tip).css_classes(["flat", "tool"]).build();
        header.pack_end(&b);
        tool_btns.push((id, b));
    }
    tool_btns.reverse();
    header.pack_end(&spinner);
    let banner = adw::Banner::builder().title("The Sound Blaster isn't responding").button_label("Reset card").revealed(false).build();

    // ================================================================ the room and what's playing
    let room_da = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
    let np_kick = label("", &["np-kick"]);
    let np_title = label("", &["np-title"]);
    np_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let np_sub = label("", &["np-sub"]);
    np_sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let np_prog = gtk::ProgressBar::builder().css_classes(["thin", "white"]).width_request(260).halign(gtk::Align::Start).margin_top(10).build();
    let np_head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2)
        .halign(gtk::Align::Fill).valign(gtk::Align::Start).margin_start(28).margin_end(28).margin_top(22).can_target(false).build();
    np_head.append(&np_kick);
    np_head.append(&np_title);
    np_head.append(&np_sub);
    np_head.append(&np_prog);

    // GtkImage shows a picture at a fixed size (46 px), unlike GtkPicture.
    let cap_pic = gtk::Image::builder().pixel_size(46).build();
    let cap_icon = gtk::Image::builder().icon_name("audio-x-generic-symbolic").pixel_size(20).build();
    let cover_stack = gtk::Overlay::builder().child(&cap_pic).build();
    cover_stack.add_overlay(&cap_icon);
    let cap_cover = gtk::Box::builder().css_classes(["cover"]).width_request(46).height_request(46).valign(gtk::Align::Center).build();
    cap_cover.set_overflow(gtk::Overflow::Hidden);
    cap_cover.append(&cover_stack);
    let cap_title = label("", &["cap-title"]);
    cap_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cap_title.set_max_width_chars(22);
    let cap_artist = label("", &["cap-artist"]);
    cap_artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cap_artist.set_max_width_chars(24);
    let tt = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).build();
    tt.append(&cap_title);
    tt.append(&cap_artist);
    let cap_time = label("", &["cap-time"]);
    let cap_prog = gtk::ProgressBar::builder().css_classes(["thin", "pink"]).width_request(76).build();
    let tm = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(5).valign(gtk::Align::Center).build();
    tm.append(&cap_time);
    tm.append(&cap_prog);
    let path_dots = gtk::DrawingArea::builder().content_width(64).content_height(14).valign(gtk::Align::Center).build();
    let path_label = label("", &["path-text"]);
    let path_inner = gtk::Box::builder().spacing(9).build();
    path_inner.append(&path_dots);
    path_inner.append(&path_label);
    let path_box = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["path-card"]).width_request(380).build();
    let path_pop = gtk::Popover::builder().child(&path_box).position(gtk::PositionType::Top).build();
    let path_btn = gtk::MenuButton::builder().child(&path_inner).popover(&path_pop).css_classes(["pathm"])
        .tooltip_text("Signal path: how the sound gets from the app to your speakers").build();
    let capsule = gtk::Box::builder().spacing(11).css_classes(["capsule"]).halign(gtk::Align::Center).valign(gtk::Align::End)
        .margin_bottom(18).build();
    capsule.append(&cap_cover);
    capsule.append(&tt);
    capsule.append(&tm);
    capsule.append(&gtk::Separator::builder().orientation(gtk::Orientation::Vertical).margin_top(10).margin_bottom(10).build());
    capsule.append(&path_btn);
    let room_ov = gtk::Overlay::builder().child(&room_da).css_classes(["room-card"]).hexpand(true).vexpand(true).build();
    room_ov.set_overflow(gtk::Overflow::Hidden);
    room_ov.add_overlay(&np_head);
    room_ov.add_overlay(&capsule);

    // ================================================================ right column: output card
    let vol_adj = gtk::Adjustment::new(50.0, 0.0, 100.0, 1.0, 10.0, 0.0);
    let vol_knob = dash::knob(&vol_adj, None, 74, "Volume: turn, scroll or use the arrow keys");
    let out_label = caps("Speakers");
    let vol_label = label("", &["big-num"]);
    let mute = gtk::ToggleButton::builder().icon_name("audio-volume-high-symbolic").tooltip_text("Mute")
        .css_classes(["flat", "circular"]).valign(gtk::Align::Center).build();
    let vrow = gtk::Box::builder().spacing(6).build();
    vrow.append(&vol_label);
    vrow.append(&mute);
    let vtext = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).hexpand(true).build();
    vtext.append(&out_label);
    vtext.append(&vrow);
    let (mode_seg, mode_btns) = segmented(&["SBX", "Pure"], true);
    let (listen_seg, lbtns) = segmented(&["5.1", "2.1"], false);
    let segs = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).valign(gtk::Align::Center).build();
    segs.append(&mode_seg);
    segs.append(&listen_seg);
    let top = gtk::Box::builder().spacing(12).build();
    top.append(&vol_knob);
    top.append(&vtext);
    top.append(&segs);
    let (stereo_seg, sbtns) = segmented(&model::STEREO_ON.map(|x| x.1), false);
    for (b, (_, _, about)) in sbtns.iter().zip(model::STEREO_ON) {
        b.set_tooltip_text(Some(about));
    }
    let stereo_row = gtk::Box::builder().spacing(10).margin_top(12).build();
    let sl = caps("Stereo plays on");
    sl.set_hexpand(true);
    stereo_row.append(&sl);
    stereo_row.append(&stereo_seg);
    let mode_desc = label("", &["desc"]);
    mode_desc.set_wrap(true);
    mode_desc.set_max_width_chars(44);
    mode_desc.set_width_chars(30);
    mode_desc.set_hexpand(true);
    let out_switch = gtk::Button::builder().css_classes(["pill-btn"]).valign(gtk::Align::Center).build();
    let bottom = gtk::Box::builder().spacing(10).margin_top(10).build();
    bottom.append(&mode_desc);
    bottom.append(&out_switch);
    let out_card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["panel"]).build();
    out_card.append(&top);
    out_card.append(&stereo_row);
    out_card.append(&bottom);

    // ================================================================ SBX profile card
    let chips = gtk::FlowBox::builder().selection_mode(gtk::SelectionMode::None).column_spacing(6).row_spacing(6)
        .max_children_per_line(8).min_children_per_line(3).homogeneous(false).halign(gtk::Align::Start).css_classes(["chips"]).build();
    let scout_sw = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let scout_box = gtk::Box::builder().spacing(6).tooltip_text("Scout Mode: hear footsteps and quiet sounds sooner").build();
    scout_box.append(&gtk::Image::from_icon_name("find-location-symbolic"));
    scout_box.append(&label("Scout", &["dim"]));
    scout_box.append(&scout_sw);
    let mut fx = Vec::new();
    let knobs = gtk::Box::builder().homogeneous(true).spacing(4).margin_top(12).build();
    let xover_sc = hscale(50.0, 300.0, 10.0, fmt_hz);
    let sv_mode = gtk::DropDown::from_strings(&SVM_MODES);
    for (key, title, tip) in [
        ("surround", "Surround", "Wider, more spacious sound"),
        ("crystalizer", "Crystalizer", "Restores detail lost in compressed audio"),
        ("bass", "Bass", "Fuller, deeper bass (sent to the subwoofer through the EQ)"),
        ("smart_volume", "Smart Vol", "Evens out loud and quiet"),
        ("dialog", "Dialog+", "Clearer voices"),
    ] {
        let level = gtk::Adjustment::new(50.0, 0.0, 100.0, 1.0, 10.0, 0.0);
        let on = dash::power_button(&format!("{title} on/off"));
        let knob = dash::knob(&level, Some(&on), 78, tip);
        let head = gtk::Box::builder().spacing(6).halign(gtk::Align::Center).build();
        head.append(&label(&title.to_uppercase(), &["knob-name"]));
        head.append(&on);
        let value = label("", &["knob-val"]);
        value.set_xalign(0.5);
        let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).build();
        col.append(&head);
        col.append(&knob);
        col.append(&value);
        knobs.append(&col);
        let (xover, mode) = match key {
            "bass" => (Some(xover_sc.clone()), None),
            "smart_volume" => (None, Some(sv_mode.clone())),
            _ => (None, None),
        };
        fx.push(FxRow { key, col, on, level, knob, value, xover, mode });
    }
    let pm_extra = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_start(8).margin_end(8).margin_top(6).margin_bottom(6).build();
    pm_extra.append(&label("Bass crossover", &["caps-small"]));
    pm_extra.append(&xover_sc);
    pm_extra.append(&label("Smart Volume mode", &["caps-small"]));
    pm_extra.append(&sv_mode);
    let (pmenu, profile_btns) = menu(&["New…", "Rename…", "Revert", "Delete", "Import…", "Export…"],
        Some(pm_extra.upcast_ref()), "Profile: new, rename, revert, delete, import, export; Bass crossover, Smart Volume mode");
    let phead = gtk::Box::builder().spacing(8).build();
    let pl = caps("SBX profile");
    pl.set_hexpand(true);
    phead.append(&pl);
    phead.append(&scout_box);
    phead.append(&pmenu);
    let profile_card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).css_classes(["panel"]).build();
    profile_card.append(&phead);
    profile_card.append(&chips);
    profile_card.append(&knobs);

    // ================================================================ equaliser card
    let eq_on = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let eq_on_row = gtk::Box::builder().spacing(10).margin_start(8).margin_end(8).margin_top(6).margin_bottom(6).build();
    let eol = label("Equaliser on", &[]);
    eol.set_hexpand(true);
    eq_on_row.append(&eol);
    eq_on_row.append(&eq_on);
    let (emenu, eq_btns) = menu(&["New…", "Rename…", "Revert", "Delete", "Import…", "Export…"], Some(eq_on_row.upcast_ref()),
        "Preset: new, rename, revert, delete, import, export");
    let eq_preset = gtk::DropDown::from_strings(&[]);
    eq_preset.set_valign(gtk::Align::Center);
    let ehead = gtk::Box::builder().spacing(8).build();
    let el = caps("Equaliser");
    el.set_hexpand(true);
    ehead.append(&el);
    ehead.append(&eq_preset);
    ehead.append(&emenu);
    let eq_bands: Vec<gtk::Scale> = BAND_LABELS.iter().map(|_| gtk::Scale::with_range(gtk::Orientation::Vertical, -12.0, 12.0, 1.0)).collect();
    let eq_view = dash::eq_curve(&eq_bands);
    eq_view.area.set_content_height(170);
    eq_view.area.set_vexpand(true);
    let eq_card = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).css_classes(["panel"]).vexpand(true).build();
    eq_card.append(&ehead);
    eq_card.append(&eq_view.area);

    let right = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).width_request(500).hexpand(false).build();
    right.append(&out_card);
    right.append(&profile_card);
    right.append(&eq_card);
    let body = gtk::Box::builder().spacing(16).margin_start(16).margin_end(16).margin_bottom(16).margin_top(4).build();
    body.append(&room_ov);
    body.append(&right);

    // ================================================================ side panels
    // Mixer
    let mix_box = gtk::Box::builder().spacing(6).homogeneous(true).build();
    let (boost_col, boost) = strip("Speaker boost", 0.0, 9.0, 1.0, |v| format!("+{}", v.round() as i64), None);
    let (mic_col, mic_level) = strip("Microphone", -40.0, 9.0, 1.0, fmt_db_int, None);
    let (wuh_col, wuh_level) = strip("What U Hear", -40.0, 9.0, 1.0, fmt_db_int, None);
    let desk = gtk::Box::builder().spacing(10).css_classes(["desk"]).build();
    desk.append(&mix_box);
    desk.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    desk.append(&boost_col);
    let boost_label = label("", &["dim", "small"]);
    boost_label.set_wrap(true);
    let rec = gtk::Box::builder().spacing(10).css_classes(["desk"]).homogeneous(true).build();
    rec.append(&mic_col);
    rec.append(&wuh_col);
    let mix_page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    mix_page.append(&caps("Speakers"));
    mix_page.append(&desk);
    mix_page.append(&boost_label);
    mix_page.append(&caps("Recording"));
    mix_page.append(&rec);

    // CrystalVoice
    let cv_on = switch("CrystalVoice", "Creative's microphone effects");
    let nr = dash::power_button("Noise Reduction on/off");
    let msvm = dash::power_button("Smart Volume on/off");
    let msvm_level = gtk::Adjustment::new(74.0, 0.0, 100.0, 1.0, 10.0, 0.0);
    let focus = dash::power_button("Focus on/off");
    let focus_angle = gtk::Adjustment::new(60.0, 20.0, 180.0, 5.0, 20.0, 0.0);
    let cv_tile = |name: &str, power: &gtk::ToggleButton, body: &gtk::Widget, val: &gtk::Label| {
        let t = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).css_classes(["cv-tile"]).build();
        let h = gtk::Box::builder().spacing(6).build();
        let n = label(&name.to_uppercase(), &["knob-name"]);
        n.set_hexpand(true);
        h.append(&n);
        h.append(power);
        t.append(&h);
        t.append(body);
        val.set_xalign(0.5);
        t.append(val);
        t
    };
    let nr_val = label("", &["knob-val"]);
    let sv_val = label("", &["knob-val"]);
    let fo_val = label("", &["knob-val"]);
    let nr_icon = gtk::Image::builder().icon_name("audio-input-microphone-symbolic").pixel_size(34).height_request(74).css_classes(["tile-icon"]).build();
    let cv_tiles = gtk::Box::builder().spacing(8).homogeneous(true).build();
    cv_tiles.append(&cv_tile("Noise red.", &nr, nr_icon.upcast_ref(), &nr_val));
    cv_tiles.append(&cv_tile("Smart Vol", &msvm, dash::knob(&msvm_level, Some(&msvm), 70, "Smart Volume level").upcast_ref(), &sv_val));
    cv_tiles.append(&cv_tile("Focus", &focus, dash::knob(&focus_angle, Some(&focus), 70, "How wide the microphone listens").upcast_ref(), &fo_val));
    let morph = gtk::DropDown::from_strings(&VOICE_MORPHS);
    morph.set_valign(gtk::Align::Center);
    let morph_row = adw::ActionRow::builder().title("VoiceFX").subtitle("Changes how your voice sounds").build();
    morph_row.add_suffix(&morph);
    let mic_boost = combo("Microphone boost", &MIC_BOOST);
    let wuh = switch("What U Hear", "Lets you record whatever is playing");
    let voice_test = button("Test");
    let vt = adw::ActionRow::builder().title("Test your microphone").subtitle("Records 4 seconds, then plays it back").build();
    vt.add_suffix(&voice_test);
    let g_cv = group("", None);
    g_cv.add(&cv_on);
    let g_mic = group("Microphone", None);
    for w in [morph_row.upcast_ref::<gtk::Widget>(), mic_boost.upcast_ref(), wuh.upcast_ref(), vt.upcast_ref()] {
        g_mic.add(w);
    }
    let cv_page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).build();
    cv_page.append(&g_cv);
    cv_page.append(&cv_tiles);
    cv_page.append(&g_mic);

    // Speaker setup
    let cfg = combo("Speakers", &SPEAKER_CONFIG_LABELS);
    let spk_type = combo("Speaker type", &SPEAKER_TYPE_LABELS);
    let fr_front = switch("Full-range front speakers", "");
    let fr_rear = switch("Full-range rear speakers", "");
    let bass_redir = switch("Bass redirection", "Sends low frequencies to the subwoofer");
    let xover = hscale(50.0, 200.0, 10.0, fmt_hz);
    let xover_row = adw::ActionRow::builder().title("Crossover").subtitle("Bass below this goes to the subwoofer").build();
    xover_row.add_suffix(&xover);
    let hp_gain = combo("Headphone gain", &HP_GAINS);
    let g_spk = group("Speakers", None);
    for w in [cfg.upcast_ref::<gtk::Widget>(), spk_type.upcast_ref(), fr_front.upcast_ref(), fr_rear.upcast_ref(),
              bass_redir.upcast_ref(), xover_row.upcast_ref(), hp_gain.upcast_ref()] {
        g_spk.add(w);
    }
    let test_all = gtk::Button::builder().label("Test all").tooltip_text("Plays a short sound on each speaker in turn")
        .css_classes(["suggested-action"]).valign(gtk::Align::Center).build();
    let test_group = group("Test", Some("Plays a short sound on each speaker. You can also click a speaker in the room."));
    test_group.set_header_suffix(Some(&test_all));
    let setup_page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).build();
    setup_page.append(&g_spk);
    setup_page.append(&test_group);

    // Settings
    let auto_combo = combo("When something starts playing", &model::AUTO_MODE_LABELS);
    let remember_switch = switch("Remember settings per game and app", "Your last SBX profile and EQ come back when it plays again");
    let genre_switch = switch("Match the EQ to the song's genre online", "MusicBrainz, artist name only, once per artist");
    let tray_switch = switch("Show in the system tray", "Switch sound modes without opening this window");
    let g_auto = group("Automatic", None);
    for w in [auto_combo.upcast_ref::<gtk::Widget>(), remember_switch.upcast_ref(), genre_switch.upcast_ref(), tray_switch.upcast_ref()] {
        g_auto.add(w);
    }
    let refresh_btn = button("Fix");
    let rr = adw::ActionRow::builder().title("Fix fuzzy sound").subtitle("Sends every setting to the card again (Ctrl+R)").build();
    rr.add_suffix(&refresh_btn);
    let reset_btn = gtk::Button::builder().label("Reset").valign(gtk::Align::Center).css_classes(["destructive-action"]).build();
    let rs = adw::ActionRow::builder().title("Reset the sound card").subtitle("Restarts the card without restarting the PC: for fuzzy sound, a silent speaker or a card that stopped").build();
    rs.add_suffix(&reset_btn);
    let defaults_btn = button("Restore");
    let dfr = adw::ActionRow::builder().title("Restore defaults").subtitle("Every setting, your own profiles and presets").build();
    dfr.add_suffix(&defaults_btn);
    let g_q = group("Sound card", None);
    g_q.add(&adw::ActionRow::builder().title("Creative Sound Blaster ZxR").subtitle("CA0132 · distortion protection on").build());
    for w in [rr.upcast_ref::<gtk::Widget>(), rs.upcast_ref(), dfr.upcast_ref()] {
        g_q.add(w);
    }
    let set_page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(14).build();
    set_page.append(&g_auto);
    set_page.append(&g_q);

    let side_stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).vhomogeneous(false).build();
    for (id, w) in [("mixer", mix_page.upcast::<gtk::Widget>()), ("crystalvoice", cv_page.upcast()),
                    ("setup", setup_page.upcast()), ("settings", set_page.upcast())] {
        let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();
        let inner = gtk::Box::builder().orientation(gtk::Orientation::Vertical).margin_start(18).margin_end(18).margin_bottom(18).build();
        inner.append(&w);
        sw.set_child(Some(&inner));
        side_stack.add_named(&sw, Some(id));
    }
    let side_title = label("", &["side-title"]);
    side_title.set_hexpand(true);
    let side_close = icon_button("window-close-symbolic", "Close");
    let side_head = gtk::Box::builder().spacing(8).margin_start(18).margin_end(10).margin_top(14).margin_bottom(10).build();
    side_head.append(&side_title);
    side_head.append(&side_close);
    let side = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["side"]).build();
    side.append(&side_head);
    side.append(&side_stack);

    let content = adw::ToolbarView::new();
    content.add_top_bar(&header);
    content.add_top_bar(&banner);
    content.set_content(Some(&body));
    let split = adw::OverlaySplitView::builder().sidebar_position(gtk::PackType::End).collapsed(true).show_sidebar(false)
        .max_sidebar_width(460.0).min_sidebar_width(400.0).sidebar_width_fraction(0.36).build();
    split.set_sidebar(Some(&side));
    split.set_content(Some(&content));
    toasts.set_child(Some(&split));
    win.set_content(Some(&toasts));

    let listen_btns: Vec<(&'static str, gtk::ToggleButton)> = vec![("51", lbtns[0].clone()), ("21", lbtns[1].clone())];
    let stereo_btns: Vec<(&'static str, gtk::ToggleButton)> = model::STEREO_ON.iter().map(|x| x.0).zip(sbtns).collect();
    let ui = Rc::new(Ui {
        s: RefCell::new(s),
        loading: Cell::new(false),
        jobs,
        timers: RefCell::new(HashMap::new()),
        card_ok: Cell::new(true),
        win, toasts, banner, spinner,
        act_btns, tool_btns, split, side_stack, side_title,
        room: room_da, room_lit: RefCell::new(None), levels: Cell::new([0.0; 6]), meter: RefCell::new(None), t0: Instant::now(),
        np_head, np_kick, np_title, np_sub, np_prog, capsule, cap_cover, cap_pic, cap_icon, cap_title, cap_artist, cap_time, cap_prog,
        path_dots, path_label, path_box, np: RefCell::new(None), np_at: Cell::new(Instant::now()), np_gone: Cell::new(None),
        path_kinds: RefCell::new(vec![]), playing_source: RefCell::new(None),
        vol_adj, vol_label, out_label, mute, vol_loading: Cell::new(false),
        sbx_btn: mode_btns[0].clone(), pure_btn: mode_btns[1].clone(), listen_btns, stereo_btns, stereo_row, mode_desc, out_switch,
        chips, style_btns: RefCell::new(vec![]), chips_key: RefCell::new(String::new()), scout_box, scout_sw,
        profile_btns, profile_card, fx,
        eq_on, eq_preset, eq_names: RefCell::new(vec![]), eq_btns, eq_card, eq_bands, eq_view,
        mix_box, mix_layout: RefCell::new(String::new()), mix_rows: RefCell::new(vec![]), mix_meters: RefCell::new(vec![]),
        boost, boost_label, mic_level, wuh_level,
        cv_on, nr, msvm, msvm_level, focus, focus_angle, cv_tiles, cv_vals: (sv_val, fo_val), morph, mic_boost, wuh, voice_test,
        cfg, spk_type, fr_front, fr_rear, bass_redir, xover, xover_row, hp_gain,
        test_group, test_layout: RefCell::new(String::new()), test_rows: RefCell::new(vec![]), test_all,
        auto_combo, remember_switch, genre_switch, tray_switch, refresh_btn, reset_btn, defaults_btn,
        song: RefCell::new(None), song_pick: RefCell::new(None), eq_override: RefCell::new(None),
        eq_undo: RefCell::new(None), undo_snapshot: RefCell::new(None),
    });
    nr_val.set_visible(false);
    UI.with(|c| *c.borrow_mut() = Some(Rc::downgrade(&ui)));
    connect(&ui, &side_close);
    ui.refresh();
    ui.update_now_playing();

    let wk = Rc::downgrade(&ui);
    glib::spawn_future_local(async move {
        while let Ok(ev) = events.recv().await {
            let Some(ui) = wk.upgrade() else { break };
            match ev {
                Event::Toast(t) => ui.toast(&t),
                Event::CardPresent(ok) => {
                    if ok != ui.card_ok.get() {
                        ui.card_ok.set(ok);
                        ui.update_status();
                    }
                }
                Event::Busy(b) => ui.spinner.set_visible(b),
                Event::Volume(v, m) => {
                    ui.vol_loading.set(true);
                    ui.vol_adj.set_value(v as f64);
                    ui.mute.set_active(m);
                    ui.mute.set_icon_name(if m { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
                    ui.vol_loading.set(false);
                }
                Event::Detected(d, fresh) => ui.on_detected(d, fresh),
                Event::Genre(pick, song, fresh) => ui.on_genre(pick, song, fresh),
                Event::NowPlaying(np) => ui.on_now_playing(np.map(|b| *b)),
                Event::VoiceTestDone(ok) => {
                    ui.voice_test.set_sensitive(true);
                    ui.voice_test.set_label("Test");
                    if !ok {
                        ui.toast("The test couldn't record from the microphone");
                    }
                }
            }
        }
    });
    ui.send(Job::ReadVolume);
    ui.start_meters();
    ui
}

// ------------------------------------------------------------------ signals

fn connect(ui: &Rc<Ui>, side_close: &gtk::Button) {
    // Room: draw, animate the ripples while sound plays, click a speaker to test it.
    {
        let wk = Rc::downgrade(ui);
        ui.room.set_draw_func(move |_, cr, w, h| {
            if let Some(u) = wk.upgrade() {
                room::draw(cr, w as f64, h as f64, &u.room_state());
            }
        });
        let wk = Rc::downgrade(ui);
        let last = Cell::new(0i64);
        ui.room.add_tick_callback(move |da, clock| {
            let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
            let t = clock.frame_time();
            if t - last.get() > 33_000 && u.levels.get().iter().any(|l| *l > 0.02) {
                last.set(t);
                da.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        let wk = Rc::downgrade(ui);
        let click = gtk::GestureClick::new();
        click.connect_released(move |g, _, x, y| {
            let (Some(u), Some(da)) = (wk.upgrade(), g.widget()) else { return };
            if let Some(ch) = room::hit(da.width() as f64, da.height() as f64, &u.room_state(), x, y) {
                u.test_speaker(ch);
            }
        });
        ui.room.add_controller(click);
        let wk = Rc::downgrade(ui);
        ui.path_dots.set_draw_func(move |_, cr, _, h| {
            if let Some(u) = wk.upgrade() {
                dash::draw_path_dots(cr, h as f64, &u.path_kinds.borrow());
            }
        });
    }
    // Progress of the track, every second.
    {
        let wk = Rc::downgrade(ui);
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
            u.update_progress();
            glib::ControlFlow::Continue
        });
    }
    // Meters only while the window is shown.
    ui.win.connect_hide(on!(ui, |u, _w| { u.meter.borrow_mut().take(); }));
    ui.win.connect_show(on!(ui, |u, _w| u.start_meters()));

    // Header
    for (id, b) in &ui.act_btns {
        let id = *id;
        b.connect_toggled(on!(ui, |u, b| if b.is_active() && u.s.borrow().activity != id { u.set_activity(id) }));
    }
    for (id, b) in &ui.tool_btns {
        let id = *id;
        b.connect_toggled(on!(ui, |u, b| {
            if b.is_active() {
                u.show_page(id);
            } else if u.current_page() == id {
                u.split.set_show_sidebar(false);
            }
        }));
    }
    ui.split.connect_show_sidebar_notify(on!(ui, |u, sp| {
        if !sp.shows_sidebar() {
            u.loading.set(true);
            for (_, b) in &u.tool_btns {
                b.set_active(false);
            }
            u.loading.set(false);
        }
    }));
    {
        let wk = Rc::downgrade(ui);
        side_close.connect_clicked(move |_| {
            if let Some(u) = wk.upgrade() {
                u.split.set_show_sidebar(false);
            }
        });
    }

    // Output card
    ui.vol_adj.connect_value_changed(on!(ui, |u, a| {
        u.vol_label.set_label(&format!("{}%", a.value().round() as i64));
        if !u.vol_loading.get() {
            u.send(Job::Volume(a.value().round() as u32));
        }
    }));
    ui.mute.connect_toggled(on!(ui, |u, b| {
        if !u.vol_loading.get() {
            b.set_icon_name(if b.is_active() { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
            u.send(Job::Mute(b.is_active()));
        }
    }));
    ui.sbx_btn.connect_toggled(on!(ui, |u, b| if b.is_active() { u.set_sbx(true) }));
    ui.pure_btn.connect_toggled(on!(ui, |u, b| if b.is_active() { u.set_sbx(false) }));
    for (id, b) in &ui.listen_btns {
        let id = *id;
        b.connect_toggled(on!(ui, |u, b| if b.is_active() { u.set_listen(id) }));
    }
    for (id, b) in &ui.stereo_btns {
        let id = *id;
        b.connect_toggled(on!(ui, |u, b| if b.is_active() { u.set_stereo_on(id) }));
    }
    ui.out_switch.connect_clicked(on!(ui, |u, _b| u.ask_switch_output()));

    // SBX profile card
    ui.scout_sw.connect_active_notify(on!(ui, |u, sw| u.set_scout(sw.is_active())));
    for row in &ui.fx {
        row.on.connect_toggled(on!(ui, |u, _r| u.fx_from_widgets()));
        row.level.connect_value_changed(on!(ui, |u, _r| u.fx_from_widgets()));
        if let Some(x) = &row.xover {
            x.connect_value_changed(on!(ui, |u, _r| u.fx_from_widgets()));
        }
        if let Some(m) = &row.mode {
            m.connect_selected_notify(on!(ui, |u, _r| u.fx_from_widgets()));
        }
    }
    let pb = &ui.profile_btns;
    pb["New…"].connect_clicked(on!(ui, |u, _b| u.new_item(true)));
    pb["Rename…"].connect_clicked(on!(ui, |u, _b| u.rename_item(true)));
    pb["Revert"].connect_clicked(on!(ui, |u, _b| u.revert_item(true)));
    pb["Delete"].connect_clicked(on!(ui, |u, _b| u.delete_item(true)));
    pb["Import…"].connect_clicked(on!(ui, |u, _b| u.import_item(true)));
    pb["Export…"].connect_clicked(on!(ui, |u, _b| u.export_item(true)));

    // Equaliser card
    ui.eq_on.connect_active_notify(on!(ui, |u, r| {
        u.s.borrow_mut().eq_enabled = r.is_active();
        u.changed(part::EQ);
        u.refresh();
    }));
    ui.eq_preset.connect_selected_notify(on!(ui, |u, r| {
        let name = u.eq_names.borrow().get(r.selected() as usize).cloned();
        if let Some(n) = name && n != u.s.borrow().cur_eq() {
            u.pick_eq(&n);
        }
    }));
    for (i, sc) in ui.eq_bands.iter().enumerate() {
        sc.connect_value_changed(on!(ui, |u, sc| {
            let v = sc.value().round() as i32;
            {
                let mut s = u.s.borrow_mut();
                let name = s.cur_eq();
                let mut curve = s.eq_of(&name);
                if curve[i] == v {
                    return;
                }
                curve[i] = v;
                s.eq_presets.insert(name, curve);
                s.eq_enabled = true;
            }
            *u.eq_override.borrow_mut() = u.song.borrow().clone();
            u.changed(part::EQ);
            u.update_item_buttons();
        }));
    }
    let eb = &ui.eq_btns;
    eb["New…"].connect_clicked(on!(ui, |u, _b| u.new_item(false)));
    eb["Rename…"].connect_clicked(on!(ui, |u, _b| u.rename_item(false)));
    eb["Revert"].connect_clicked(on!(ui, |u, _b| u.revert_item(false)));
    eb["Delete"].connect_clicked(on!(ui, |u, _b| u.delete_item(false)));
    eb["Import…"].connect_clicked(on!(ui, |u, _b| u.import_item(false)));
    eb["Export…"].connect_clicked(on!(ui, |u, _b| u.export_item(false)));

    // Mixer
    ui.boost.connect_value_changed(on!(ui, |u, sc| {
        u.s.borrow_mut().boost_db = sc.value().round();
        u.changed(part::VOLUMES);
        u.refresh_summary();
    }));
    ui.mic_level.connect_value_changed(on!(ui, |u, sc| {
        u.s.borrow_mut().rec.mic_level = Some(sc.value().round() as i32);
        u.changed(part::REC);
    }));
    ui.wuh_level.connect_value_changed(on!(ui, |u, sc| {
        u.s.borrow_mut().rec.wuh_level = Some(sc.value().round() as i32);
        u.changed(part::REC);
    }));

    // CrystalVoice
    ui.mic_boost.connect_selected_notify(on!(ui, |u, _r| u.rec_from_widgets()));
    ui.morph.connect_selected_notify(on!(ui, |u, _r| u.rec_from_widgets()));
    for w in [&ui.wuh, &ui.cv_on] {
        w.connect_active_notify(on!(ui, |u, _r| u.rec_from_widgets()));
    }
    for w in [&ui.nr, &ui.msvm, &ui.focus] {
        w.connect_toggled(on!(ui, |u, _r| u.rec_from_widgets()));
    }
    for w in [&ui.msvm_level, &ui.focus_angle] {
        w.connect_value_changed(on!(ui, |u, _r| u.rec_from_widgets()));
    }
    ui.voice_test.connect_clicked(on!(ui, |u, b| {
        b.set_sensitive(false);
        b.set_label("Speak now…");
        u.send(Job::VoiceTest(Box::new(u.s.borrow().clone())));
    }));

    // Speaker setup
    ui.cfg.connect_selected_notify(on!(ui, |u, r| {
        u.s.borrow_mut().speaker_config = SPEAKER_CONFIGS[(r.selected() as usize).min(4)].into();
        u.changed(part::ALL);
        u.send(Job::Outputs(Box::new(u.s.borrow().clone())));
        u.refresh();
    }));
    ui.spk_type.connect_selected_notify(on!(ui, |u, r| {
        {
            let mut s = u.s.borrow_mut();
            let t = SPEAKER_TYPES[(r.selected() as usize).min(SPEAKER_TYPES.len() - 1)];
            s.speaker_type = t.into();
            match t {
                "dtt2200" => {
                    (s.full_range_front, s.full_range_rear) = (false, false);
                    s.bass_redirect = true;
                    s.crossover = model::DTT2200_CROSSOVER;
                }
                "desktop" => (s.full_range_front, s.full_range_rear) = (false, false),
                "bookshelf" => (s.full_range_front, s.full_range_rear) = (true, false),
                "tower" => (s.full_range_front, s.full_range_rear) = (true, true),
                _ => {}
            }
        }
        u.changed(part::SPEAKER);
        u.refresh();
    }));
    for w in [&ui.fr_front, &ui.fr_rear, &ui.bass_redir] {
        w.connect_active_notify(on!(ui, |u, _r| u.speaker_from_widgets()));
    }
    ui.xover.connect_value_changed(on!(ui, |u, _r| u.speaker_from_widgets()));
    ui.hp_gain.connect_selected_notify(on!(ui, |u, r| {
        u.s.borrow_mut().hp_600ohm = r.selected() == 1;
        u.changed(part::SPEAKER);
    }));
    ui.test_all.connect_clicked(on!(ui, |u, _b| u.test_all_speakers()));

    // Settings
    ui.auto_combo.connect_selected_notify(on!(ui, |u, r| {
        u.set_auto_mode(model::AUTO_MODES[(r.selected() as usize).min(2)]);
    }));
    ui.remember_switch.connect_active_notify(on!(ui, |u, r| {
        u.s.borrow_mut().remember_apps = r.is_active();
        u.save_now();
    }));
    ui.genre_switch.connect_active_notify(on!(ui, |u, r| {
        u.s.borrow_mut().genre_online = r.is_active();
        u.save_now();
    }));
    ui.tray_switch.connect_active_notify(on!(ui, |u, r| {
        u.s.borrow_mut().tray = r.is_active();
        u.save_now();
    }));
    ui.refresh_btn.connect_clicked(on!(ui, |u, _b| u.fix_sound()));
    ui.reset_btn.connect_clicked(on!(ui, |u, _b| u.ask_reset_card()));
    ui.defaults_btn.connect_clicked(on!(ui, |u, _b| u.confirm_defaults()));
    {
        let wk = Rc::downgrade(ui);
        ui.banner.connect_button_clicked(move |_| {
            if let Some(u) = wk.upgrade() {
                u.send(Job::ResetCard);
            }
        });
    }

    // Window actions (toast buttons, shortcut, tray)
    type Handler = Box<dyn Fn(&Ui, Option<&glib::Variant>)>;
    let add = |name: &str, param: Option<&glib::VariantTy>, f: Handler| {
        let a = gio::SimpleAction::new(name, param);
        let wk = Rc::downgrade(ui);
        a.connect_activate(move |_, p| {
            if let Some(u) = wk.upgrade() {
                f(&u, p);
            }
        });
        ui.win.add_action(&a);
    };
    add("fix-sound", None, Box::new(|u, _| u.fix_sound()));
    add("undo-switch", None, Box::new(|u, _| u.undo_switch()));
    add("undo-eq", None, Box::new(|u, _| u.undo_eq()));
    add("use-eq", Some(glib::VariantTy::STRING), Box::new(|u, p| {
        if let Some(e) = p.and_then(|v| v.str().map(str::to_string)) {
            u.apply_eq(&e);
        }
    }));
    add("use-activity", Some(glib::VariantTy::STRING), Box::new(|u, p| {
        if let Some(a) = p.and_then(|v| v.str().map(str::to_string)) {
            u.set_activity(&a);
        }
    }));
    if let Some(app) = ui.win.application() {
        app.set_accels_for_action("win.fix-sound", &["<Control>r"]);
    }
}

// ------------------------------------------------------------------ behaviour

impl Ui {
    pub fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn toast(&self, text: &str) {
        self.add_toast(adw::Toast::builder().use_markup(false).title(text).timeout(4).build());
    }

    pub fn add_toast(&self, toast: adw::Toast) {
        self.toasts.add_toast(toast);
    }

    /// Saves (debounced), sends the changed parts to the card, and remembers
    /// the profile and EQ for the game or app that's playing.
    fn changed(&self, parts: u32) {
        if parts & (part::FX | part::EQ) != 0
            && let Some(src) = self.playing_source.borrow().clone() {
                self.s.borrow_mut().remember_app(&src);
            }
        self.send(Job::Apply(Box::new(self.s.borrow().clone()), parts));
        self.debounce("save", 300, |u| u.save_now());
        self.update_status();
    }

    pub fn save_now(&self) {
        if let Err(e) = self.s.borrow().save() {
            self.toast(&format!("Couldn't save settings: {e}"));
        }
    }

    fn debounce(&self, key: &'static str, ms: u64, f: fn(&Ui)) {
        if let Some(id) = self.timers.borrow_mut().remove(key) {
            id.remove();
        }
        let wk = UI.with(|c| c.borrow().clone());
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            if let Some(u) = wk.and_then(|w| w.upgrade()) {
                u.timers.borrow_mut().remove(key);
                f(&u);
            }
        });
        self.timers.borrow_mut().insert(key, id);
    }

    /// Settings changed elsewhere (the tray): load them again.
    pub fn reload(&self) {
        *self.s.borrow_mut() = Settings::load();
        self.refresh();
    }

    // ---- activities, SBX, Scout, listening

    pub fn set_activity(&self, id: &str) {
        self.s.borrow_mut().set_activity(id);
        self.changed(part::FX | part::EQ | part::SPEAKER);
        self.refresh();
    }

    pub fn set_style(&self, style: &str) {
        self.s.borrow_mut().set_style(style);
        self.changed(part::FX | part::EQ);
        self.refresh();
    }

    pub fn set_sbx(&self, on: bool) {
        if self.s.borrow().sbx_enabled == on {
            return;
        }
        self.s.borrow_mut().sbx_enabled = on;
        self.changed(part::FX | part::EQ);
        self.refresh();
    }

    pub fn set_scout(&self, on: bool) {
        {
            let mut s = self.s.borrow_mut();
            if s.game_footsteps == on {
                return;
            }
            s.game_footsteps = on;
            s.scout = on && s.activity == "game";
        }
        self.changed(part::FX | part::EQ);
        self.refresh();
    }

    pub fn set_listen(&self, mode: &str) {
        if self.s.borrow().listen == mode {
            return;
        }
        self.s.borrow_mut().listen = mode.into();
        self.save_now();
        self.changed(part::FX);
        self.send(Job::Listen(Box::new(self.s.borrow().clone())));
        self.refresh();
    }

    /// Where stereo plays on 5.1 speakers: fronts, + rears, or all.
    pub fn set_stereo_on(&self, mode: &str) {
        if self.s.borrow().stereo_on == mode {
            return;
        }
        self.s.borrow_mut().stereo_on = mode.into();
        self.save_now();
        self.send(Job::Listen(Box::new(self.s.borrow().clone())));
        self.refresh();
    }

    fn set_auto_mode(&self, mode: &str) {
        self.s.borrow_mut().auto_mode = mode.into();
        self.save_now();
        self.refresh();
    }

    /// The SBX profile chips: Creative's for this activity, then your own.
    fn rebuild_chips(&self, s: &Settings) {
        let mut names: Vec<String> = model::styles_for(&s.activity).iter().map(|x| x.to_string()).collect();
        names.extend(s.fx_profiles.keys().filter(|k| !model::is_fx_builtin(k)).cloned());
        let key = names.join("|");
        if *self.chips_key.borrow() != key {
            while let Some(c) = self.chips.first_child() {
                self.chips.remove(&c);
            }
            let mut btns = self.style_btns.borrow_mut();
            btns.clear();
            let wk = UI.with(|c| c.borrow().clone());
            for name in &names {
                let b = gtk::ToggleButton::builder().label(name).tooltip_text(model::profile_about(name)).build();
                if let Some((_, first)) = btns.first() {
                    b.set_group(Some(first));
                }
                let n = name.clone();
                let wk2 = wk.clone();
                b.connect_toggled(move |b| {
                    let Some(u) = wk2.as_ref().and_then(|w| w.upgrade()) else { return };
                    if !u.loading.get() && b.is_active() {
                        u.set_style(&n);
                    }
                });
                self.chips.insert(&b, -1);
                btns.push((name.clone(), b));
            }
            let mut c = self.chips.first_child();
            while let Some(w) = c {
                w.set_focusable(false);
                c = w.next_sibling();
            }
            *self.chips_key.borrow_mut() = key;
        }
        let cur = s.cur_profile();
        for (n, b) in self.style_btns.borrow().iter() {
            b.set_active(*n == cur);
        }
    }

    // ---- SBX effects

    fn fx_from_widgets(&self) {
        {
            let mut s = self.s.borrow_mut();
            let name = s.cur_profile();
            let mut p = s.fx_of(&name);
            for r in &self.fx {
                let on = r.on.is_active();
                let lvl = r.level.value().round() as i32;
                match r.key {
                    "surround" => p.surround = (on, lvl),
                    "crystalizer" => p.crystalizer = (on, lvl),
                    "bass" => {
                        p.bass = (on, lvl);
                        if let Some(x) = &r.xover {
                            p.bass_xover = x.value().round() as i32;
                        }
                    }
                    "smart_volume" => {
                        let mode = r.mode.as_ref().map_or(0, |m| m.selected() as usize).min(2);
                        p.smart_volume = (on, lvl, SVM_MODES[mode].to_string());
                    }
                    _ => p.dialog = (on, lvl),
                }
            }
            s.fx_profiles.insert(name, p);
        }
        self.fx_values();
        // Bass may be applied through the EQ (5.1), so send both.
        self.changed(part::FX | part::EQ);
        self.update_item_buttons();
        self.refresh_summary();
    }

    fn fx_values(&self) {
        let sbx = self.s.borrow().sbx_active();
        let via_eq = model::bass_through_eq(&self.s.borrow());
        for r in &self.fx {
            r.col.set_sensitive(sbx);
            r.value.set_label(&if r.on.is_active() { format!("{}", r.level.value().round() as i64) } else { "OFF".into() });
            if r.key == "bass" {
                r.knob.set_tooltip_text(Some(if via_eq {
                    "Fuller, deeper bass. With a subwoofer the Linux driver turns the card's own Bass off, so it's applied through the EQ and goes to the subwoofer."
                } else { "Fuller, deeper bass" }));
            }
            r.knob.queue_draw();
        }
    }

    // ---- CrystalVoice

    fn cv_sensitivity(&self) {
        let on = self.cv_on.is_active();
        self.cv_tiles.set_sensitive(on);
        self.morph.set_sensitive(on);
        self.cv_vals.0.set_label(&if self.msvm.is_active() { format!("{}", self.msvm_level.value().round() as i64) } else { "OFF".into() });
        self.cv_vals.1.set_label(&if self.focus.is_active() { format!("{}°", self.focus_angle.value().round() as i64) } else { "OFF".into() });
    }

    fn rec_from_widgets(&self) {
        {
            let mut s = self.s.borrow_mut();
            let r = &mut s.rec;
            r.mic_boost = MIC_BOOST[(self.mic_boost.selected() as usize).min(3)].into();
            r.morph = VOICE_MORPHS[(self.morph.selected() as usize).min(VOICE_MORPHS.len() - 1)].into();
            r.what_u_hear = self.wuh.is_active();
            r.voice_clarity = self.cv_on.is_active();
            r.noise_reduction = self.nr.is_active();
            r.smart_volume = self.msvm.is_active();
            r.svm_level = self.msvm_level.value().round() as i32;
            r.focus = self.focus.is_active();
            r.focus_angle = self.focus_angle.value().round() as i32;
        }
        self.cv_sensitivity();
        self.changed(part::REC);
    }

    // ---- Speakers

    fn speaker_from_widgets(&self) {
        {
            let mut s = self.s.borrow_mut();
            s.full_range_front = self.fr_front.is_active();
            s.full_range_rear = self.fr_rear.is_active();
            s.bass_redirect = self.bass_redir.is_active();
            s.crossover = self.xover.value().round() as i32;
            s.speaker_type = "custom".into();
        }
        self.changed(part::SPEAKER | part::EQ);
        self.refresh();
    }

    // ---- equaliser

    /// A preset chosen by you: it stays for this song.
    fn pick_eq(&self, name: &str) {
        *self.eq_override.borrow_mut() = self.song.borrow().clone();
        self.apply_eq(name);
    }

    /// Sets the EQ without counting it as your own choice.
    pub fn apply_eq(&self, name: &str) {
        self.s.borrow_mut().set_eq(name);
        self.changed(part::EQ);
        self.refresh();
    }

    pub fn undo_eq(&self) {
        if let Some(prev) = self.eq_undo.borrow_mut().take() {
            *self.eq_override.borrow_mut() = self.song.borrow().clone();
            self.apply_eq(&prev);
        }
    }

    // ---- profile / preset management (fx = true for SBX profiles, false for EQ)

    fn update_item_buttons(&self) {
        let s = self.s.borrow();
        let cur = s.cur_profile();
        let builtin = model::is_fx_builtin(&cur);
        self.profile_btns["Rename…"].set_sensitive(!builtin);
        self.profile_btns["Delete"].set_sensitive(!builtin);
        self.profile_btns["Revert"].set_sensitive(builtin && s.fx_profiles.contains_key(&cur));
        let cur = s.cur_eq();
        let builtin = model::is_eq_builtin(&cur);
        self.eq_btns["Rename…"].set_sensitive(!builtin);
        self.eq_btns["Delete"].set_sensitive(!builtin);
        self.eq_btns["Revert"].set_sensitive(builtin && s.eq_presets.contains_key(&cur));
    }

    fn current(&self, fx: bool) -> String {
        let s = self.s.borrow();
        if fx { s.cur_profile() } else { s.cur_eq() }
    }

    fn exists(&self, fx: bool, name: &str) -> bool {
        let s = self.s.borrow();
        if fx { s.has_profile(name) } else { s.has_eq(name) }
    }

    fn is_builtin(fx: bool, name: &str) -> bool {
        if fx { model::is_fx_builtin(name) } else { model::is_eq_builtin(name) }
    }

    fn after_item_change(&self, fx: bool) {
        self.save_now();
        self.changed(if fx { part::FX | part::EQ } else { part::EQ });
        self.refresh();
    }

    pub fn create_copy(&self, fx: bool, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            self.toast("Names must be 1 to 60 characters");
            return false;
        }
        if self.exists(fx, name) {
            self.toast(&format!("There's already one called {name}"));
            return false;
        }
        let cur = self.current(fx);
        {
            let mut s = self.s.borrow_mut();
            if fx {
                let p = s.fx_of(&cur);
                s.fx_profiles.insert(name.into(), p);
                s.set_style(name);
            } else {
                let c = s.eq_of(&cur);
                s.eq_presets.insert(name.into(), c);
                s.set_eq(name);
            }
        }
        self.after_item_change(fx);
        true
    }

    pub fn rename_current(&self, fx: bool, name: &str) -> bool {
        let cur = self.current(fx);
        let name = name.trim();
        if Self::is_builtin(fx, &cur) {
            self.toast("Creative's own can't be renamed. Use New… to make a copy.");
            return false;
        }
        if name.is_empty() || name.chars().count() > 60 {
            self.toast("Names must be 1 to 60 characters");
            return false;
        }
        if self.exists(fx, name) {
            self.toast(&format!("There's already one called {name}"));
            return false;
        }
        {
            let mut guard = self.s.borrow_mut();
            let s = &mut *guard;
            let map: &mut std::collections::BTreeMap<String, String> = if fx { &mut s.styles } else { &mut s.eqs };
            for v in map.values_mut() {
                if *v == cur {
                    *v = name.into();
                }
            }
            if fx {
                if let Some(p) = s.fx_profiles.remove(&cur) {
                    s.fx_profiles.insert(name.into(), p);
                }
                for slot in [&mut s.profile.speakers, &mut s.profile.headphones] {
                    if *slot == cur {
                        *slot = name.into();
                    }
                }
            } else {
                if let Some(c) = s.eq_presets.remove(&cur) {
                    s.eq_presets.insert(name.into(), c);
                }
                for slot in [&mut s.eq.speakers, &mut s.eq.headphones] {
                    if *slot == cur {
                        *slot = name.into();
                    }
                }
            }
        }
        self.after_item_change(fx);
        true
    }

    fn new_item(&self, fx: bool) {
        let cur = self.current(fx);
        let title = if fx { "New profile (copy of this one)" } else { "New preset (copy of this one)" };
        self.ask_name(title, &format!("{cur} copy"), move |u, name| {
            u.create_copy(fx, &name);
        });
    }

    fn rename_item(&self, fx: bool) {
        let cur = self.current(fx);
        if Self::is_builtin(fx, &cur) {
            self.toast("Creative's own can't be renamed. Use New… to make a copy.");
            return;
        }
        self.ask_name("Rename", &cur, move |u, name| {
            u.rename_current(fx, &name);
        });
    }

    pub fn revert_item(&self, fx: bool) {
        let cur = self.current(fx);
        if !Self::is_builtin(fx, &cur) {
            self.toast("Only Creative's own can be reverted");
            return;
        }
        {
            let mut s = self.s.borrow_mut();
            if fx { s.fx_profiles.remove(&cur); } else { s.eq_presets.remove(&cur); }
        }
        self.after_item_change(fx);
        self.toast(&format!("{cur} is back to Creative's settings"));
    }

    pub fn delete_item(&self, fx: bool) {
        let cur = self.current(fx);
        if Self::is_builtin(fx, &cur) {
            self.toast("Creative's own can't be deleted");
            return;
        }
        {
            let mut guard = self.s.borrow_mut();
            let s = &mut *guard;
            let fallback = if fx { "Music" } else { "Flat" };
            let map: &mut std::collections::BTreeMap<String, String> = if fx { &mut s.styles } else { &mut s.eqs };
            map.retain(|_, v| *v != cur);
            if fx {
                s.fx_profiles.remove(&cur);
                for slot in [&mut s.profile.speakers, &mut s.profile.headphones] {
                    if *slot == cur {
                        *slot = fallback.into();
                    }
                }
            } else {
                s.eq_presets.remove(&cur);
                for slot in [&mut s.eq.speakers, &mut s.eq.headphones] {
                    if *slot == cur {
                        *slot = fallback.into();
                    }
                }
            }
            let a = s.activity.clone();
            s.set_activity(&a);
        }
        self.after_item_change(fx);
        self.toast(&format!("Deleted {cur}"));
    }

    pub fn export_to(&self, fx: bool, path: &std::path::Path) -> Result<(), String> {
        let cur = self.current(fx);
        let s = self.s.borrow();
        let json = if fx {
            serde_json::json!({"type": "sbcommand-sbx-profile", "name": cur, "settings": s.fx_of(&cur)})
        } else {
            serde_json::json!({"type": "sbcommand-eq-preset", "name": cur, "bands_hz": BAND_LABELS, "gain_db": s.eq_of(&cur)})
        };
        std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default()).map_err(|e| e.to_string())
    }

    pub fn import_from(&self, fx: bool, path: &std::path::Path) -> Result<String, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| "not a JSON file".to_string())?;
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into());
        let mut name = v["name"].as_str().map(str::to_string).unwrap_or(stem);
        name = name.chars().take(60).collect();
        while self.exists(fx, &name) {
            name.push_str(" (imported)");
        }
        {
            let mut s = self.s.borrow_mut();
            if fx {
                let p: model::Fx = serde_json::from_value(v["settings"].clone()).map_err(|_| "no profile in this file".to_string())?;
                s.fx_profiles.insert(name.clone(), p);
                s.sanitise();
                s.set_style(&name);
            } else {
                let curve: Vec<i32> = serde_json::from_value(v["gain_db"].clone()).map_err(|_| "no EQ preset in this file".to_string())?;
                if curve.len() != 10 {
                    return Err("an EQ preset needs 10 bands".into());
                }
                s.eq_presets.insert(name.clone(), curve);
                s.sanitise();
                s.set_eq(&name);
            }
        }
        self.after_item_change(fx);
        Ok(name)
    }

    fn import_item(&self, fx: bool) {
        let wk = UI.with(|c| c.borrow().clone());
        file_dialog(&self.win, false, None, move |path| {
            if let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                match u.import_from(fx, &path) {
                    Ok(n) => u.toast(&format!("Imported {n}")),
                    Err(e) => u.toast(&format!("Couldn't import: {e}")),
                }
            }
        });
    }

    fn export_item(&self, fx: bool) {
        let cur = self.current(fx);
        let suggested = if fx { format!("{cur}.json") } else { format!("{cur} EQ.json") };
        let wk = UI.with(|c| c.borrow().clone());
        file_dialog(&self.win, true, Some(&suggested), move |path| {
            if let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                match u.export_to(fx, &path) {
                    Ok(()) => u.toast(&format!("Exported {cur}")),
                    Err(e) => u.toast(&format!("Couldn't export: {e}")),
                }
            }
        });
    }

    fn ask_name(&self, title: &str, initial: &str, done: impl Fn(&Ui, String) + 'static) {
        let d = adw::AlertDialog::new(Some(title), None);
        let entry = gtk::Entry::builder().text(initial).activates_default(true).build();
        d.set_extra_child(Some(&entry));
        d.add_response("cancel", "Cancel");
        d.add_response("ok", "Save");
        d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("ok"));
        d.set_close_response("cancel");
        let wk = UI.with(|c| c.borrow().clone());
        d.connect_response(None, move |_, resp| {
            if resp == "ok"
                && let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                    done(&u, entry.text().to_string());
                }
        });
        d.present(Some(&self.win));
    }

    fn confirm_defaults(&self) {
        let d = adw::AlertDialog::new(
            Some("Restore defaults?"),
            Some("Every setting, your own profiles and your own EQ presets go back to how they started."),
        );
        d.add_response("cancel", "Cancel");
        d.add_response("ok", "Restore");
        d.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
        d.set_close_response("cancel");
        let wk = UI.with(|c| c.borrow().clone());
        d.connect_response(None, move |_, resp| {
            if resp == "ok"
                && let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                    u.restore_defaults();
                }
        });
        d.present(Some(&self.win));
    }

    pub fn restore_defaults(&self) {
        let mut d = Settings::default();
        d.sanitise();
        let a = d.activity.clone();
        d.set_activity(&a);
        *self.s.borrow_mut() = d;
        self.save_now();
        self.changed(part::ALL);
        self.send(Job::Outputs(Box::new(self.s.borrow().clone())));
        self.refresh();
        self.toast("Defaults restored");
    }

    // ---- output

    fn ask_switch_output(&self) {
        let to_hp = !self.s.borrow().headphones();
        let d = adw::AlertDialog::new(
            Some(if to_hp { "Switch to headphones?" } else { "Switch to speakers?" }),
            Some(if to_hp {
                "Sound moves to the headphone socket and the speakers go quiet."
            } else {
                "Sound moves to the speakers and the headphones go quiet."
            }),
        );
        d.add_response("cancel", "Cancel");
        d.add_response("switch", if to_hp { "Switch to headphones" } else { "Switch to speakers" });
        d.set_response_appearance("switch", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let wk = UI.with(|c| c.borrow().clone());
        d.connect_response(None, move |_, resp| {
            if resp == "switch"
                && let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                    u.set_output(if to_hp { "headphones" } else { "speakers" });
                }
        });
        d.present(Some(&self.win));
    }

    pub fn set_output(&self, out: &str) {
        if self.s.borrow().output == out {
            return;
        }
        self.s.borrow_mut().output = out.into();
        self.changed(part::ALL | part::REFRESH);
        self.send(Job::Outputs(Box::new(self.s.borrow().clone())));
        self.refresh();
    }

    // ---- fixes

    fn fix_sound(&self) {
        self.send(Job::Apply(Box::new(self.s.borrow().clone()), part::ALL | part::REFRESH));
        self.toast("Every setting was sent to the card again. Still fuzzy? Settings › Reset the sound card.");
    }

    fn ask_reset_card(&self) {
        let d = adw::AlertDialog::new(Some("Reset the sound card?"),
            Some("Restarts the card without restarting the PC. Sound stops for a few seconds."));
        d.add_response("cancel", "Cancel");
        d.add_response("reset", "Reset");
        d.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        d.set_close_response("cancel");
        let wk = UI.with(|c| c.borrow().clone());
        d.connect_response(None, move |_, resp| {
            if resp == "reset"
                && let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) {
                    u.send(Job::ResetCard);
                }
        });
        d.present(Some(&self.win));
    }

    // ---- what's playing

    /// What's playing changed: per-game memory, then switch (or offer to).
    pub fn on_detected(&self, d: Option<crate::detect::Detection>, fresh: bool) {
        let Some(d) = d else {
            *self.playing_source.borrow_mut() = None;
            return;
        };
        *self.playing_source.borrow_mut() = Some(d.source.clone());
        let mode = self.s.borrow().auto_mode.clone();
        if !fresh || mode == "off" {
            return;
        }
        // Per-game memory: put back what you last used with this game or app.
        if mode == "auto" && self.s.borrow().remember_apps && self.s.borrow().app_memory.contains_key(&d.source) {
            let before = self.s.borrow().clone();
            if self.s.borrow_mut().recall_app(&d.source) {
                *self.undo_snapshot.borrow_mut() = Some(before);
                self.changed(part::FX | part::EQ | part::SPEAKER);
                self.refresh();
                let (p, e) = { let s = self.s.borrow(); (s.cur_profile(), s.cur_eq()) };
                self.add_toast(adw::Toast::builder().use_markup(false)
                    .title(format!("{}: your settings for it (SBX {p}, EQ {e})", d.source))
                    .button_label("Undo").action_name("win.undo-switch").timeout(6).build());
                return;
            }
        }
        if self.s.borrow().activity == d.activity {
            return;
        }
        let label = model::activity_label(d.activity);
        if mode == "auto" {
            *self.undo_snapshot.borrow_mut() = Some(self.s.borrow().clone());
            self.set_activity(d.activity);
            self.add_toast(adw::Toast::builder().use_markup(false)
                .title(format!("{} is playing: switched to {label}", d.source))
                .button_label("Undo").action_name("win.undo-switch").timeout(6).build());
        } else {
            self.add_toast(adw::Toast::builder().use_markup(false)
                .title(format!("{} is playing. Switch to {label}?", d.source))
                .button_label("Switch").action_name("win.use-activity")
                .action_target(&d.activity.to_variant()).timeout(10).build());
        }
    }

    fn show_now_playing(&self) {
        self.update_now_playing();
    }

    /// A new song: set (or suggest) the EQ preset its genre matches.
    pub fn on_genre(&self, pick: Option<crate::genre::EqPick>, song: String, fresh: bool) {
        *self.song.borrow_mut() = Some(song.clone());
        *self.song_pick.borrow_mut() = pick.clone();
        self.show_now_playing();
        let Some(p) = pick else { return };
        let (mode, activity, cur) = {
            let s = self.s.borrow();
            (s.auto_mode.clone(), s.activity.clone(), s.cur_eq())
        };
        if !fresh || mode == "off" || activity != "music" || cur == p.preset
            || self.eq_override.borrow().as_deref() == Some(song.as_str()) {
            return;
        }
        let why = {
            let mut c = p.why.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        };
        if mode == "auto" {
            *self.eq_undo.borrow_mut() = Some(cur);
            self.apply_eq(p.preset);
            self.add_toast(adw::Toast::builder().use_markup(false)
                .title(format!("{why}: equaliser set to {}", p.preset))
                .button_label("Undo")
                .action_name("win.undo-eq")
                .timeout(5)
                .build());
        } else {
            self.add_toast(adw::Toast::builder().use_markup(false)
                .title(format!("{why}: set the equaliser to {}?", p.preset))
                .button_label("Set")
                .action_name("win.use-eq")
                .action_target(&p.preset.to_variant())
                .timeout(8)
                .build());
        }
    }

    /// Puts back the settings from before the last automatic switch.
    pub fn undo_switch(&self) {
        let Some(prev) = self.undo_snapshot.borrow_mut().take() else { return };
        *self.s.borrow_mut() = prev;
        self.changed(part::FX | part::EQ | part::SPEAKER);
        self.refresh();
    }

    // ---- now playing, signal path, meters

    pub fn on_now_playing(&self, np: Option<NowPlaying>) {
        // Keep the last item for 15 s after everything stops, so a quick
        // pause or a short gap doesn't make the capsule flicker away.
        if np.is_none() && self.np.borrow().is_some() {
            let since = self.np_gone.get().unwrap_or_else(Instant::now);
            self.np_gone.set(Some(since));
            if since.elapsed() < Duration::from_secs(15) {
                if let Some(n) = self.np.borrow_mut().as_mut() && n.playing {
                    n.playing = false;
                    n.position = n.position.map(|p| p + self.np_at.get().elapsed().as_secs_f64());
                }
                self.np_at.set(Instant::now());
                self.update_now_playing();
                return;
            }
        }
        if np.is_some() {
            self.np_gone.set(None);
        }
        let new_track = self.np.borrow().as_ref().map(NowPlaying::key) != np.as_ref().map(NowPlaying::key);
        let art_changed = self.np.borrow().as_ref().map(|n| n.art.clone()) != np.as_ref().map(|n| n.art.clone());
        *self.np.borrow_mut() = np;
        self.np_at.set(Instant::now());
        if new_track || art_changed {
            let art = self.np.borrow().as_ref().and_then(|n| n.art.clone());
            match art {
                Some(p) => match gtk::gdk::Texture::from_filename(&p) {
                    Ok(tex) => {
                        self.cap_pic.set_paintable(Some(&tex));
                        self.cap_icon.set_visible(false);
                    }
                    Err(_) => {
                        self.cap_pic.set_paintable(None::<&gtk::gdk::Paintable>);
                        self.cap_icon.set_visible(true);
                    }
                },
                None => {
                    self.cap_pic.set_paintable(None::<&gtk::gdk::Paintable>);
                    self.cap_icon.set_visible(true);
                }
            }
        }
        self.update_now_playing();
    }

    /// Fills the title across the room and the capsule.
    pub fn update_now_playing(&self) {
        let s = self.s.borrow().clone();
        let np = self.np.borrow().clone();
        let esc = |t: &str| glib::markup_escape_text(t).to_string();
        let eq_tag = if s.sbx_active() && s.eq_enabled && (s.activity == "music" || np.as_ref().is_some_and(|n| n.activity == "music")) {
            format!("  ·  <span foreground='#ff8aa3' weight='bold'>EQ {}</span>", esc(&s.cur_eq()))
        } else { String::new() };
        match &np {
            Some(n) => {
                self.np_kick.set_label(&format!("{}  ·  {}", n.kind.to_uppercase(), n.app.to_uppercase()));
                self.np_title.set_label(&n.title);
                let mut sub: Vec<String> = Vec::new();
                if !n.artist.is_empty() { sub.push(esc(&n.artist)); }
                if !n.album.is_empty() { sub.push(esc(&n.album)); }
                if sub.is_empty() && !n.extra.is_empty() { sub.push(esc(&n.extra)); }
                self.np_sub.set_markup(&format!("{}{eq_tag}", sub.join(" · ")));
                self.cap_title.set_label(&n.title);
                self.cap_artist.set_label(&if n.artist.is_empty() { n.app.clone() } else { n.artist.clone() });
                self.cap_icon.set_icon_name(Some(match n.activity { "game" => "input-gaming-symbolic", "movie" => "video-x-generic-symbolic", _ => "audio-x-generic-symbolic" }));
                self.cap_cover.set_css_classes(if n.activity == "music" && n.playing { &["cover", "spin"] } else { &["cover"] });
                self.capsule.set_visible(true);
            }
            None => {
                self.np_kick.set_label("NOTHING PLAYING");
                self.np_title.set_label(model::activity_label(&s.activity));
                let sound = if s.sbx_active() { format!("SBX {}", esc(&s.cur_profile())) } else { "Pure".into() };
                self.np_sub.set_markup(&format!("{sound}{eq_tag}"));
                self.capsule.set_visible(false);
            }
        }
        self.update_path();
        self.update_progress();
        self.room.queue_draw();
    }

    fn update_progress(&self) {
        let np = self.np.borrow();
        let (frac, text) = match np.as_ref() {
            Some(n) => match (n.length, n.position) {
                (Some(len), Some(pos)) => {
                    let p = if n.playing { pos + self.np_at.get().elapsed().as_secs_f64() } else { pos }.min(len);
                    (Some(p / len), if n.playing { format!("{} / {}", mmss(p), mmss(len)) } else { format!("Paused · {}", mmss(p)) })
                }
                _ => (None, if n.playing { "Playing".into() } else { "Paused".into() }),
            },
            None => (None, String::new()),
        };
        self.np_prog.set_visible(frac.is_some());
        self.cap_prog.set_visible(frac.is_some());
        if let Some(f) = frac {
            self.np_prog.set_fraction(f);
            self.cap_prog.set_fraction(f);
        }
        self.cap_time.set_label(&text);
    }

    /// Each step the sound takes: (kind, name, note, detail).
    pub fn signal_path(&self) -> Vec<(&'static str, String, &'static str, String)> {
        let s = self.s.borrow();
        let np = self.np.borrow();
        let st = np.as_ref().and_then(|n| n.stream.clone());
        let app = np.as_ref().map(|n| n.app.clone()).unwrap_or_else(|| "App".into());
        let hp = s.headphones();
        let out_ch = if hp { 2 } else if s.listen == "21" { 3 } else { model::channels(&s.speaker_config).len() };
        let (src_ch, src_rate) = st.as_ref().map_or((2, 0), |f| (f.channels, f.rate));
        let resample = src_rate != 0 && src_rate != CARD_RATE;
        let mixdown = src_ch > out_ch;
        let src_detail = st.as_ref().map_or("Format not reported".into(), |f| {
            [f.layout(), f.rate_text(), f.depth_text()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")
        });
        let card_rate = crate::media::StreamFmt { rate: CARD_RATE, ..Default::default() }.rate_text();
        let pw = [
            if resample { format!("Resampled {} → {card_rate}", st.as_ref().map(|f| f.rate_text()).unwrap_or_default()) } else { "No resampling".into() },
            if mixdown { format!("Mixed {} → {}", st.as_ref().map(|f| f.layout()).unwrap_or_default(), if hp { "stereo".into() } else if s.listen == "21" { "2.1".to_string() } else { format!("{out_ch} ch") }) }
            else if src_ch <= 2 && s.spread_mode() == "rear" { "Stereo copied to the rear speakers (3 dB quieter)".into() }
            else if src_ch <= 2 && s.spread_mode() == "all" { "Stereo spread to the centre and rear speakers".into() }
            else if src_ch <= 2 { "Stereo stays on the front speakers".into() } else { "Each channel to its own speaker".into() },
        ].join(" · ");
        let p = s.fx_of(&s.cur_profile());
        let on: Vec<&str> = [("Surround", p.surround.0), ("Crystalizer", p.crystalizer.0), ("Bass", p.bass.0),
                             ("Smart Volume", p.smart_volume.0), ("Dialog+", p.dialog.0)]
            .into_iter().filter(|(_, o)| *o).map(|(n, _)| n).collect();
        let dsp = if s.sbx_active() {
            let mut d = format!("SBX {}", s.cur_profile());
            if !on.is_empty() { d += &format!(" ({})", on.join(", ")); }
            if s.scout { d += " · Scout"; }
            if s.eq_enabled { d += &format!(" · EQ {}", s.cur_eq()); }
            d
        } else { "Pure: effects and EQ bypassed".into() };
        let speakers = if hp { "Headphones".to_string() }
            else if s.listen == "21" { "Front left · Front right · Subwoofer".into() }
            else if src_ch <= 2 && s.spread_mode() == "rear" { "Fronts, rears and subwoofer (centre silent)".into() }
            else if src_ch <= 2 && s.spread_mode() == "all" { "All six speakers".into() }
            else if src_ch <= 2 { "Front left · Front right · Subwoofer (centre and rears silent)".into() }
            else { "All six speakers".into() };
        vec![
            ("same", app, "What the app sends", src_detail),
            (if resample || mixdown || (src_ch <= 2 && s.spreads()) { "changed" } else { "same" }, "PipeWire".into(), "Linux sound server", pw),
            (if s.sbx_active() { "enhanced" } else { "same" }, "Sound Blaster ZxR".into(), "The card's DSP", dsp),
            ("same", if hp { "Headphone amp".into() } else { format!("Analog {} out", s.speaker_config) }, if hp { "Front panel" } else { "Your speakers" }, speakers),
        ]
    }

    fn update_path(&self) {
        let path = self.signal_path();
        *self.path_kinds.borrow_mut() = path.iter().map(|p| p.0).collect();
        self.path_dots.queue_draw();
        let s = self.s.borrow();
        let np = self.np.borrow();
        let st = np.as_ref().and_then(|n| n.stream.clone());
        let r = st.as_ref().map(|f| f.rate_text().replace(" kHz", "k")).unwrap_or_default();
        let lay = if s.headphones() { "HP".to_string() } else if s.listen == "21" { "2.1".into() }
                  else if st.as_ref().is_some_and(|f| f.channels <= 2) {
                      match s.spread_mode() { "rear" => "2.0 + rears".into(), "all" => "2.0 → all".into(), _ => "2.0".into() }
                  } else { "5.1".into() };
        let rate = if r.is_empty() || r == "48k" { r } else { format!("{r} → 48k") };
        let sound = if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "Pure".into() };
        self.path_label.set_label(&[rate, sound, lay].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "));
        while let Some(c) = self.path_box.first_child() {
            self.path_box.remove(&c);
        }
        let head = gtk::Box::builder().margin_bottom(10).build();
        head.append(&label("Signal path", &["path-head"]));
        self.path_box.append(&head);
        let n = path.len();
        for (i, (kind, name, note, detail)) in path.into_iter().enumerate() {
            let row = gtk::Box::builder().spacing(12).build();
            let dot_col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(12).build();
            let dot = gtk::Box::builder().css_classes(["pdot", kind]).halign(gtk::Align::Center).margin_top(5).build();
            dot_col.append(&dot);
            if i + 1 < n {
                dot_col.append(&gtk::Box::builder().css_classes(["pline"]).halign(gtk::Align::Center).vexpand(true).margin_top(4).build());
            }
            let txt = gtk::Box::builder().orientation(gtk::Orientation::Vertical).margin_bottom(12).build();
            let nm = gtk::Label::new(None);
            nm.set_xalign(0.0);
            nm.set_markup(&format!("<b>{}</b>  <span alpha='45%' size='small'>{}</span>", glib::markup_escape_text(&name), note));
            let dt = label(&detail, &["path-detail"]);
            dt.set_wrap(true);
            txt.append(&nm);
            txt.append(&dt);
            row.append(&dot_col);
            row.append(&txt);
            self.path_box.append(&row);
        }
        let leg = gtk::Box::builder().spacing(14).css_classes(["path-legend"]).build();
        for (k, t) in [("same", "Unchanged"), ("changed", "Changed"), ("enhanced", "Effects")] {
            let b = gtk::Box::builder().spacing(6).build();
            b.append(&gtk::Box::builder().css_classes(["pdot", k]).valign(gtk::Align::Center).build());
            b.append(&label(t, &["dim", "small"]));
            leg.append(&b);
        }
        self.path_box.append(&leg);
    }

    fn start_meters(&self) {
        if self.meter.borrow().is_some() {
            return;
        }
        let (tx, rx) = async_channel::bounded::<[f32; 6]>(4);
        *self.meter.borrow_mut() = crate::meters::start(move |l| {
            let _ = tx.try_send(l);
        });
        let wk = UI.with(|c| c.borrow().clone());
        glib::spawn_future_local(async move {
            while let Ok(l) = rx.recv().await {
                let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) else { break };
                u.levels.set(l);
                for (_, m) in u.mix_meters.borrow().iter() {
                    m.queue_draw();
                }
            }
        });
    }

    fn room_state(&self) -> room::RoomState {
        let s = self.s.borrow();
        let hp = s.headphones();
        let cfg = model::channels(&s.speaker_config);
        // The room lists the speakers in its own order (FL, FC, FR, LFE, RL, RR).
        let plays = room::SPEAKERS.map(|(c, _, _)| !hp && cfg.contains(&c) && (s.listen != "21" || matches!(c, "FL" | "FR" | "LFE")));
        let lv = self.levels.get();
        let levels: [f32; 6] = std::array::from_fn(|i| if plays[i] { lv[ch_index(room::SPEAKERS[i].0)] } else { 0.0 });
        let top = if self.np_head.is_visible() { self.np_head.height() as f64 + 40.0 } else { 30.0 };
        room::RoomState {
            plays, levels, lit: self.room_lit.borrow().clone(), headphones: hp,
            time: self.t0.elapsed().as_secs_f64(), top: top.max(120.0),
            bottom: if self.capsule.is_visible() { 96.0 } else { 30.0 },
            off_note: if hp { "headphones on" } else if s.listen == "21" { "off in 2.1" } else { "not connected" },
        }
    }

    // ---- speakers

    /// Plays a test sound on one speaker and lights it up in the room.
    pub fn test_speaker(&self, ch: &str) {
        self.send(Job::Test(Box::new(self.s.borrow().clone()), ch.to_string()));
        *self.room_lit.borrow_mut() = Some(ch.to_string());
        self.room.queue_draw();
        let wk = UI.with(|c| c.borrow().clone());
        glib::timeout_add_local_once(Duration::from_millis(1500), move || {
            if let Some(u) = wk.and_then(|w| w.upgrade()) {
                *u.room_lit.borrow_mut() = None;
                u.room.queue_draw();
            }
        });
    }

    /// Each speaker in turn.
    pub fn test_all_speakers(&self) {
        let chans: Vec<String> = Self::channels_now(&self.s.borrow()).iter().map(|c| c.to_string()).collect();
        for (i, ch) in chans.into_iter().enumerate() {
            let wk = UI.with(|c| c.borrow().clone());
            glib::timeout_add_local_once(Duration::from_millis(1800 * i as u64), move || {
                if let Some(u) = wk.and_then(|w| w.upgrade()) {
                    u.test_speaker(&ch);
                }
            });
        }
    }

    fn channels_now(s: &Settings) -> &'static [&'static str] {
        if s.headphones() { &["FL", "FR"] } else { model::channels(&s.speaker_config) }
    }

    fn rebuild_test_rows(&self, s: &Settings) {
        let layout = format!("{}-{}", s.output, s.speaker_config);
        if *self.test_layout.borrow() == layout {
            return;
        }
        for (_, row, _) in self.test_rows.borrow_mut().drain(..) {
            self.test_group.remove(&row);
        }
        let wk = UI.with(|c| c.borrow().clone());
        for ch in Self::channels_now(s) {
            let row = adw::ActionRow::builder().title(model::channel_name(ch)).build();
            let b = gtk::Button::builder().icon_name("media-playback-start-symbolic").tooltip_text("Play a test sound")
                .valign(gtk::Align::Center).css_classes(["circular"]).build();
            row.add_prefix(&b);
            let c = ch.to_string();
            let wk2 = wk.clone();
            b.connect_clicked(move |_| {
                if let Some(u) = wk2.as_ref().and_then(|w| w.upgrade()) {
                    u.test_speaker(&c);
                }
            });
            self.test_group.add(&row);
            self.test_rows.borrow_mut().push((ch.to_string(), row, b));
        }
        *self.test_layout.borrow_mut() = layout;
    }

    /// Mixer: each speaker's level, with its live meter.
    fn rebuild_mix_rows(&self, s: &Settings) {
        let layout = format!("{}-{}", s.output, s.speaker_config);
        if *self.mix_layout.borrow() == layout {
            for (ch, _, sc) in self.mix_rows.borrow().iter() {
                sc.set_value(s.trim.get(ch).copied().unwrap_or(0.0));
            }
            return;
        }
        for (_, col, _) in self.mix_rows.borrow_mut().drain(..) {
            self.mix_box.remove(&col);
        }
        self.mix_meters.borrow_mut().clear();
        let wk = UI.with(|c| c.borrow().clone());
        for ch in Self::channels_now(s) {
            let m = meter_area(ch_index(ch));
            let (col, sc) = strip(model::channel_name(ch), -20.0, 0.0, 0.5, fmt_db, Some(&m));
            sc.set_value(s.trim.get(*ch).copied().unwrap_or(0.0));
            sc.set_sensitive(!s.headphones());
            let c = ch.to_string();
            let wk2 = wk.clone();
            sc.connect_value_changed(move |sc| {
                if let Some(u) = wk2.as_ref().and_then(|w| w.upgrade()) {
                    if u.loading.get() {
                        return;
                    }
                    u.s.borrow_mut().trim.insert(c.clone(), (sc.value() * 2.0).round() / 2.0);
                    u.changed(part::VOLUMES);
                }
            });
            self.mix_box.append(&col);
            self.mix_rows.borrow_mut().push((ch.to_string(), col, sc));
            self.mix_meters.borrow_mut().push((ch_index(ch), m));
        }
        *self.mix_layout.borrow_mut() = layout;
    }

    // ---- side panels

    /// "home" when no side panel is open, else "mixer", "crystalvoice", "setup" or "settings".
    pub fn current_page(&self) -> String {
        if self.split.shows_sidebar() {
            self.side_stack.visible_child_name().map(|n| n.to_string()).unwrap_or_default()
        } else {
            "home".into()
        }
    }

    pub fn show_page(&self, name: &str) {
        let id = match name {
            "mixer" | "mix" => "mixer",
            "crystalvoice" | "mic" | "recording" => "crystalvoice",
            "setup" | "speakers" | "playback" => "setup",
            "settings" => "settings",
            _ => {
                self.split.set_show_sidebar(false);
                return;
            }
        };
        self.side_stack.set_visible_child_name(id);
        self.side_title.set_label(match id { "mixer" => "Mixer", "crystalvoice" => "CrystalVoice", "setup" => "Speaker setup", _ => "Settings" });
        self.split.set_show_sidebar(true);
        let was = self.loading.replace(true);
        for (t, b) in &self.tool_btns {
            b.set_active(*t == id);
        }
        self.loading.set(was);
    }

    // ---- fill the widgets from the settings

    pub fn refresh(&self) {
        let was = self.loading.replace(true);
        let s = self.s.borrow().clone();
        let sbx = s.sbx_active();
        let hp = s.headphones();

        for (id, b) in &self.act_btns {
            b.set_active(*id == s.activity);
        }
        // Output card
        self.out_label.set_label(if hp { "HEADPHONES" } else { "SPEAKERS" });
        self.vol_label.set_label(&format!("{}%", self.vol_adj.value().round() as i64));
        self.sbx_btn.set_active(s.sbx_enabled);
        self.pure_btn.set_active(!s.sbx_enabled);
        for (id, b) in &self.listen_btns {
            b.set_active(*id == s.listen);
            b.set_sensitive(!hp && s.has_sub());
        }
        for (id, b) in &self.stereo_btns {
            b.set_active(*id == s.stereo_on);
        }
        let can_spread = !hp && s.listen != "21" && model::channels(&s.speaker_config).contains(&"RL");
        self.stereo_row.set_sensitive(can_spread);
        self.stereo_row.set_tooltip_text(if can_spread { None } else if hp { Some("Headphones are stereo") }
            else { Some("Only with rear speakers and 5.1 listening") });
        self.out_switch.set_child(Some(&{
            let b = gtk::Box::builder().spacing(8).build();
            b.append(&gtk::Image::from_icon_name(if hp { "audio-speakers-symbolic" } else { "audio-headphones-symbolic" }));
            b.append(&gtk::Label::new(Some(if hp { "Switch to speakers" } else { "Switch to headphones" })));
            b
        }));
        self.mode_desc.set_label(&mode_description(&s));

        // SBX profile card
        self.rebuild_chips(&s);
        self.scout_box.set_visible(s.activity == "game");
        self.scout_sw.set_active(s.game_footsteps);
        self.profile_card.set_sensitive(true);
        let p = s.fx_of(&s.cur_profile());
        for r in &self.fx {
            let (on, lvl) = match r.key {
                "surround" => p.surround,
                "crystalizer" => p.crystalizer,
                "bass" => p.bass,
                "smart_volume" => (p.smart_volume.0, p.smart_volume.1),
                _ => p.dialog,
            };
            r.on.set_active(on);
            r.level.set_value(lvl as f64);
            if let Some(x) = &r.xover {
                x.set_value(p.bass_xover as f64);
            }
            if let Some(m) = &r.mode {
                m.set_selected(SVM_MODES.iter().position(|x| *x == p.smart_volume.2).unwrap_or(0) as u32);
            }
        }
        self.chips.set_sensitive(sbx);
        self.fx_values();

        // Equaliser card
        self.eq_on.set_active(s.eq_enabled);
        let enames = s.eq_list();
        let epos = enames.iter().position(|n| *n == s.cur_eq()).unwrap_or(0) as u32;
        set_strings(&self.eq_preset, &self.eq_names, enames);
        self.eq_preset.set_selected(epos);
        for (sc, v) in self.eq_bands.iter().zip(s.eq_of(&s.cur_eq())) {
            sc.set_value(v as f64);
        }
        self.eq_card.set_sensitive(sbx);
        self.eq_card.set_tooltip_text(if sbx { None } else { Some("Pure: the equaliser is bypassed") });

        // Mixer
        self.rebuild_mix_rows(&s);
        self.boost.set_value(s.boost_db);
        self.boost.set_sensitive(!hp);
        self.mic_level.set_value(s.rec.mic_level.unwrap_or(9) as f64);
        self.wuh_level.set_value(s.rec.wuh_level.unwrap_or(0) as f64);

        // CrystalVoice
        let r = &s.rec;
        self.cv_on.set_active(r.voice_clarity);
        self.nr.set_active(r.noise_reduction);
        self.msvm.set_active(r.smart_volume);
        self.msvm_level.set_value(r.svm_level as f64);
        self.focus.set_active(r.focus);
        self.focus_angle.set_value(r.focus_angle as f64);
        self.morph.set_selected(VOICE_MORPHS.iter().position(|x| *x == r.morph).unwrap_or(0) as u32);
        self.mic_boost.set_selected(MIC_BOOST.iter().position(|x| *x == r.mic_boost).unwrap_or(0) as u32);
        self.wuh.set_active(r.what_u_hear);
        self.cv_sensitivity();

        // Speaker setup
        self.cfg.set_selected(SPEAKER_CONFIGS.iter().position(|c| *c == s.speaker_config).unwrap_or(4) as u32);
        self.spk_type.set_selected(SPEAKER_TYPES.iter().position(|c| *c == s.speaker_type).unwrap_or(0) as u32);
        self.fr_front.set_active(s.full_range_front);
        self.fr_rear.set_active(s.full_range_rear);
        self.bass_redir.set_active(s.bass_redirect);
        self.xover.set_value(s.crossover as f64);
        self.bass_redir.set_sensitive(s.has_sub() && !hp);
        self.xover_row.set_sensitive(s.has_sub() && s.bass_redirect && !hp);
        self.hp_gain.set_selected(u32::from(s.hp_600ohm));
        self.rebuild_test_rows(&s);

        // Settings
        self.auto_combo.set_selected(model::AUTO_MODES.iter().position(|m| *m == s.auto_mode).unwrap_or(0) as u32);
        self.remember_switch.set_active(s.remember_apps);
        self.genre_switch.set_active(s.genre_online);
        self.tray_switch.set_active(s.tray);

        self.update_item_buttons();
        self.loading.set(was);
        self.refresh_summary();
    }

    pub fn refresh_summary(&self) {
        let s = self.s.borrow().clone();
        self.boost_label.set_label(&if s.boost_db > 0.0 {
            format!("Speaker boost +{} dB: louder, like Windows (which ran the card at +9). Lower it if loud sound gets fuzzy.", s.boost_db)
        } else {
            "Speaker boost raises the card's output above the safe 0 dB, like Windows did (+9). Lower it if loud sound gets fuzzy.".into()
        });
        // The EQ graph: the dashed line is what SBX Bass adds through it (5.1/2.1).
        *self.eq_view.boost.borrow_mut() = model::bass_boost(&s);
        self.eq_view.live.set(model::effective_eq(&s).1);
        self.eq_view.area.queue_draw();
        self.update_now_playing();
        self.update_status();
    }

    pub fn update_status(&self) {
        self.banner.set_revealed(!self.card_ok.get());
        let s = self.s.borrow();
        let sound = if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "Pure".into() };
        self.win.set_title(Some(&format!("Sound Blaster Command — {} · {sound}", model::activity_label(&s.activity))));
    }
}

/// One sentence about what SBX/Pure and 5.1/2.1 mean right now.
pub fn mode_description(s: &Settings) -> String {
    let p = s.cur_profile();
    if s.headphones() {
        return if s.sbx_active() { format!("Headphones: stereo, with SBX {p}.") } else { "Headphones: stereo, exactly as mixed.".into() };
    }
    let stereo = match s.spread_mode() {
        "rear" => " Stereo also plays on the rear speakers.",
        "all" => " Stereo plays on all speakers.",
        _ => "",
    };
    let d = match (s.sbx_active(), s.listen == "21") {
        (true, false) => format!("SBX {p} on all six speakers. Films and games play each sound from its own speaker."),
        (true, true) => format!("SBX {p} on the front speakers and subwoofer only."),
        (false, false) if s.spreads() => "Pure 5.1: no effects, no equaliser.".into(),
        (false, false) => "Pure 5.1: every channel exactly as mixed. No effects, no equaliser.".into(),
        (false, true) => "Pure 2.1: front speakers and subwoofer only, exactly as mixed. No effects, no equaliser.".into(),
    };
    format!("{d}{stereo}")
}

thread_local! {
    /// The window's UI, for callbacks that can't capture an Rc directly.
    pub static UI: RefCell<Option<Weak<Ui>>> = const { RefCell::new(None) };
}

fn file_dialog(win: &adw::ApplicationWindow, save: bool, name: Option<&str>, done: impl FnOnce(std::path::PathBuf) + 'static) {
    let fd = gtk::FileDialog::builder().title(if save { "Export" } else { "Import" }).build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON files"));
    filter.add_pattern("*.json");
    let store = gio::ListStore::new::<gtk::FileFilter>();
    store.append(&filter);
    fd.set_filters(Some(&store));
    if let Some(n) = name {
        fd.set_initial_name(Some(n));
    }
    let cb = move |res: Result<gio::File, glib::Error>| {
        if let Some(p) = res.ok().and_then(|f| f.path()) {
            done(p);
        }
    };
    if save {
        fd.save(Some(win), gio::Cancellable::NONE, cb);
    } else {
        fd.open(Some(win), gio::Cancellable::NONE, cb);
    }
}
