//! The page: what's playing, full window. Layers, back to front:
//! the background picture (printed in the page red, pictures take turns
//! slowly), a shade so words stay readable, the room (drawn by a theme), the
//! title block at the top left, the big number or the live venue at the top
//! right, the cover at the bottom left, and along the bottom the sound
//! choices (slim dock, rows or the sport-style bar) and the controls dock.
//!
//! Everything here is display: choices are reported through `on_pick` and
//! controls through `on_action`, and ui.rs does the work.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

pub const CSS: &str = r#"
.stage { background: #0b0b0c; }
.stage .shade { background: linear-gradient(to right, alpha(black,0.94) 0%, alpha(black,0.77) 36%, alpha(black,0.19) 70%, transparent 100%); }
.stage.split .shade { background: linear-gradient(to right, alpha(black,0.35) 0%, alpha(black,0.2) 40%, alpha(black,0.1) 70%, transparent 100%); }
.stage .splitart { box-shadow: 2px 0 0 alpha(#e3262f,0.55); }
.stage .kick { font-family: "Archivo Narrow"; font-weight: 700; font-size: 13px; letter-spacing: 0.78px; color: #ff4b53; }
.stage .title { font-family: "Anton"; font-size: 92px; line-height: 0.95; color: white; text-shadow: 0 4px 40px alpha(black,0.73); }
.stage .title.long { font-size: 60px; line-height: 1.04; }
.stage .artist { font-family: "Anton"; font-size: 40px; line-height: 1.1; color: #ffd0d3; }
.stage .cred-l { font-family: "Archivo Narrow"; font-weight: 700; font-size: 10.5px; letter-spacing: 1.47px; color: #ff4b53; }
.stage .cred-v { font-family: "Archivo"; font-weight: 600; font-size: 14px; color: white; }
.stage .credits { border-top: 1px solid alpha(white,0.165); padding-top: 10px; margin-top: 18px; }
.stage .hline { font-family: "Archivo"; font-size: 15px; color: alpha(white,0.66); }
.stage .bio { font-family: "Archivo"; font-size: 15px; line-height: 24px; color: alpha(white,0.847); }
.stage .facts { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12px; letter-spacing: 1.2px; color: #ff4b53; }
.stage .sleeve { box-shadow: 0 22px 50px alpha(black,0.8), 0 0 0 1px alpha(white,0.36); }
.stage .fsbtn { border-radius: 999px; padding: 6px 14px 6px 10px; border: 1px solid alpha(white,0.4); background: alpha(black,0.3); min-height: 0; }
.stage .fsbtn label { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11.5px; letter-spacing: 1.2px; }
.stage .fsbtn:hover { border-color: #e3262f; background: alpha(#e3262f,0.2); }
.stage .bignum { font-family: "Anton"; font-size: 118px; line-height: 0.82; color: white; text-shadow: 0 4px 30px alpha(black,0.7); }
.stage .bigspk { font-family: "Archivo Narrow"; font-weight: 700; font-size: 13px; letter-spacing: 1.5px; color: alpha(white,0.72); }
.stage .bigspk-n { font-family: "Anton"; font-size: 26px; color: #ff4b53; }
.stage .venue { background: alpha(#050505,0.92); border-radius: 4px; padding: 6px 14px 6px 6px; box-shadow: 0 10px 30px alpha(black,0.5); }
.stage .venue .rp { background: #e3262f; font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.6px; padding: 5px 9px; border-radius: 2px; }
.stage .venue .vn { font-family: "Anton"; font-size: 22px; }
.stage .venue .vd { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.1px; color: alpha(white,0.66); }
.stage .bigart { box-shadow: 0 22px 50px alpha(black,0.85), 0 0 0 1px alpha(white,0.36); }
.stage .dock { background: linear-gradient(to top, alpha(black,0.92) 0%, alpha(black,0.69) 50%, transparent 100%); padding: 40px 46px 20px 46px; }
.stage .soundtag { background: #e3262f; border-radius: 3px; padding: 5px 10px; margin-right: 4px; font-family: "Anton"; font-size: 18px; color: white; }
.stage .chev, popover .chev { color: #ff4b53; }
.stage .f-l { font-family: "Archivo Narrow"; font-weight: 700; font-size: 10.5px; letter-spacing: 1.4px; color: #ff4b53; }
.stage .f-v button { border: 2px solid alpha(white,0.34); border-radius: 4px; background: alpha(black,0.33); padding: 1px 6px 1px 7px; min-height: 0; box-shadow: none; }
.stage .f-v button:hover { border-color: #e3262f; background: alpha(#e3262f,0.12); }
.stage .f-v button:checked { border-color: #e3262f; background: alpha(#e3262f,0.18); }
.stage .f-v label.v { font-family: "Anton"; font-weight: normal; font-size: 19px; color: white; }
.stage .f-v.fixed button { border-color: transparent; background: none; }
.stage .chip > button { border: 1px dashed alpha(white,0.45); border-radius: 999px; padding: 3px 11px; background: alpha(black,0.4); min-height: 0; box-shadow: none; }
.stage .chip > button:hover { border-color: #e3262f; }
.stage .chip label { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12.5px; }
.stage .amenu button { padding: 7px 12px; border-radius: 8px; }
.stage .amenu button:hover, popover .amenu button:hover { background: #e3262f; }
.mi-t { font-weight: 700; font-size: 13px; } .mi-s { font-size: 11.5px; color: alpha(white,0.55); }
/* Look and feel panel */
.lf-h { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12px; letter-spacing: 1.4px; color: alpha(white,0.62); margin-top: 8px; }
.lf-card { padding: 6px; border-radius: 10px; background: alpha(white,0.04); border: 1px solid alpha(white,0.08); }
.lf-card:hover { border-color: alpha(#e3262f,0.7); }
.lf-card.on { border-color: #e3262f; box-shadow: inset 0 0 0 1px #e3262f; background: alpha(#e3262f,0.10); }
.lf-n { font-weight: 700; font-size: 12px; }
.lf-save { background: #e3262f; color: white; border-radius: 999px; padding: 6px 14px; font-weight: 700; }
.lf-back { border-radius: 999px; padding: 6px 14px; border: 1px solid alpha(white,0.35); background: none; font-weight: 700; }
.lf-hint { font-size: 12px; color: alpha(white,0.55); }
.lf-def { padding: 6px 9px; border-radius: 8px; background: alpha(white,0.04); }
.lf-def.now { background: alpha(#e3262f,0.15); box-shadow: inset 0 0 0 1px alpha(#e3262f,0.6); }
.lf-dk { font-size: 11px; color: alpha(white,0.55); } .lf-dv { font-weight: 700; font-size: 12.5px; }
.lf-link { background: none; color: #ff8a8f; font-weight: 700; padding: 2px 0; }
/* full screen pictures and trailer */
.fullview { background: black; }
.fbar { padding: 16px 24px 14px 24px; background: linear-gradient(to bottom, alpha(black,0.85), transparent); }
.fk { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.8px; color: #ff4b53; }
.ft { font-family: "Anton"; font-size: 26px; }
.fplay { background: #e3262f; color: white; border-radius: 999px; padding: 6px 14px; font-weight: 700; }
.fback { border-radius: 999px; padding: 6px 14px; border: 1px solid alpha(white,0.45); background: alpha(black,0.3); font-weight: 700; }
.farrow { min-width: 54px; min-height: 54px; border-radius: 999px; background: alpha(black,0.6); }
.farrow:hover { background: #e3262f; }
.fctl { padding: 7px 12px; border-radius: 12px; background: alpha(black,0.72); }
/* the slim dock: one line left of the controls, a thin rule between the choices */
.stage .dline .sf { padding: 2px 13px; border-right: 1px solid alpha(white,0.15); }
.stage .dline .sf.last { border-right: none; }
.stage .dline.tight .sf { padding: 2px 8px; } .stage .dline.tight .soundtag { margin-right: 2px; }
.stage .dline .f-l { font-size: 11px; letter-spacing: 1.43px; }
/* in the dock each word stands between two grey bars (round 80), the chevron a thin red stroke */
.stage .dline .f-v button { border-style: solid; border-width: 0 2px; border-color: alpha(white,0.333); border-radius: 0; background: alpha(black,0.333); padding: 0 2px 0 3px; }
.stage .dline .f-v button:hover, .stage .dline .f-v button:checked { border-color: #e3262f; }
.stage .dline .f-v.fixed button { border-color: transparent; background: none; }
.stage .dline .chip > button { padding: 4px 11px; border-color: alpha(white,0.376); background: alpha(black,0.467); }
.stage .dline .chip label { font-size: 12px; letter-spacing: 0.24px; color: alpha(white,0.784); }
.stage .rows .fd { border-top: 1px solid alpha(white,0.16); padding: 7px 0 8px 0; }
.stage .rows .fd:first-child { border-top: 2px solid #e3262f; }
.stage .rows label.v { font-size: 28px; }
.stage .tech { font-family: "Archivo Narrow"; font-weight: 600; font-size: 12px; letter-spacing: 0.4px; color: alpha(white,0.62); }
.stage .bar { background: linear-gradient(to right, alpha(#050505,0.96), alpha(#050505,0.91) 60%, alpha(#050505,0.75)); border-top: 3px solid #e3262f; }
.stage .bar .tab { background: #e3262f; padding: 10px 18px; border-radius: 0; color: white; }
.stage .bar .tab .tb { font-family: "Anton"; font-size: 26px; }
.stage .bar .tab .ts { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11.5px; letter-spacing: 0.8px; }
.stage .bar .fd { padding: 10px 18px 11px 18px; border-right: 1px solid alpha(white,0.1); }
.stage .ctl button.round, .stage .ctl menubutton.round > button { min-width: 36px; min-height: 36px; padding: 0; border-radius: 999px; border: 1px solid alpha(white,0.16); background: alpha(black,0.4); }
.stage .ctl button.round:hover, .stage .ctl menubutton.round > button:hover { border-color: #e3262f; }
.stage .ctl button.round:checked { background: #e3262f; border-color: #e3262f; }
.stage .ctl button.pp { min-width: 44px; min-height: 44px; background: white; color: black; border: none; }
.stage .ctl button.flat-t { min-width: 36px; min-height: 36px; padding: 0; border-radius: 999px; background: none; border: none; }
.stage .ctl button.flat-t:disabled { opacity: 0.25; }
.stage .readout > button { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12px; letter-spacing: 1.2px; color: #ff4b53; padding: 6px 10px; border: 1px solid alpha(#e3262f,0.4); border-radius: 6px; background: alpha(#1a0507,0.8); min-height: 0; box-shadow: none; }
.stage .readout > button:hover { border-color: #e3262f; }
.stage .ctl scale { min-width: 100px; }
.stage .ctl scale trough { min-height: 3px; background: alpha(white,0.16); }
.stage .ctl scale highlight { background: #e3262f; }
.stage .ctl scale slider { min-width: 13px; min-height: 13px; margin: -5px; background: white; border: none; box-shadow: none; }
.stage .hint { font-size: 12px; color: alpha(white,0.44); }
.stage .tk-admit { font-family: "Anton"; font-size: 34px; color: white; letter-spacing: 1px; }
.stage .tk-sub { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11.5px; letter-spacing: 2px; color: alpha(white,0.72); }
.stage.s-s .tk-admit, .stage.short .tk-admit { font-size: 26px; }
/* sizes for smaller windows */
.stage.s-m .title { font-size: 76px; } .stage.s-m .title.long { font-size: 54px; } .stage.s-m .bignum { font-size: 96px; } .stage.s-m .artist { font-size: 34px; }
.stage.s-s .title { font-size: 58px; } .stage.s-s .title.long { font-size: 44px; } .stage.s-s .artist { font-size: 27px; } .stage.s-s .bignum { font-size: 76px; } .stage.s-s .bio { font-size: 13px; }
.stage.s-s .f-v label.v { font-size: 16px; } .stage.s-s .dock { padding: 34px 24px 14px 24px; }
.stage.short .bio, .stage.short .credits { margin-top: 0; }
.stage.short .title { font-size: 56px; } .stage.short .title.long { font-size: 42px; } .stage.short .bignum { font-size: 72px; }
/* the compact view */
.stage.compact .title { font-size: 44px; } .stage.compact .title.long { font-size: 32px; } .stage.compact .artist { font-size: 24px; }
.stage.compact .bignum { font-size: 56px; } .stage.compact .dock { padding: 24px 16px 12px 16px; }
.stage.compact .rows label.v { font-size: 20px; }
.stage.compact .shade { background: linear-gradient(to bottom, alpha(black,0.2) 0%, alpha(black,0.75) 45%, alpha(black,0.95) 100%); }
/* half a screen (round 125): each theme its own layout */
.stage.half .shade { background: linear-gradient(to bottom, alpha(black,0.9) 0%, alpha(black,0.6) 26%, alpha(black,0.45) 60%, alpha(black,0.92) 100%); }
.stage.half .tile { padding: 12px 16px; }
.stage.half .tile .f-l { font-size: 11px; letter-spacing: 1.5px; }
.stage.half .tile .f-v button, .stage.half .tile .chip > button { border: none; background: none; padding: 0; box-shadow: none; min-height: 0; }
.stage.half .tile label.v, .stage.half .tile .chip label { font-family: "Anton"; font-weight: normal; font-size: 26px; letter-spacing: 0; color: white; }
.stage.half .dock { padding: 18px 28px 22px 28px; }
.stage.half-oys .tile { background: alpha(#0b0b0d,0.8); border: 1px solid alpha(white,0.1); border-radius: 14px; }
.stage.half-oys .tile:hover { border-color: alpha(#e3262f,0.8); }
.stage.half-oys .dock { background: alpha(black,0.85); border-top: 1px solid alpha(white,0.08); padding: 20px 24px; }
.stage.half-blueprint .shade { background: alpha(#060607,0.82); }
.stage.half-blueprint .tile { border: 1.5px solid white; padding: 10px 12px; background: alpha(#060607,0.7); }
.stage.half-blueprint .tile:hover { border-color: #ff4b53; }
.stage.half-blueprint .tile label.v, .stage.half-blueprint .tile .chip label { font-size: 22px; }
.stage.half-blueprint .dock { background: none; }
.stage.half-blueprint .low { border: 1.5px solid white; padding: 10px 16px; background: alpha(#060607,0.85); }
.stage.half-screenprint .shade { background: linear-gradient(to bottom, alpha(black,0.25) 0%, alpha(black,0.1) 30%, black 62%); }
.stage.half-screenprint .hband { background: #e3262f; }
.stage.half-screenprint .kick { color: white; }
.stage.half-screenprint .artist { color: #1a0406; }
.stage.half-screenprint .title { text-shadow: none; }
.stage.half-screenprint .hline { color: alpha(white,0.86); }
.stage.half-screenprint .tile { background: #b5141c; box-shadow: 6px 6px 0 black; }
.stage.half-screenprint .tile:hover { background: #c81922; }
.stage.half-screenprint .tile .f-l { color: #ffd0d3; }
.stage.half-screenprint .tile .chev { color: white; }
.stage.half-screenprint .dock { background: black; }
.stage.half-callouts .tile { padding: 4px 0; }
.stage.half-callouts .tile label.v, .stage.half-callouts .tile .chip label { font-size: 24px; }
.stage.half-callouts .dock { background: linear-gradient(to top, alpha(black,0.92), alpha(black,0.6) 60%, transparent); padding-top: 30px; }
popover.choices button { padding: 7px 16px; border-radius: 9px; font-family: "Anton"; font-weight: normal; font-size: 20px; color: alpha(white,0.6); background: none; }
popover.choices button:hover { background: alpha(white,0.08); color: white; }
popover.choices button.cur { background: #e3262f; color: white; }
popover.choices .sub { font-family: "Archivo Narrow"; font-size: 11px; font-weight: 600; letter-spacing: 0.6px; color: alpha(white,0.55); }
popover.choices .head { font-family: "Archivo Narrow"; font-size: 11px; font-weight: 700; letter-spacing: 1.4px; color: #ff4b53; padding: 4px 16px 2px 16px; }
"#;

/// One of the four sound choices (Where, Room, Tone, Extras).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Field {
    pub key: &'static str,
    pub label: String,
    pub value: String,
    /// (id, phrase, small print). Empty: the value is fixed.
    pub options: Vec<(String, String, String)>,
    pub current: String,
    /// Shown as a dashed chip (Extras) rather than a boxed word.
    pub chip: bool,
}

/// The title block's content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Head {
    pub kick: String,
    pub title: String,
    pub subtitle: String,
    pub credits: Vec<(String, String)>,
    pub bio: String,
    pub facts: String,
    /// "Full screen" / "Watch the trailer" button text (and its small print).
    pub button: Option<(String, String)>,
    /// Films, series, games and sport: one line under the name ("2024 ·
    /// Jellyfin") in place of the credits, which only songs show.
    pub line: String,
}

/// The ticket band across the top of a film or series page ("Admit one").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ticket {
    /// Under "Admit one": the app and the screen ("JELLYFIN · SCREEN 5.1").
    pub left: String,
    /// Beside the barcode: the source ("5.1 SOUND").
    pub source: String,
    /// The outlined number: the speakers it plays on ("5.1", "HP").
    pub number: String,
    /// Picks the barcode's bars (the title).
    pub seed: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Corner {
    None,
    /// "5.1", active speakers, of how many.
    Big(String, usize, usize),
    /// Live: venue, date.
    Venue(String, String),
}

/// Where things go on half a screen: the room's box, the tiles' boxes, the
/// title's top (all page pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalfGeo {
    pub room: (f64, f64, f64, f64),
    pub tiles: [(f64, f64, f64, f64); 4],
    pub head_top: i32,
    /// Screenprint's red band behind the title: its top and height.
    pub band: Option<(f64, f64)>,
}

/// Half a screen, per theme (round 125, `half-round125.html`): Over your
/// shoulder fills the middle with the room and puts the choices two to a row
/// over the controls; Screenprint prints the room across the top with the
/// title on a red band and red blocks below; Blueprint draws the plan in the
/// middle with the choices as notes at its corners; Callouts puts the
/// choices at the corners with red lines to what they change.
/// `ch` is the controls' height at the bottom, `th` a tile's, `head_h` the
/// title block's (kicker, title, artist, the "year · app" line).
pub fn half_geo(theme: &str, w: f64, h: f64, ch: f64, th: f64, head_h: f64) -> HalfGeo {
    const M: f64 = 28.0;
    let ch = ch.max(60.0);
    let th = th.max(50.0);
    // Two by two, or (a short window) four in a row so the room keeps its height.
    let one_row = h < 760.0;
    let rows_h = |gap: f64| if one_row { th } else { 2.0 * th + gap };
    let grid = |top: f64, gap: f64| {
        if one_row {
            let tw = (w - 2.0 * M - 3.0 * gap) / 4.0;
            return std::array::from_fn(|i| (M + i as f64 * (tw + gap), top, tw, th));
        }
        let tw = (w - 2.0 * M - gap) / 2.0;
        [(M, top, tw, th), (M + tw + gap, top, tw, th), (M, top + th + gap, tw, th), (M + tw + gap, top + th + gap, tw, th)]
    };
    // Below the title block, with a little air.
    let below_title = 56.0 + head_h.max(90.0) + 20.0;
    match theme {
        "screenprint" => {
            // From the bottom up: the controls, the red blocks, the red band with
            // the title; the room takes the rest of the top.
            let top = h - ch - rows_h(12.0) - 14.0;
            // The band holds the whole title block (kicker to the "year · app" line).
            let bh = (head_h + 40.0).clamp(150.0, 240.0);
            let band = (top - 16.0 - bh, bh);
            HalfGeo { room: (0.0, 0.0, w, band.0 + 30.0), tiles: grid(top, 12.0), head_top: (band.0 + 18.0) as i32, band: Some(band) }
        }
        "blueprint" => {
            // The plan between the four notes, centred in the space under the title.
            let bottom = h - ch - 24.0;
            let rw = (w - 2.0 * (M + 200.0 + 24.0)).max(260.0);
            let rh = (bottom - below_title).min(rw * 0.86);
            let rw = rh / 0.86;
            let ry = below_title + ((bottom - below_title - rh) / 2.0).max(0.0);
            let rx = (w - rw) / 2.0;
            let tw = (rx - M - 24.0).max(150.0);
            HalfGeo { room: (rx, ry, rw, rh), tiles: [(M, ry, tw, th), (w - M - tw, ry, tw, th), (M, ry + rh - th, tw, th), (w - M - tw, ry + rh - th, tw, th)],
                      head_top: 56, band: None }
        }
        "callouts" => {
            // The choices at the room's corners: above and below it, or (a short
            // window) beside it, so the room keeps its size.
            let ry = below_title + th;
            let rh = h - ry - th - ch - 16.0;
            if rh >= h * 0.4 {
                let tw = (w - 2.0 * M) * 0.42;
                HalfGeo { room: (w * 0.04, ry, w * 0.92, rh),
                          tiles: [(M, ry - th, tw, th), (w - M - tw, ry - th, tw, th), (M, ry + rh, tw, th), (w - M - tw, ry + rh, tw, th)],
                          head_top: 56, band: None }
            } else {
                let bottom = h - ch - 12.0;
                let tw = ((w - 2.0 * M) * 0.24).max(150.0);
                let (rx, rw) = (M + tw * 0.6, w - 2.0 * (M + tw * 0.6));
                HalfGeo { room: (rx, below_title, rw, bottom - below_title),
                          tiles: [(M, below_title, tw, th), (w - M - tw, below_title, tw, th), (M, bottom - th, tw, th), (w - M - tw, bottom - th, tw, th)],
                          head_top: 56, band: None }
            }
        }
        _ => {
            let top = h - ch - rows_h(10.0) - 12.0;
            let ry = h * 0.22;
            HalfGeo { room: (0.0, ry, w, (top - ry + 30.0).max(120.0)), tiles: grid(top, 10.0), head_top: 56, band: None }
        }
    }
}

/// How the sound choices and the cover are laid out (Look and feel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Slim,
    SlimBig,
    Rows,
    RowsTitle,
    Bar,
    Split,
    SplitSlim,
}

impl Layout {
    pub fn from_id(s: &str) -> Layout {
        match s {
            "slimbig" => Layout::SlimBig,
            "rows" => Layout::Rows,
            "rowstitle" => Layout::RowsTitle,
            "bar" => Layout::Bar,
            "split" => Layout::Split,
            "splitslim" => Layout::SplitSlim,
            _ => Layout::Slim,
        }
    }
    fn cover_by_title(&self) -> bool {
        matches!(self, Layout::Slim | Layout::RowsTitle)
    }
    fn rows(&self) -> bool {
        matches!(self, Layout::Rows | Layout::RowsTitle | Layout::Split)
    }
}

type Cb = RefCell<Option<Rc<dyn Fn(&str)>>>;

/// A picture shown at exactly the size asked for (GtkPicture would take the
/// picture's own size), with the sleeve's shadow and thin edge.
#[derive(Clone)]
pub struct Art {
    pub area: gtk::DrawingArea,
    surf: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    framed: bool,
}

impl Art {
    fn new(framed: bool) -> Art {
        let area = gtk::DrawingArea::builder().can_target(false).visible(false).build();
        let surf: Rc<RefCell<Option<gtk::cairo::ImageSurface>>> = Rc::new(RefCell::new(None));
        let s2 = surf.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let Some(img) = s2.borrow().clone() else { return };
            let (w, h) = (w as f64, h as f64);
            let (iw, ih) = (img.width() as f64, img.height() as f64);
            let pad = if framed { 14.0 } else { 0.0 };
            let k = ((w - 2.0 * pad) / iw).min((h - pad - 2.0) / ih);
            let (dw, dh) = (iw * k, ih * k);
            let (x, y) = (pad, h - dh - 2.0);
            if framed {
                for i in 0..10 {
                    let g = i as f64 * 1.6;
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.06);
                    cr.rectangle(x - g * 0.4, y + g * 0.8, dw + g * 0.8, dh + g * 0.3);
                    let _ = cr.fill();
                }
            }
            cr.save().ok();
            cr.translate(x, y);
            cr.scale(k, k);
            let _ = cr.set_source_surface(&img, 0.0, 0.0);
            cr.source().set_filter(gtk::cairo::Filter::Good);
            let _ = cr.paint();
            cr.restore().ok();
            if framed {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.36);
                cr.set_line_width(1.0);
                cr.rectangle(x + 0.5, y + 0.5, dw - 1.0, dh - 1.0);
                let _ = cr.stroke();
            }
        });
        Art { area, surf, framed }
    }

    pub fn set(&self, s: Option<gtk::cairo::ImageSurface>) {
        *self.surf.borrow_mut() = s;
        self.area.queue_draw();
    }

    /// Sizes the area to show the picture `h` pixels tall (wider for wide pictures).
    pub fn fit_height(&self, h: f64, max_w: f64) {
        let pad = if self.framed { 28.0 } else { 0.0 };
        let (iw, ih) = self.surf.borrow().as_ref().map(|s| (s.width() as f64, s.height() as f64)).unwrap_or((1.0, 1.0));
        let mut dh = h.max(1.0);
        let mut dw = dh * iw / ih.max(1.0);
        if dw > max_w { dw = max_w.max(1.0); dh = dw * ih / iw.max(1.0); }
        self.area.set_content_width((dw + pad) as i32);
        self.area.set_content_height((dh + if self.framed { 16.0 } else { 0.0 }) as i32);
    }
}

pub struct Stage {
    pub root: gtk::Overlay,
    /// The compact views (mini.rs): the window shows one of them instead of the page.
    pub remote: Rc<crate::mini::Remote>,
    pub strip: Rc<crate::mini::Strip>,
    pub shoulder: Rc<crate::mini::Shoulder>,
    bg: gtk::Stack,
    bg_pics: [gtk::Picture; 2],
    bg_flip: Cell<bool>,
    bg_list: RefCell<Vec<gdk::Texture>>,
    bg_i: Cell<usize>,
    bg_timer: RefCell<Option<glib::SourceId>>,
    pub every: Cell<u64>,
    split_pic: gtk::Picture,
    pub room: gtk::DrawingArea,
    head: gtk::Box,
    kick: gtk::Label,
    title: gtk::Label,
    logo: Art,
    sleeve: Art,
    subtitle: gtk::Label,
    credits: gtk::Box,
    bio: gtk::Label,
    facts: gtk::Label,
    fsbtn: gtk::Button,
    fsbtn_l: gtk::Label,
    corner: gtk::Box,
    ticket: gtk::Box,
    tk_sub: gtk::Label,
    tk_src: gtk::Label,
    tk_bc: gtk::DrawingArea,
    tk_num: gtk::DrawingArea,
    ticket_now: RefCell<Option<Ticket>>,
    bignum: gtk::Label,
    bigspk: gtk::Label,
    hline: gtk::Label,
    venue: gtk::Box,
    venue_n: gtk::Label,
    venue_d: gtk::Label,
    bigart: Art,
    /// Height free for the big cover between the title block and the dock.
    cover_room: Cell<f64>,
    low: gtk::Box,
    dock: gtk::Box,
    /// Short compact windows: the sound choices two to a line.
    pairs: Cell<bool>,
    /// Parts of the title block hidden while it would run into the choices
    /// below: the bio, then the full-screen button (with the height each
    /// freed, to know when it fits again).
    shed: RefCell<Vec<f64>>,
    /// The slim dock: one line left of the controls (`ctl`), in `low_sw`.
    dock_line: gtk::Box,
    ctl: gtk::Box,
    low_sw: gtk::ScrolledWindow,
    /// How far the slim dock is squeezed to stay on one line: 0 as written,
    /// 1 the short phrases, 2 closer together, 3 no labels, 4 the words cut short. `dock_need`
    /// holds the width each level wanted, to know when it fits again.
    dock_fit: Cell<u8>,
    dock_need: RefCell<Vec<i32>>,
    /// Sizes that follow the window width like the design's vw sizes: the
    /// title, the artist line and the slim dock's words (`fluid_now` is the
    /// rules last loaded).
    fluid_css: gtk::CssProvider,
    fluid_now: RefCell<String>,
    /// Where the title block sits: its centre-ish (54% of its height) at this
    /// share of the page height, per kind of page (the design's top: 38% and
    /// translateY(-54%)).
    anchor: Cell<f64>,
    /// The title block's widest (see `resized`).
    head_w: Cell<i32>,
    rows: gtk::Box,
    rows_art: Art,
    rows_fields: gtk::Box,
    bar: gtk::Box,
    bar_fields: gtk::Box,
    bar_sub: gtk::Label,
    tech: gtk::Label,
    pub hint: gtk::Label,
    pub readout: gtk::Label,
    pub readout_btn: gtk::MenuButton,
    pub prev: gtk::Button,
    pub pp: gtk::Button,
    pub next: gtk::Button,
    pub hp: gtk::ToggleButton,
    pub volume: gtk::Scale,
    pub test: gtk::Button,
    pub menu: gtk::Button,
    pub video: gtk::Picture,
    cover: RefCell<Option<gtk::cairo::ImageSurface>>,
    tall_cover: Cell<bool>,
    fields: RefCell<Vec<Field>>,
    head_now: RefCell<Head>,
    layout: Cell<Layout>,
    compact: Cell<bool>,
    /// Half a screen (900–1200 wide, 600+ tall, e.g. Meta+Left on a 1920
    /// screen): the theme's own layout (round 125). The theme's id, or empty.
    half: RefCell<String>,
    /// The four sound choices as tiles (half a screen only), and the red band
    /// behind the title (Screenprint's).
    tiles: Vec<gtk::Box>,
    band: gtk::Box,
    /// Where the room and the tiles are drawn now (page pixels), half a screen only.
    half_geo: Cell<Option<HalfGeo>>,
    /// The window is small: the page asks for the compact view.
    pub small: Cell<bool>,
    pub on_pick: Cb,
    pub on_action: Cb,
}

fn lbl(text: &str, cls: &[&str]) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(cls.iter().map(|s| s.to_string()).collect::<Vec<_>>()).build()
}

/// The design's speaker icon (a cabinet with a tweeter and a woofer), 20 px.
fn speaker_icon() -> gtk::DrawingArea {
    let a = gtk::DrawingArea::builder().content_width(20).content_height(20).halign(gtk::Align::Center).valign(gtk::Align::Center).build();
    a.set_draw_func(|_, cr, _, _| {
        cr.scale(20.0 / 24.0, 20.0 / 24.0);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_line_width(1.8);
        let (x, y, w, h, r) = (6.0, 2.5, 12.0, 19.0, 2.5);
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
        cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        cr.close_path();
        let _ = cr.stroke();
        for (cy, rr) in [(14.5, 3.4), (7.2, 1.3)] {
            cr.new_sub_path();
            cr.arc(12.0, cy, rr, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
        }
    });
    a
}

/// The design's chevron after a word you can change: half the word's size,
/// a thin red stroke sitting on its baseline.
/// The first label inside a widget (a choice's word inside its button).
fn first_label(w: &gtk::Widget) -> Option<gtk::Label> {
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        return Some(l.clone());
    }
    let mut c = w.first_child();
    while let Some(x) = c {
        if let Some(l) = first_label(&x) {
            return Some(l);
        }
        c = x.next_sibling();
    }
    None
}

pub(crate) fn chevron(word: &gtk::Label) -> gtk::DrawingArea {
    let a = gtk::DrawingArea::builder().content_width(12).css_classes(["chev"]).build();
    let wk = word.downgrade();
    a.set_draw_func(move |a, cr, _, _| {
        let Some(v) = wk.upgrade() else { return };
        let lay = v.layout();
        let em = lay.context().font_description().map(|d| d.size() as f64 / gtk::pango::SCALE as f64
            * if d.is_size_absolute() { 1.0 } else { 96.0 / 72.0 }).filter(|e| *e > 0.0).unwrap_or(19.0);
        let Some(top) = v.compute_point(a, &gtk::graphene::Point::new(0.0, 0.0)) else { return };
        let base = top.y() as f64 + v.layout_offsets().1 as f64 + lay.baseline() as f64 / gtk::pango::SCALE as f64;
        let (s, ml) = (em * 0.5, em * 0.06);
        let want = (s + ml).ceil() as i32 + 1;
        if (want - a.content_width()).abs() > 1 {
            let aw = a.downgrade();
            glib::idle_add_local_once(move || if let Some(a) = aw.upgrade() { a.set_content_width(want) });
        }
        let k = s / 24.0;
        let y0 = base - 0.06 * em - s;
        let c = a.color();
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        cr.set_line_width(3.4 * k);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        cr.move_to(ml + 6.0 * k, y0 + 9.0 * k);
        cr.line_to(ml + 12.0 * k, y0 + 15.0 * k);
        cr.line_to(ml + 18.0 * k, y0 + 9.0 * k);
        let _ = cr.stroke();
    });
    a
}

fn round_btn(icon: &str, tip: &str, cls: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).tooltip_text(tip).css_classes([cls]).valign(gtk::Align::Center).build()
}

impl Stage {
    pub fn new(vol: &gtk::Adjustment) -> Rc<Stage> {
        let root = gtk::Overlay::builder().css_classes(["stage"]).hexpand(true).vexpand(true).build();
        root.set_overflow(gtk::Overflow::Hidden);
        let mk_pic = || gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).hexpand(true).vexpand(true).build();
        let bg_pics = [mk_pic(), mk_pic()];
        let bg = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).transition_duration(3500)
            .hexpand(true).vexpand(true).build();
        bg.add_named(&bg_pics[0], Some("a"));
        bg.add_named(&bg_pics[1], Some("b"));
        root.set_child(Some(&bg));

        let split_pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).halign(gtk::Align::Start)
            .vexpand(true).css_classes(["splitart"]).visible(false).can_target(false).build();
        root.add_overlay(&split_pic);
        let shade = gtk::Box::builder().css_classes(["shade"]).can_target(false).build();
        root.add_overlay(&shade);
        let room = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        root.add_overlay(&room);
        // Half a screen: Screenprint's red band, then the four tiles, above the room.
        let band = gtk::Box::builder().css_classes(["hband"]).halign(gtk::Align::Fill).valign(gtk::Align::Start).visible(false).can_target(false).build();
        root.add_overlay(&band);
        let tiles: Vec<gtk::Box> = (0..4).map(|_| {
            let t = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).css_classes(["tile"])
                .halign(gtk::Align::Start).valign(gtk::Align::Start).visible(false).build();
            root.add_overlay(&t);
            // A press anywhere on the tile opens its choices.
            let g = gtk::GestureClick::new();
            let tw = t.downgrade();
            g.connect_released(move |_, _, _, _| {
                let Some(t) = tw.upgrade() else { return };
                let mut c = t.last_child().and_then(|b| b.first_child());
                while let Some(w) = c {
                    if let Some(mb) = w.downcast_ref::<gtk::MenuButton>() {
                        mb.popup();
                        return;
                    }
                    c = w.first_child();
                }
            });
            t.add_controller(g);
            t
        }).collect();
        // Filled, with margins on all four sides: exactly the screen's rectangle (a
        // GtkPicture would otherwise ask for the video's own size).
        let video = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill).can_target(false).visible(false).build();
        root.add_overlay(&video);

        // ---- the title block
        let kick = lbl("", &["kick"]);
        let title = lbl("", &["title"]);
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title.set_lines(3);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let logo = Art::new(false);
        logo.area.set_halign(gtk::Align::Start);
        let sleeve = Art::new(true);
        sleeve.area.set_valign(gtk::Align::End);
        let subtitle = lbl("", &["artist"]);
        subtitle.set_wrap(true);
        let tcol = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::End).build();
        tcol.append(&title);
        tcol.append(&logo.area);
        tcol.append(&subtitle);
        let hrow = gtk::Box::builder().spacing(24).margin_top(10).build();
        hrow.append(&sleeve.area);
        hrow.append(&tcol);
        let hline = lbl("", &["hline"]);
        hline.set_margin_top(8);
        hline.set_visible(false);
        let credits = gtk::Box::builder().spacing(26).css_classes(["credits"]).build();
        let bio = lbl("", &["bio"]);
        bio.set_wrap(true);
        bio.set_max_width_chars(58);
        bio.set_lines(2);
        // Its own width (at most 540 px, see fit_title_width), not the block's.
        bio.set_halign(gtk::Align::Start);
        bio.set_ellipsize(gtk::pango::EllipsizeMode::End);
        bio.set_margin_top(22);
        let facts = lbl("", &["facts"]);
        facts.set_margin_top(12);
        let fsbtn_l = gtk::Label::new(None);
        let fsb = gtk::Box::builder().spacing(8).build();
        fsb.append(&gtk::Image::from_icon_name("view-fullscreen-symbolic"));
        fsb.append(&fsbtn_l);
        let fsbtn = gtk::Button::builder().child(&fsb).css_classes(["fsbtn"]).halign(gtk::Align::Start).margin_top(14).visible(false).build();
        let head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).halign(gtk::Align::Start).valign(gtk::Align::Start)
            .margin_start(46).margin_top(78).width_request(300).build();
        head.append(&kick);
        head.append(&hrow);
        head.append(&hline);
        head.append(&credits);
        head.append(&bio);
        head.append(&facts);
        head.append(&fsbtn);
        root.add_overlay(&head);

        // ---- top right: the big number, or the live venue
        let bignum = lbl("", &["bignum"]);
        let bigspk = gtk::Label::builder().use_markup(true).css_classes(["bigspk"]).valign(gtk::Align::End).margin_bottom(6).build();
        let venue_n = lbl("", &["vn"]);
        let venue_d = lbl("", &["vd"]);
        let venue = gtk::Box::builder().spacing(10).css_classes(["venue"]).visible(false).build();
        venue.append(&lbl("LIVE", &["rp"]));
        venue.append(&venue_n);
        venue.append(&venue_d);
        let corner = gtk::Box::builder().spacing(12).halign(gtk::Align::End).valign(gtk::Align::Start).margin_end(56).margin_top(64)
            .can_target(false).build();
        corner.append(&bignum);
        corner.append(&bigspk);
        corner.append(&venue);
        root.add_overlay(&corner);

        // ---- films and series: the ticket band across the top
        let tk_sub = lbl("", &["tk-sub"]);
        let tk_src = lbl("", &["tk-sub"]);
        let tk_bc = gtk::DrawingArea::builder().content_width(118).content_height(34).valign(gtk::Align::Center).build();
        let tk_num = gtk::DrawingArea::builder().content_width(90).content_height(60).valign(gtk::Align::Center).build();
        let ticket = gtk::Box::builder().spacing(22).css_classes(["ticket"]).valign(gtk::Align::Start).margin_top(58)
            .margin_start(46).margin_end(46).visible(false).can_target(false).build();
        let tk_admit = lbl("ADMIT ONE", &["tk-admit"]);
        tk_admit.set_valign(gtk::Align::Center);
        tk_sub.set_valign(gtk::Align::Center);
        tk_src.set_valign(gtk::Align::Center);
        tk_src.set_hexpand(true);
        tk_src.set_xalign(1.0);
        ticket.append(&tk_admit);
        ticket.append(&tk_sub);
        ticket.append(&tk_src);
        ticket.append(&tk_bc);
        ticket.append(&tk_num);
        root.add_overlay(&ticket);

        // ---- the big cover (bottom left)
        let bigart = Art::new(true);
        bigart.area.set_halign(gtk::Align::Start);
        bigart.area.set_valign(gtk::Align::End);
        bigart.area.set_margin_start(32);
        bigart.area.set_margin_bottom(104);
        root.add_overlay(&bigart.area);

        // ---- rows (bottom left)
        let rows_art = Art::new(true);
        rows_art.area.set_valign(gtk::Align::End);
        let rows_fields = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::End).width_request(330).build();
        let rows = gtk::Box::builder().spacing(18).css_classes(["rows"]).halign(gtk::Align::Start).valign(gtk::Align::End)
            .margin_start(46).margin_bottom(100).visible(false).build();
        rows.append(&rows_art.area);
        rows.append(&rows_fields);
        root.add_overlay(&rows);

        // ---- the bottom: dock with the sound line (or the bar) and the controls
        let bar_sub = lbl("", &["ts"]);
        let tab = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["tab"]).valign(gtk::Align::Fill).build();
        tab.append(&lbl("SOUND", &["tb"]));
        tab.append(&bar_sub);
        let bar_fields = gtk::Box::builder().build();
        let bar = gtk::Box::builder().css_classes(["bar"]).margin_bottom(10).visible(false).build();
        bar.append(&tab);
        bar.append(&bar_fields);
        let dock_line = gtk::Box::builder().css_classes(["dline"]).valign(gtk::Align::Center).hexpand(true).build();
        let prev = round_btn("media-skip-backward-symbolic", "Previous", "flat-t");
        let pp = round_btn("media-playback-pause-symbolic", "Play / pause", "round");
        pp.add_css_class("pp");
        let next = round_btn("media-skip-forward-symbolic", "Next", "flat-t");
        let readout = gtk::Label::builder().build();
        // The readout opens the signal path (each step the sound takes).
        let readout_btn = gtk::MenuButton::builder().child(&readout).css_classes(["readout"]).valign(gtk::Align::Center)
            .tooltip_text("Signal path: how the sound gets from the app to your speakers").build();
        let hp = gtk::ToggleButton::builder().icon_name("audio-headphones-symbolic").tooltip_text("Headphones")
            .css_classes(["round"]).valign(gtk::Align::Center).build();
        let volume = gtk::Scale::builder().adjustment(vol).orientation(gtk::Orientation::Horizontal).draw_value(false)
            .valign(gtk::Align::Center).tooltip_text("Volume").build();
        let test = round_btn("audio-speakers-symbolic", "Test speakers", "round");
        test.set_child(Some(&speaker_icon()));
        // ☰ opens the control centre (not a menu).
        let menu = gtk::Button::builder().icon_name("open-menu-symbolic").tooltip_text("Sound, Mixer, Microphone, Speakers, Look and feel, Settings").css_classes(["round"])
            .valign(gtk::Align::Center).build();
        let ctl = gtk::Box::builder().spacing(10).css_classes(["ctl"]).halign(gtk::Align::End).hexpand(true).valign(gtk::Align::Center).build();
        let transport = gtk::Box::builder().spacing(6).valign(gtk::Align::Center).build();
        for w in [&prev, &pp, &next] {
            transport.append(w);
        }
        for w in [transport.upcast_ref::<gtk::Widget>(), readout_btn.upcast_ref(), hp.upcast_ref(),
                  volume.upcast_ref(), test.upcast_ref(), menu.upcast_ref()] {
            ctl.append(w);
        }
        let tech = lbl("", &["tech"]);
        let hint = lbl("tap a speaker to test it", &["hint"]);
        hint.set_margin_bottom(6);
        let low = gtk::Box::builder().spacing(16).css_classes(["low"]).build();
        low.append(&dock_line);
        low.append(&ctl);
        let dock = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["dock"]).valign(gtk::Align::End).build();
        dock.append(&hint);
        dock.append(&bar);
        // Scrolls sideways rather than holding the window wide (small windows get the compact view).
        let low_sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::External).vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true).propagate_natural_height(true).child(&low).build();
        dock.append(&low_sw);
        root.add_overlay(&dock);
        let fluid_css = gtk::CssProvider::new();
        if let Some(d) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&d, &fluid_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }

        let remote = crate::mini::Remote::new(vol);
        let strip = crate::mini::Strip::new(vol);
        let shoulder = crate::mini::Shoulder::new(vol);
        let st = Rc::new(Stage {
            root, remote, strip, shoulder, bg, bg_pics, bg_flip: Cell::new(false), bg_list: RefCell::new(vec![]), bg_i: Cell::new(0), bg_timer: RefCell::new(None),
            every: Cell::new(120), split_pic, room, head, kick, title, logo, sleeve, subtitle, credits, hline, bio, facts, fsbtn, fsbtn_l,
            corner, ticket, tk_sub, tk_src, tk_bc, tk_num, ticket_now: RefCell::new(None), bignum, bigspk, venue, venue_n, venue_d, bigart, cover_room: Cell::new(f64::MAX), dock_line, rows, rows_art, rows_fields, bar, bar_fields, bar_sub,
            tech, hint, readout, readout_btn, prev, pp, next, hp, volume, test, menu, video, low: low.clone(), dock: dock.clone(),
            pairs: Cell::new(false), shed: RefCell::new(vec![]), ctl, low_sw, dock_fit: Cell::new(0), dock_need: RefCell::new(vec![]),
            fluid_css, fluid_now: RefCell::new(String::new()), anchor: Cell::new(0.38), head_w: Cell::new(300),
            cover: RefCell::new(None), tall_cover: Cell::new(false), fields: RefCell::new(vec![]), head_now: RefCell::new(Head::default()),
            layout: Cell::new(Layout::SlimBig), compact: Cell::new(false), small: Cell::new(false), on_pick: RefCell::new(None), on_action: RefCell::new(None),
            half: RefCell::new(String::new()), tiles, band, half_geo: Cell::new(None),
        });
        {
            let send = |wk: std::rc::Weak<Stage>| -> Rc<dyn Fn(&str)> { Rc::new(move |a: &str| if let Some(s) = wk.upgrade() {
                if let Some(x) = a.strip_prefix("act:") { s.act(x) } else if let Some(x) = a.strip_prefix("pick:") { s.pick(x) }
            }) };
            st.remote.set_send(send(Rc::downgrade(&st)));
            st.strip.set_send(send(Rc::downgrade(&st)));
            st.shoulder.set_send(send(Rc::downgrade(&st)));
        }
        for (b, a) in [(&st.prev, "prev"), (&st.pp, "pp"), (&st.next, "next"), (&st.test, "test"), (&st.fsbtn, "fs"), (&st.menu, "menu")] {
            let wk = Rc::downgrade(&st);
            b.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.act(a) });
        }
        {
            let wk = Rc::downgrade(&st);
            st.hp.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.act("hp") });
        }
        {
            let wk = Rc::downgrade(&st);
            st.room.connect_resize(move |_, w, h| if let Some(s) = wk.upgrade() { s.resized(w, h) });
        }
        {
            // The title block's height changes with what's playing: keep the
            // big cover in the space under it.
            let wk = Rc::downgrade(&st);
            glib::timeout_add_local(Duration::from_millis(250), move || {
                let Some(s) = wk.upgrade() else { return glib::ControlFlow::Break };
                s.fit_title_width();
                s.set_head_top();
                s.fit_cover_room();
                s.fit_dock_line();
                glib::ControlFlow::Continue
            });
        }
        {
            // The ticket's barcode (bars picked by the title) and its outlined number.
            let wk = Rc::downgrade(&st);
            st.tk_bc.set_draw_func(move |_, cr, w, h| {
                let Some(s) = wk.upgrade() else { return };
                let seed = s.ticket_now.borrow().as_ref().map(|t| t.seed.clone()).unwrap_or_default();
                let mut r = seed.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
                let mut x = 0.0;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
                while x < w as f64 {
                    r = r.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let bw = 1.0 + ((r >> 33) % 3) as f64;
                    cr.rectangle(x, 0.0, bw.min(w as f64 - x), h as f64);
                    x += bw + 1.0 + ((r >> 40) % 3) as f64;
                }
                let _ = cr.fill();
            });
            let wk = Rc::downgrade(&st);
            st.tk_num.set_draw_func(move |_, cr, w, h| {
                let Some(s) = wk.upgrade() else { return };
                let num = s.ticket_now.borrow().as_ref().map(|t| t.number.clone()).unwrap_or_default();
                cr.select_font_face("Anton", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
                cr.set_font_size(h as f64 * 0.92);
                let Ok(ext) = cr.text_extents(&num) else { return };
                cr.move_to(w as f64 - ext.width() - ext.x_bearing() - 2.0, h as f64 - 3.0);
                cr.text_path(&num);
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(1.5);
                let _ = cr.stroke();
            });
        }
        st
    }

    /// Whether the ticket band shows (for the self-test).
    pub fn ticket_shown(&self) -> bool {
        self.ticket.is_visible()
    }

    /// The ticket band for a film or series (none for other pages, or in the
    /// compact view): it takes the place of the kicker and the top-right corner.
    pub fn set_ticket(&self, t: Option<Ticket>) {
        let t = t.filter(|_| !self.compact.get() && !self.is_half());
        if *self.ticket_now.borrow() == t {
            return;
        }
        if let Some(t) = &t {
            self.tk_sub.set_label(&t.left.to_uppercase());
            self.tk_src.set_label(&t.source.to_uppercase());
            self.tk_num.set_content_width(26 * t.number.chars().count() as i32 + 10);
        }
        self.ticket.set_visible(t.is_some());
        self.kick.set_visible(t.is_none());
        *self.ticket_now.borrow_mut() = t;
        self.tk_bc.queue_draw();
        self.tk_num.queue_draw();
        self.set_head_top();
    }

    /// Where the title block sits on this kind of page (see `anchor`).
    pub fn set_anchor(&self, a: f64) {
        if (self.anchor.get() - a).abs() > 0.001 {
            self.anchor.set(a);
            self.set_head_top();
        }
    }

    /// Places the title block as the design does: 54% of its height above
    /// the anchor line, but never above the top margin (or the ticket band).
    fn set_head_top(&self) {
        if self.is_half() {
            self.place_half();
            return;
        }
        let h = self.root.height();
        let base = if self.compact.get() { 56 } else if h < 720 { 58 } else { 78 };
        let min = base + if self.ticket.is_visible() { if h < 720 { 52 } else { 66 } } else { 0 };
        if self.compact.get() || h <= 0 {
            self.head.set_margin_top(min);
            return;
        }
        // Its real height once laid out (GTK's height-for-width measure of the
        // block comes out taller than what it gets).
        let hh = if self.head.height() > 0 { self.head.height() } else {
            let fw = self.head_w.get().max(self.head.measure(gtk::Orientation::Horizontal, -1).0).max(1);
            self.head.measure(gtk::Orientation::Vertical, fw).0
        };
        let top = (h as f64 * self.anchor.get() - 0.54 * hh as f64).round() as i32;
        let top = top.max(min.min(base));
        let top = if self.ticket.is_visible() { top.max(min) } else { top };
        if (self.head.margin_top() - top).abs() > 1 {
            self.head.set_margin_top(top);
        }
    }

    fn fit_cover_room(&self) {
        if !self.root.is_mapped() {
            return;
        }
        let Some(hb) = self.head.compute_bounds(&self.root) else { return };
        let head_bottom = (hb.y() + hb.height()) as f64;
        let free = self.root.height() as f64 - self.bigart.area.margin_bottom() as f64 - head_bottom - 40.0;
        if (free - self.cover_room.get()).abs() > 6.0 {
            self.cover_room.set(free);
            self.place_cover();
        }
        // The title block against what sits below it (the choices, or the dock).
        let gap = self.lower_top() - head_bottom - 10.0;
        let n = self.shed.borrow().len();
        let last = self.shed.borrow().last().copied();
        if gap < 0.0 && n < 2 {
            let w: &gtk::Widget = if n == 0 { self.bio.upcast_ref() } else { self.fsbtn.upcast_ref() };
            self.shed.borrow_mut().push(if w.is_visible() { w.height() as f64 + 8.0 } else { 0.0 });
            w.set_visible(false);
        } else if let Some(freed) = last && gap > freed + 12.0 {
            self.shed.borrow_mut().pop();
            self.show_shed();
        }
    }

    /// Keeps the title and the artist line within the title block: a wrapped
    /// label otherwise asks for its whole text on one line. GTK measures
    /// `max_width_chars` with the font's approximate character width, so the
    /// same measure gives the block's width in characters.
    fn fit_title_width(&self) {
        let hw = self.head_w.get() as f64;
        // The bio is at most 540 px wide in the design.
        for (l, w) in [(&self.title, hw), (&self.subtitle, hw), (&self.bio, hw.min(540.0))] {
            let m = l.pango_context().metrics(None, None);
            let cw = m.approximate_char_width().max(m.approximate_digit_width()) as f64 / gtk::pango::SCALE as f64;
            l.set_max_width_chars(if cw > 0.0 && w > 0.0 { (w / cw).floor().max(8.0) as i32 } else { -1 });
        }
    }

    /// Where the choices (or the dock) start below the title block, in page pixels.
    pub fn lower_top(&self) -> f64 {
        [self.rows.upcast_ref::<gtk::Widget>(), self.dock.upcast_ref()].into_iter()
            .filter(|w| w.is_visible())
            .filter_map(|w| w.compute_bounds(&self.root))
            .map(|b| b.y() as f64)
            .fold(self.root.height() as f64, f64::min)
    }

    /// Shows the bio and the full-screen button unless they're shed.
    fn show_shed(&self) {
        let n = self.shed.borrow().len();
        let h = self.head_now.borrow();
        let half = self.is_half();
        self.bio.set_visible(!half && !h.bio.is_empty() && n < 1);
        self.fsbtn.set_visible(!half && h.button.is_some() && n < 2);
    }

    fn act(&self, a: &str) {
        let cb = self.on_action.borrow().clone();
        if let Some(cb) = cb { cb(a) }
    }

    fn pick(&self, a: &str) {
        let cb = self.on_pick.borrow().clone();
        if let Some(cb) = cb { cb(a) }
    }

    /// Sizes follow the window: classes for smaller widths and short windows.
    fn resized(self: &Rc<Self>, w: i32, h: i32) {

        let small = w < 900 || h < 600;
        let pairs = (self.compact.get() || small) && w >= 700 && h < 760;
        if pairs != self.pairs.get() {
            self.pairs.set(pairs);
            // Lays the choices out again.
            self.fields.borrow_mut().clear();
            if small == self.small.get() {
                self.act("resized");
            }
        }
        if small != self.small.get() {
            self.small.set(small);
            self.act("resized");
        }
        let r = &self.root;
        let size = if self.compact.get() { "" } else if w < 1060 { "s-s" } else if w < 1420 { "s-m" } else { "s-l" };
        for c in ["s-s", "s-m", "s-l"] {
            if c == size { r.add_css_class(c) } else { r.remove_css_class(c) }
        }
        if h < 720 && !self.compact.get() { r.add_css_class("short") } else { r.remove_css_class("short") }
        // The title block may use a little under half the width.
        // Compact: the small speaker plan takes the top right when there's room (see ui/page.rs).
        let wf = w as f64;
        let hw = if self.compact.get() {
            if w > 560 { wf - (wf * 0.42).min(h as f64 * 0.5).min(460.0) - 48.0 } else { wf * 0.92 }
        } else if self.is_half() { wf - 72.0 } else { wf * 0.44 }.clamp(260.0, 780.0) as i32;
        // The block is as wide as what's in it, up to hw (the design's
        // max-width: min(44vw, 780px)); the compact view fills its width.
        self.head_w.set(hw);
        self.head.set_width_request(if self.compact.get() { hw } else { -1 });
        self.fit_title_width();
        let m = if self.compact.get() { 18 } else if self.is_half() { 36 } else if w < 1060 { 24 } else { 46 };
        for wd in [self.head.upcast_ref::<gtk::Widget>(), self.rows.upcast_ref()] {
            wd.set_margin_start(m);
        }
        self.bigart.area.set_margin_start(m - 14);
        // Very narrow: no big number at the top (the readout says it), slimmer controls.
        self.corner.set_visible(!self.is_half() && (!self.compact.get() || w >= 640));
        self.readout_btn.set_visible(w >= 520);
        self.test.set_visible(w >= 600);
        self.volume.set_width_request(if w < 520 { 70 } else { 100 });
        // The slim dock stays one line beside the controls (round 80); its
        // words grow with the width like the design's clamp(14px, 1.12vw, 19px)
        // times 1.2 (the display font's scale there).
        self.low.set_spacing(if w < 1060 { 12 } else { 16 });
        self.bigart.area.set_margin_bottom(104);
        // (The compact dock holds only the controls: the choices sit lower.)
        self.rows.set_margin_bottom(if self.compact.get() { 86 } else { 100 });
        // The title block's sizes are the design's too (clamp(42px, 5.2vw, 96px)
        // and so on, times 1.2); short windows keep the title within 11% of the height.
        let half = |v: f64| (v * 2.0).round() / 2.0;
        let dock_px = half((wf * 0.0112).clamp(14.0, 19.0) * 1.2);
        let title_px = half(((wf * 0.052).clamp(42.0, 96.0) * 1.2).min(h as f64 * 0.11).max(44.0));
        let long_px = half((wf * 0.033).clamp(30.0, 60.0) * 1.2).min(title_px);
        let artist_px = half((wf * 0.023).clamp(22.0, 38.0) * 1.2);
        // Line heights in px: GTK takes a bare number as a share of the font's
        // own line height (tall for Anton), CSS as a share of its size.
        let lh = |px: f64, k: f64| (px * k).round();
        let rules = format!(".stage .dline label.v {{ font-size: {dock_px}px; }}\n\
            .stage:not(.compact) .title {{ font-size: {title_px}px; line-height: {}px; }}\n\
            .stage:not(.compact) .title.long {{ font-size: {long_px}px; line-height: {}px; }}\n\
            .stage:not(.compact) .artist {{ font-size: {artist_px}px; line-height: {}px; }}",
            lh(title_px, 0.95), lh(long_px, 1.04), lh(artist_px, 1.1));
        if *self.fluid_now.borrow() != rules {
            let dock_changed = !self.fluid_now.borrow().starts_with(&format!(".stage .dline label.v {{ font-size: {dock_px}px; }}"));
            self.fluid_css.load_from_string(&rules);
            *self.fluid_now.borrow_mut() = rules;
            if dock_changed {
                self.dock_need.borrow_mut().clear();
                if self.dock_fit.replace(0) != 0 {
                    self.refill_later();
                }
            }
        }
        if self.is_half() {
            self.place_half();
        }
        // The artist line stands 0.35 of its size below the title.
        self.subtitle.set_margin_top(if self.compact.get() { 4 } else { (artist_px * 0.35).round() as i32 });
        self.fit_dock_later();
        self.set_head_top();
        self.split_pic.set_width_request(46 + ((w as f64 * 0.44).min(780.0)) as i32 + 38);
        self.place_cover();
    }

    // ---- content

    /// The background pictures; with more than one they take turns slowly.
    pub fn set_backgrounds(self: &Rc<Self>, list: Vec<gdk::Texture>) {
        let same = {
            let cur = self.bg_list.borrow();
            cur.len() == list.len() && cur.iter().zip(list.iter()).all(|(a, b)| a == b)
        };
        if same {
            return;
        }
        *self.bg_list.borrow_mut() = list;
        self.bg_i.set(0);
        self.show_bg(0);
        if let Some(id) = self.bg_timer.borrow_mut().take() {
            id.remove();
        }
        if self.bg_list.borrow().len() > 1 {
            let wk = Rc::downgrade(self);
            let id = glib::timeout_add_local(Duration::from_secs(self.every.get()), move || {
                let Some(s) = wk.upgrade() else { return glib::ControlFlow::Break };
                let n = s.bg_list.borrow().len();
                if n > 1 {
                    s.bg_i.set((s.bg_i.get() + 1) % n);
                    s.show_bg(s.bg_i.get());
                }
                glib::ControlFlow::Continue
            });
            *self.bg_timer.borrow_mut() = Some(id);
        }
    }

    /// The background photo showing now (printed in red), for the speaker test.
    pub fn bg_paintable(&self) -> Option<gtk::gdk::Paintable> {
        self.bg_pics[self.bg_flip.get() as usize].paintable()
    }

    fn show_bg(&self, i: usize) {
        let tex = self.bg_list.borrow().get(i).cloned();
        let flip = !self.bg_flip.get();
        self.bg_flip.set(flip);
        let pic = &self.bg_pics[flip as usize];
        pic.set_paintable(tex.as_ref());
        self.bg.set_visible_child(pic);
        // The Remote's top and the mini's show the same photo, printed in red.
        self.remote.set_photo(tex.as_ref());
        self.shoulder.set_photo(tex.as_ref());
    }

    /// The compact views follow the page's controls: play / pause, what can be
    /// pressed, headphones.
    pub fn mirror(&self) {
        let icon = self.pp.icon_name();
        for (pp, prev, next, hp) in [(&self.remote.pp, &self.remote.prev, &self.remote.next, &self.remote.hp),
                                     (&self.strip.pp, &self.strip.prev, &self.strip.next, &self.strip.hp)] {
            pp.set_icon_name(icon.as_deref().unwrap_or("media-playback-start-symbolic"));
            pp.set_sensitive(self.pp.is_sensitive());
            prev.set_sensitive(self.prev.is_sensitive());
            next.set_sensitive(self.next.is_sensitive());
            hp.set_active(self.hp.is_active());
        }
        self.shoulder.pp.set_icon_name(icon.as_deref().unwrap_or("media-playback-start-symbolic"));
        self.shoulder.pp.set_sensitive(self.pp.is_sensitive());
        self.shoulder.hp.set_active(self.hp.is_active());
    }

    /// Which background picture is showing (for the self-test).
    pub fn set_head(self: &Rc<Self>, h: &Head, logo: Option<&gtk::cairo::ImageSurface>) {
        self.remote.set_head(h);
        self.strip.set_head(h);
        self.shoulder.set_head(h);
        self.kick.set_label(&h.kick.to_uppercase());
        self.title.set_label(&h.title);
        if h.title.chars().count() > 30 { self.title.add_css_class("long") } else { self.title.remove_css_class("long") }
        self.fit_title_width();
        self.logo.set(logo.cloned());
        self.logo.area.set_visible(logo.is_some());
        self.title.set_visible(logo.is_none());
        if logo.is_some() {
            let maxh = if self.compact.get() { 64.0 } else if self.root.height() < 760 { 96.0 } else { 120.0 };
            self.logo.fit_height(maxh, 400.0);
        }
        self.subtitle.set_label(&h.subtitle);
        self.subtitle.set_visible(!h.subtitle.is_empty());
        if *self.head_now.borrow() != *h {
            while let Some(c) = self.credits.first_child() {
                self.credits.remove(&c);
            }
            for (l, v) in &h.credits {
                let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(3).build();
                b.append(&lbl(&l.to_uppercase(), &["cred-l"]));
                let vl = lbl(v, &["cred-v"]);
                vl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                vl.set_max_width_chars(34);
                b.append(&vl);
                self.credits.append(&b);
            }
        }
        self.hline.set_label(&h.line);
        self.hline.set_visible(!h.line.is_empty());
        self.credits.set_visible(!h.credits.is_empty() && h.line.is_empty() && !self.compact.get());
        self.bio.set_label(&h.bio);
        self.bio.set_visible(!h.bio.is_empty() && self.shed.borrow().is_empty());
        self.facts.set_label(&h.facts.to_uppercase());
        self.facts.set_visible(!h.facts.is_empty() && !self.compact.get());
        match &h.button {
            Some((t, s)) => {
                self.fsbtn_l.set_markup(&format!("{}  <span alpha='60%' weight='normal'>{}</span>",
                    glib::markup_escape_text(&t.to_uppercase()), glib::markup_escape_text(s)));
                self.fsbtn.set_visible(self.shed.borrow().len() < 2);
            }
            None => self.fsbtn.set_visible(false),
        }
        *self.head_now.borrow_mut() = h.clone();
        self.show_head_parts(h);
        // Re-place it once the new words are laid out.
        let wk = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(40), move || if let Some(s) = wk.upgrade() { s.set_head_top() });
    }

    /// Where the top-right corner sits over the room (x, y, w, h), if shown.
    pub fn corner_rect(&self) -> Option<(f64, f64, f64, f64)> {
        if self.ticket.is_visible() {
            // The ticket band's right end: the source, the barcode and the number.
            let t = self.ticket.compute_bounds(&self.room)?;
            let b = self.tk_bc.compute_bounds(&self.room)?;
            let x = b.x() as f64 - 190.0;
            return Some((x, t.y() as f64, (t.x() + t.width()) as f64 - x, t.height() as f64));
        }
        if !self.corner.is_visible() {
            return None;
        }
        let b = self.corner.compute_bounds(&self.room)?;
        Some((b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64))
    }

    pub fn set_corner(&self, c: &Corner) {
        self.shoulder.set_corner(c);
        match c {
            Corner::None => {
                self.bignum.set_visible(false);
                self.bigspk.set_visible(false);
                self.venue.set_visible(false);
            }
            Corner::Big(n, a, of) => {
                self.bignum.set_label(n);
                self.bigspk.set_markup(&format!("<span font_family='Anton' size='x-large' foreground='#ff4b53'>{a}</span> / {of}\nSPEAKERS"));
                self.bignum.set_visible(true);
                self.bigspk.set_visible(true);
                self.venue.set_visible(false);
            }
            Corner::Venue(n, d) => {
                self.venue_n.set_label(n);
                self.venue_d.set_label(&d.to_uppercase());
                self.bignum.set_visible(false);
                self.bigspk.set_visible(false);
                self.venue.set_visible(true);
            }
        }
    }

    /// The cover (album, poster, box). `tall` for posters and game boxes.
    pub fn set_cover(&self, tex: Option<gtk::cairo::ImageSurface>, tall: bool) {
        self.remote.set_cover(tex.clone());
        self.strip.set_cover(tex.clone());
        *self.cover.borrow_mut() = tex;
        self.tall_cover.set(tall);
        self.place_cover();
    }

    /// The split page: the album cover fills the left side.
    pub fn set_split(&self, tex: Option<&gdk::Texture>) {
        self.split_pic.set_paintable(tex);
    }

    pub fn set_layout(self: &Rc<Self>, l: Layout, compact: bool) {
        let changed = self.layout.get() != l || self.compact.get() != compact;
        self.layout.set(l);
        self.compact.set(compact);
        if compact { self.root.add_css_class("compact") } else { self.root.remove_css_class("compact") }
        let split = matches!(l, Layout::Split | Layout::SplitSlim) && !compact && self.split_pic.paintable().is_some();
        self.split_pic.set_visible(split);
        if split { self.root.add_css_class("split") } else { self.root.remove_css_class("split") }
        self.place_cover();
        if changed {
            let f = self.fields.borrow().clone();
            self.fields.borrow_mut().clear();
            self.set_fields(f, &self.tech.label());
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout.get()
    }

    /// Half a screen in this theme's layout, or the full page (None).
    pub fn set_half(self: &Rc<Self>, theme: Option<&str>) -> bool {
        let t = theme.unwrap_or("").to_string();
        if *self.half.borrow() == t {
            return false;
        }
        for c in ["half", "half-oys", "half-blueprint", "half-screenprint", "half-callouts"] {
            self.root.remove_css_class(c);
        }
        if !t.is_empty() {
            self.root.add_css_class("half");
            self.root.add_css_class(&format!("half-{t}"));
        }
        *self.half.borrow_mut() = t;
        let (w, h) = (self.root.width(), self.root.height());
        if w > 0 && h > 0 {
            self.resized(w, h);
        }
        self.place_half();
        self.place_cover();
        let f = self.fields.borrow().clone();
        self.fields.borrow_mut().clear();
        self.set_fields(f, &self.tech.label());
        let h = self.head_now.borrow().clone();
        self.show_head_parts(&h);
        true
    }

    pub fn is_half(&self) -> bool {
        !self.half.borrow().is_empty()
    }

    /// Where the room and the tiles go on half a screen (None on the full page).
    pub fn half_geo(&self) -> Option<HalfGeo> {
        self.half_geo.get()
    }

    /// Puts the tiles, the band, the title and the controls where the theme's
    /// half-screen layout wants them, for the page's size now.
    fn place_half(&self) {
        let half = self.half.borrow().clone();
        let (w, h) = (self.root.width(), self.root.height());
        let on = !half.is_empty() && w > 0 && h > 0;
        for t in &self.tiles {
            t.set_visible(on && t.first_child().is_some());
        }
        let ch = self.dock.measure(gtk::Orientation::Vertical, w).1 as f64;
        let th = self.tiles.iter().filter(|t| t.first_child().is_some())
            .map(|t| t.measure(gtk::Orientation::Vertical, -1).1 - t.margin_top() - t.margin_bottom()).max().unwrap_or(70) as f64;
        let head_h = self.head.measure(gtk::Orientation::Vertical, (w - 72).max(200)).1 - self.head.margin_top() - self.head.margin_bottom();
        let g = on.then(|| half_geo(&half, w as f64, h as f64, ch, th, head_h as f64));
        self.half_geo.set(g);
        let from_right = matches!(half.as_str(), "blueprint" | "callouts");
        if let Some(g) = g {
            self.fit_tile_words(&g);
        }
        for (i, t) in self.tiles.iter().enumerate() {
            if let Some(g) = g {
                let (x, y, tw, _) = g.tiles[i];
                t.set_margin_start(x.round() as i32);
                t.set_margin_top(y.round() as i32);
                t.set_size_request(tw.round() as i32, -1);
                let a = if from_right && x > w as f64 / 2.0 { gtk::Align::End } else { gtk::Align::Start };
                let mut c = t.first_child();
                while let Some(w) = c {
                    w.set_halign(a);
                    c = w.next_sibling();
                }
            }
        }
        match g.and_then(|g| g.band) {
            Some((y, bh)) => {
                self.band.set_margin_top(y.round() as i32);
                self.band.set_size_request(-1, bh.round() as i32);
                self.band.set_visible(true);
            }
            None => self.band.set_visible(false),
        }
        // The controls across the bottom, the choices above them (not in the dock line).
        self.ctl.set_halign(if on { gtk::Align::Center } else { gtk::Align::End });
        self.hint.set_visible(!on);
        self.corner.set_visible(!on && (!self.compact.get() || w >= 640));
        if let Some(g) = g {
            self.head.set_margin_top(g.head_top);
            self.head.set_width_request((w - 2 * 36).max(200));
        }
    }

    /// Each tile's word fits its tile: the whole phrase, else the phrase before
    /// the comma (as the Strip says it), else cut short with "…".
    fn fit_tile_words(&self, g: &HalfGeo) {
        let fields = self.fields.borrow();
        for (i, (t, f)) in self.tiles.iter().zip(fields.iter()).enumerate() {
            let Some(l) = t.last_child().and_then(|v| first_label(&v)) else { continue };
            let pad = if *self.half.borrow() == "callouts" { 0 } else { 34 };
            let avail = g.tiles[i].2 as i32 - pad - 20;
            l.set_ellipsize(gtk::pango::EllipsizeMode::None);
            l.set_max_width_chars(-1);
            l.set_label(&f.value);
            if l.measure(gtk::Orientation::Horizontal, -1).1 > avail {
                l.set_label(&crate::mini::short(f));
                l.set_tooltip_text(Some(&f.value));
            } else {
                l.set_tooltip_text(None);
            }
            if l.measure(gtk::Orientation::Horizontal, -1).1 > avail {
                let m = l.pango_context().metrics(None, None);
                let cw = (m.approximate_char_width() as f64 / gtk::pango::SCALE as f64).max(1.0);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                l.set_max_width_chars(((avail as f64 / cw).floor() as i32).max(4));
            }
        }
    }

    /// On half a screen the title block keeps its kicker, title and artist (or
    /// "year · app" line); the credits, bio, facts and full-screen button go.
    fn show_head_parts(&self, h: &Head) {
        let full = !self.is_half() && !self.compact.get();
        self.credits.set_visible(full && !h.credits.is_empty() && h.line.is_empty());
        self.facts.set_visible(full && !h.facts.is_empty());
        self.show_shed();
    }

    fn place_cover(&self) {
        let cover = self.cover.borrow().clone();
        let l = self.layout.get();
        let compact = self.compact.get();
        let split = matches!(l, Layout::Split | Layout::SplitSlim) && self.split_pic.is_visible();
        // The big cover needs 90 px under the title block; a shorter window
        // shows it by the title instead.
        let room = self.cover_room.get() - 16.0;
        let half = self.is_half();
        let big = cover.is_some() && !split && l == Layout::SlimBig && !compact && !half;
        let by_title = cover.is_some() && !split && !half && (l.cover_by_title() || l == Layout::Bar || compact || (big && room < 90.0));
        let big = big && room >= 90.0;
        let in_rows = cover.is_some() && !split && l == Layout::Rows && !compact && !half;
        let h = self.root.height().max(600) as f64;
        let sz = if compact { 96.0 } else { (h * 0.16).clamp(96.0, 150.0) };
        self.sleeve.set(cover.clone().filter(|_| by_title));
        self.sleeve.area.set_visible(by_title);
        self.sleeve.fit_height(sz, sz * 1.2);
        self.bigart.set(cover.clone().filter(|_| big));
        self.bigart.area.set_visible(big);
        self.bigart.fit_height((h * 0.27).clamp(120.0, 250.0).min(room), 300.0);
        self.rows_art.set(cover.clone().filter(|_| in_rows));
        self.rows_art.area.set_visible(in_rows);
        self.rows_art.fit_height(150.0, 180.0);
    }

    /// The four sound choices, laid out for the current layout.
    pub fn set_fields(self: &Rc<Self>, fields: Vec<Field>, tech: &str) {
        self.tech.set_label(tech);
        self.bar_sub.set_label(&self.readout.label());
        self.remote.set_fields(&fields);
        self.remote.set_readout(&self.readout.label(), tech);
        self.strip.set_fields(&fields);
        self.strip.set_readout(&self.readout.label());
        self.shoulder.set_fields(&fields);
        self.shoulder.set_readout(&self.readout.label());
        if *self.fields.borrow() == fields {
            return;
        }
        let words = |f: &[Field]| f.iter().map(|x| (x.label.clone(), x.value.clone())).collect::<Vec<_>>();
        if words(&self.fields.borrow()) != words(&fields) {
            self.dock_fit.set(0);
            self.dock_need.borrow_mut().clear();
        }
        self.fill_fields(fields);
        self.fit_dock_line();
    }

    /// Lays the same choices out again (a new layout, or the slim dock squeezed).
    fn refill(self: &Rc<Self>) {
        let f = self.fields.borrow().clone();
        self.fill_fields(f);
    }

    /// From inside a resize: lays the choices out again once it's done.
    fn refill_later(self: &Rc<Self>) {
        let wk = Rc::downgrade(self);
        glib::idle_add_local_once(move || if let Some(s) = wk.upgrade() { s.refill(); s.fit_dock_line(); });
    }

    fn fit_dock_later(self: &Rc<Self>) {
        let wk = Rc::downgrade(self);
        glib::idle_add_local_once(move || if let Some(s) = wk.upgrade() { s.fit_dock_line(); });
    }

    /// Keeps the slim dock on one line beside the controls, as in the design:
    /// as the window narrows, the short phrases ("close" for "close, like the
    /// studio"), then closer together, then no labels (they become tooltips),
    /// then words cut short.
    fn fit_dock_line(self: &Rc<Self>) {
        if !self.dock_line.is_visible() || !self.low_sw.is_mapped() {
            return;
        }
        let avail = self.low_sw.width() - self.ctl.measure(gtk::Orientation::Horizontal, -1).1 - self.low.spacing();
        for _ in 0..6 {
            let need = self.dock_line.measure(gtk::Orientation::Horizontal, -1).1;
            let lv = self.dock_fit.get();
            if need > avail && lv < 4 {
                self.dock_need.borrow_mut().push(need);
                self.dock_fit.set(lv + 1);
            } else if lv > 0 && self.dock_need.borrow().last().is_some_and(|&n| n + 8 <= avail) {
                self.dock_need.borrow_mut().pop();
                self.dock_fit.set(lv - 1);
            } else {
                return;
            }
            self.refill();
        }
    }

    fn fill_fields(self: &Rc<Self>, fields: Vec<Field>) {
        *self.fields.borrow_mut() = fields.clone();
        for b in [&self.dock_line, &self.rows_fields, &self.bar_fields] {
            while let Some(c) = b.first_child() {
                b.remove(&c);
            }
        }
        let l = self.layout.get();
        let compact = self.compact.get();
        let half = self.is_half();
        for t in &self.tiles {
            while let Some(c) = t.first_child() {
                t.remove(&c);
            }
        }
        if half {
            // Half a screen: each choice is a tile (its label, then its word), placed by the theme.
            self.rows.set_visible(false);
            self.bar.set_visible(false);
            self.dock_line.set_visible(false);
            for (t, f) in self.tiles.iter().zip(fields.iter()) {
                t.append(&lbl(&f.label.to_uppercase(), &["f-l"]));
                let v = self.value_widget(f, false);
                v.set_halign(gtk::Align::Start);
                t.append(&v);
            }
            self.place_half();
            return;
        }
        let rows = (l.rows() || compact) && !fields.is_empty();
        let bar = l == Layout::Bar && !compact;
        let slim = !rows && !bar;
        self.rows.set_visible(rows);
        self.bar.set_visible(bar);
        self.dock_line.set_visible(slim);
        let squeeze = if slim { self.dock_fit.get() } else { 0 };
        if slim {
            self.dock_line.append(&lbl("SOUND", &["soundtag"]));
        }
        if squeeze >= 2 { self.dock_line.add_css_class("tight") } else { self.dock_line.remove_css_class("tight") }
        self.hint.set_margin_bottom(if slim { 15 } else { 6 });
        let pairs = rows && compact && self.pairs.get();
        let mut line: Option<gtk::Box> = None;
        for (i, f) in fields.iter().enumerate() {
            let short;
            let f = if squeeze >= 1 && !f.chip && f.value.contains(", ") {
                short = Field { value: f.value.split(", ").next().unwrap_or_default().to_string(), ..f.clone() };
                &short
            } else { f };
            let value = self.value_widget(f, squeeze >= 4);
            if rows {
                let row = gtk::Box::builder().spacing(12).css_classes(["fd"]).build();
                let l = lbl(&f.label.to_uppercase(), &["f-l"]);
                l.set_width_chars(if pairs { 7 } else { 10 });
                l.set_valign(gtk::Align::Center);
                row.append(&l);
                row.append(&value);
                if pairs {
                    // Two to a line: the first starts a line, the second joins it.
                    match line.take() {
                        Some(ln) => {
                            row.set_hexpand(true);
                            ln.append(&row);
                        }
                        None => {
                            let ln = gtk::Box::builder().spacing(28).build();
                            row.set_size_request(270, -1);
                            ln.append(&row);
                            self.rows_fields.append(&ln);
                            line = Some(ln);
                        }
                    }
                } else {
                    self.rows_fields.append(&row);
                }
            } else if bar {
                let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).css_classes(["fd"]).valign(gtk::Align::Center).build();
                col.append(&lbl(&f.label.to_uppercase(), &["f-l"]));
                col.append(&value);
                self.bar_fields.append(&col);
            } else {
                // One choice: its label and its word, a thin rule after it.
                let sf = gtk::Box::builder().spacing(8).css_classes(["sf"]).valign(gtk::Align::Center).build();
                if i + 1 == fields.len() {
                    sf.add_css_class("last");
                }
                // The label sits on the word's baseline, as in the design.
                value.set_valign(gtk::Align::BaselineFill);
                if squeeze >= 3 {
                    value.set_tooltip_text(Some(&f.label));
                } else {
                    let l = lbl(&f.label.to_uppercase(), &["f-l"]);
                    l.set_valign(gtk::Align::BaselineFill);
                    sf.append(&l);
                }
                sf.append(&value);
                self.dock_line.append(&sf);
            }
        }
        if rows {
            self.tech.unparent_if_any();
            self.rows_fields.append(&self.tech);
            self.tech.set_margin_top(8);
        }
    }

    fn value_widget(self: &Rc<Self>, f: &Field, cut: bool) -> gtk::Widget {
        let fixed = f.options.is_empty();
        // Baseline-aligned all the way down, so the dock's labels can sit on the word's baseline.
        let inner = gtk::Box::builder().valign(gtk::Align::BaselineFill).build();
        let v = gtk::Label::builder().label(&f.value).valign(gtk::Align::BaselineFill)
            .css_classes(if f.chip { vec![] } else { vec!["v".to_string()] }).build();
        if cut {
            v.set_ellipsize(gtk::pango::EllipsizeMode::End);
            v.set_width_chars(4);
        }
        inner.append(&v);
        if !fixed && !f.chip {
            inner.append(&chevron(&v));
        }
        if fixed {
            let b = gtk::Box::builder().css_classes(["f-v", "fixed"]).valign(gtk::Align::Center).build();
            let bt = gtk::Button::builder().child(&inner).can_target(false).valign(gtk::Align::BaselineFill).build();
            b.append(&bt);
            return b.upcast();
        }
        let pop = gtk::Popover::builder().css_classes(["choices"]).has_arrow(false).position(gtk::PositionType::Top).build();
        let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_top(6).margin_bottom(6)
            .margin_start(6).margin_end(6).build();
        col.append(&lbl(&f.label.to_uppercase(), &["head"]));
        for (id, phrase, sub) in &f.options {
            let bx = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
            bx.append(&lbl(phrase, &[]));
            if !sub.is_empty() {
                bx.append(&lbl(sub, &["sub"]));
            }
            let b = gtk::Button::builder().child(&bx).build();
            if *id == f.current {
                b.add_css_class("cur");
            }
            let key = format!("{}:{}", f.key, id);
            let wk = Rc::downgrade(self);
            let pw = pop.downgrade();
            b.connect_clicked(move |_| {
                if let Some(p) = pw.upgrade() { p.popdown() }
                if let Some(s) = wk.upgrade() { s.pick(&key) }
            });
            col.append(&b);
        }
        pop.set_child(Some(&col));
        let mb = gtk::MenuButton::builder().child(&inner).popover(&pop).valign(gtk::Align::BaselineFill).build();
        if f.chip {
            mb.add_css_class("chip");
            let b = gtk::Box::builder().valign(gtk::Align::Center).build();
            b.append(&mb);
            return b.upcast();
        }
        let b = gtk::Box::builder().css_classes(["f-v"]).valign(gtk::Align::Center).build();
        b.append(&mb);
        b.upcast()
    }

    /// The title block's and the controls' boxes on the page (for the self-test).
    pub fn head_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let b = self.head.compute_bounds(&self.root)?;
        Some((b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64))
    }

    pub fn dock_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let b = self.low_sw.compute_bounds(&self.root)?;
        Some((b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64))
    }

    /// The word a sound choice shows now.
    pub fn field_value(&self, key: &str) -> Option<String> {
        self.fields.borrow().iter().find(|f| f.key == key).map(|f| f.value.clone())
    }

    /// The keys of the sound choices showing now, in order.
    pub fn field_keys(&self) -> Vec<String> {
        self.fields.borrow().iter().map(|f| f.key.to_string()).collect()
    }

    /// Half a screen: each tile's box and its word's box, in `to`'s pixels.
    pub fn tile_rects(&self, to: &impl IsA<gtk::Widget>) -> Vec<((f64, f64, f64, f64), (f64, f64, f64, f64))> {
        let r = |b: gtk::graphene::Rect| (b.x() as f64, b.y() as f64, b.width() as f64, b.height() as f64);
        self.tiles.iter().filter(|t| t.is_visible())
            .filter_map(|t| Some((r(t.compute_bounds(to)?), r(t.last_child()?.first_child()?.compute_bounds(to)?))))
            .collect()
    }

    /// The labels of the sound choices showing now (for the self-test).
    pub fn field_labels(&self) -> Vec<String> {
        self.fields.borrow().iter().map(|f| f.label.clone()).collect()
    }

    pub fn field_values(&self) -> Vec<String> {
        self.fields.borrow().iter().map(|f| f.value.clone()).collect()
    }

    /// Puts the video over the screen drawn in the room (page pixels).
    pub fn place_video(&self, r: Option<(f64, f64, f64, f64)>) {
        match r {
            Some((x, y, w, h)) if w > 8.0 && h > 8.0 && self.video.paintable().is_some() => {
                let (pw, ph) = (self.root.width() as f64, self.root.height() as f64);
                let m = [x.round() as i32, y.round() as i32, (pw - x - w).round().max(0.0) as i32, (ph - y - h).round().max(0.0) as i32];
                if [self.video.margin_start(), self.video.margin_top(), self.video.margin_end(), self.video.margin_bottom()] != m {
                    self.video.set_margin_start(m[0]);
                    self.video.set_margin_top(m[1]);
                    self.video.set_margin_end(m[2]);
                    self.video.set_margin_bottom(m[3]);
                }
                self.video.set_visible(true);
            }
            _ => self.video.set_visible(false),
        }
    }

    pub fn shows(&self) -> (bool, bool, bool, bool, bool) {
        (self.dock_line.is_visible(), self.rows.is_visible(), self.bar.is_visible(), self.sleeve.area.is_visible(), self.bigart.area.is_visible())
    }
}

trait Unparent {
    fn unparent_if_any(&self);
}

impl<W: IsA<gtk::Widget>> Unparent for W {
    fn unparent_if_any(&self) {
        if let Some(p) = self.parent() {
            if let Some(b) = p.downcast_ref::<gtk::Box>() {
                b.remove(self);
            } else {
                self.unparent();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half a screen at its usual sizes: the tiles stay apart, under the title,
    /// above the controls and inside the window, in every theme.
    #[test]
    fn half_screen_layouts_keep_apart() {
        let meets = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
        for theme in ["oys", "blueprint", "screenprint", "callouts"] {
            for (w, h) in [(900.0, 600.0), (960.0, 1032.0), (1000.0, 650.0), (1199.0, 900.0), (1100.0, 760.0)] {
                let (ch, th, head_h) = (85.0, 72.0, 150.0);
                let g = half_geo(theme, w, h, ch, th, head_h);
                let head = (36.0, g.head_top as f64, w - 72.0, head_h);
                for (i, t) in g.tiles.iter().enumerate() {
                    assert!(t.0 >= 0.0 && t.0 + t.2 <= w && t.1 + t.3 <= h - ch + 1.0, "{theme} {w}x{h}: tile {i} {t:?}");
                    if theme != "screenprint" {
                        assert!(!meets(*t, head), "{theme} {w}x{h}: tile {i} {t:?} on the title");
                    }
                    for (j, o) in g.tiles.iter().enumerate().skip(i + 1) {
                        assert!(!meets(*t, *o), "{theme} {w}x{h}: tiles {i} and {j}");
                    }
                }
                if let Some((y, bh)) = g.band {
                    assert!(g.tiles.iter().all(|t| t.1 >= y + bh), "{theme} {w}x{h}: the band runs into the tiles");
                }
                assert!(g.room.2 > 200.0 && g.room.3 > 150.0, "{theme} {w}x{h}: room {:?}", g.room);
            }
        }
    }
}
