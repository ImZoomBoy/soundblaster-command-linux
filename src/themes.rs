//! Three alternative looks ("themes") for the listening room on the full-window "now playing" page:
//! Blueprint (a top-down floor plan in white line work), Screenprint (a one-point-perspective room
//! in flat red, black and white print colours) and Callouts (an isometric room with red leader
//! lines to small labels). The main look ("Over your shoulder") lives in `oys`; this module uses its
//! types as they are and draws plain cairo, with no per-frame image processing.
//!
//! Notes for the caller:
//! * `draw` returns the speaker hit boxes (ids from `oys::ORDER`), `you`, `screen_rect` (only when a
//!   screen is drawn) and `animating` (playing, or an easing still on its way).
//! * Easing (speaker brightness, headphone ring, zone size, wall distance) is kept in a small
//!   thread-local, because `oys::Smooth` has no fields; the first frame snaps to its target.
//! * Fonts are cairo toy fonts: "Anton" for big words, "Archivo Narrow" for small capitals; fontconfig
//!   falls back to Sans when they are missing, and text is measured and fitted so it never overflows.
#![allow(dead_code)]

use crate::oys::{Layout, ORDER, Place, Scene, Screen, Smooth};
use gtk::cairo::{
    Context, Extend, Filter, FontSlant, FontWeight, Format, ImageSurface, LineCap, LineJoin, LinearGradient,
    Matrix, SurfacePattern,
};
use std::cell::Cell;
use std::f64::consts::{PI, TAU};

type Rgb = (f64, f64, f64);

/// The page red, #e3262f.
const RED: Rgb = (0.8902, 0.1490, 0.1843);
const WHITE: Rgb = (1.0, 1.0, 1.0);
const BLACK: Rgb = (0.0, 0.0, 0.0);

// ---------------------------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------------------------

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// The page red darkened to `m` (0 = black, 1 = the page red).
fn kred(m: f64) -> Rgb {
    (RED.0 * m, RED.1 * m, RED.2 * m)
}

fn src(cr: &Context, c: Rgb, a: f64) {
    cr.set_source_rgba(c.0, c.1, c.2, a.clamp(0.0, 1.0));
}

fn fill(cr: &Context) {
    let _ = cr.fill();
}

fn stroke(cr: &Context) {
    let _ = cr.stroke();
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn rrect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, PI * 1.5);
    cr.close_path();
}

fn poly(cr: &Context, pts: &[(f64, f64)]) {
    if let Some(&(x, y)) = pts.first() {
        cr.move_to(x, y);
        for &(x, y) in &pts[1..] {
            cr.line_to(x, y);
        }
        cr.close_path();
    }
}

fn fill_poly(cr: &Context, pts: &[(f64, f64)], c: Rgb, a: f64) {
    poly(cr, pts);
    src(cr, c, a);
    fill(cr);
}

/// Dots along a path (set before the stroke): `gap` between centres, `dia` across.
fn dotted(cr: &Context, gap: f64, dia: f64) {
    cr.set_line_cap(LineCap::Round);
    cr.set_line_width(dia);
    cr.set_dash(&[0.001, gap], 0.0);
}

fn solid_line(cr: &Context, w: f64) {
    cr.set_dash(&[], 0.0);
    cr.set_line_cap(LineCap::Butt);
    cr.set_line_join(LineJoin::Miter);
    cr.set_line_width(w);
}

fn display_font(cr: &Context, size: f64) {
    cr.select_font_face("Anton", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(size);
}

fn small_font(cr: &Context, size: f64) {
    cr.select_font_face("Archivo Narrow", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(size);
}

fn text_w(cr: &Context, s: &str, tracking: f64) -> f64 {
    if tracking == 0.0 {
        return cr.text_extents(s).map(|e| e.x_advance()).unwrap_or(0.0);
    }
    let mut w = 0.0;
    let mut n = 0;
    for ch in s.chars() {
        let mut b = [0u8; 4];
        w += cr.text_extents(ch.encode_utf8(&mut b)).map(|e| e.x_advance()).unwrap_or(0.0);
        n += 1;
    }
    w + tracking * (n.max(1) - 1) as f64
}

/// Text with letter spacing. `align`: -1 left, 0 centre, 1 right of `x`; `y` is the baseline.
fn text_at(cr: &Context, s: &str, x: f64, y: f64, align: i32, tracking: f64) {
    let tw = text_w(cr, s, tracking);
    let mut x0 = match align {
        a if a < 0 => x,
        0 => x - tw / 2.0,
        _ => x - tw,
    };
    if tracking == 0.0 {
        cr.move_to(x0, y);
        let _ = cr.show_text(s);
        return;
    }
    for ch in s.chars() {
        let mut b = [0u8; 4];
        let t = ch.encode_utf8(&mut b);
        cr.move_to(x0, y);
        let _ = cr.show_text(t);
        x0 += cr.text_extents(t).map(|e| e.x_advance()).unwrap_or(0.0) + tracking;
    }
}

/// Small capitals with a dark halo, so they read over dots and lines.
fn label_halo(cr: &Context, s: &str, x: f64, y: f64, align: i32, tracking: f64, c: Rgb, a: f64, halo: f64) {
    if halo > 0.0 {
        src(cr, BLACK, 0.85);
        cr.set_line_width(halo);
        cr.set_line_join(LineJoin::Round);
        let tw = text_w(cr, s, tracking);
        let mut x0 = match align {
            a if a < 0 => x,
            0 => x - tw / 2.0,
            _ => x - tw,
        };
        for ch in s.chars() {
            let mut b = [0u8; 4];
            let t = ch.encode_utf8(&mut b);
            cr.move_to(x0, y);
            cr.text_path(t);
            stroke(cr);
            x0 += cr.text_extents(t).map(|e| e.x_advance()).unwrap_or(0.0) + tracking;
        }
    }
    src(cr, c, a);
    text_at(cr, s, x, y, align, tracking);
}

/// A tiled dot pattern: one dot of radius `r` per `step` x `step` user-space cell, crisp at scale `k`.
fn halftone(col: Rgb, step: f64, r: f64, k: f64) -> Option<SurfacePattern> {
    let n = (k * 1.6).ceil().max(2.0);
    let px = (step * n).round().max(2.0);
    let s = ImageSurface::create(Format::ARgb32, px as i32, px as i32).ok()?;
    {
        let c = Context::new(&s).ok()?;
        src(&c, col, 1.0);
        c.arc(px / 2.0, px / 2.0, r * px / step, 0.0, TAU);
        fill(&c);
    }
    let p = SurfacePattern::create(&s);
    p.set_extend(Extend::Repeat);
    p.set_filter(Filter::Good);
    let sc = px / step;
    p.set_matrix(Matrix::new(sc, 0.0, 0.0, sc, 0.0, 0.0));
    Some(p)
}

fn spread01(sc: &Scene) -> f64 {
    let s = sc.spread;
    (if s > 1.5 { s / 100.0 } else { s }).clamp(0.0, 1.0)
}

/// The colour of the sound zone: the tone swatch while SBX is on, the page red otherwise.
fn zone_colour(sc: &Scene) -> Rgb {
    if !sc.sbx_on {
        return RED;
    }
    let (mut r, mut g, mut b) = sc.tone_rgb;
    if r > 1.5 || g > 1.5 || b > 1.5 {
        r /= 255.0;
        g /= 255.0;
        b /= 255.0;
    }
    (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
}

fn level(sc: &Scene, i: usize) -> f64 {
    if sc.playing { sc.levels[i].clamp(0.0, 1.0) } else { 0.0 }
}

/// Fit a block of `half_w` local units across the page: returns (centre x, scale). The room sits at
/// x = w * 0.70, never left of `left_clip` (nor, without a clip, into the text on the left `text_frac`).
fn fit(w: f64, clip: f64, half_w: f64, k0: f64, text_frac: f64) -> (f64, f64) {
    if text_frac < 0.0 {
        // The room's own box (half a screen): centred, as big as fits.
        return (w * 0.5, k0.min((w - 20.0) / (2.0 * half_w)));
    }
    let pad = 10.0;
    let left = clip.max(w * text_frac) + pad;
    let right = w - pad;
    let mut k = k0;
    if 2.0 * half_w * k > right - left {
        k = (right - left) / (2.0 * half_w);
    }
    let hw = half_w * k;
    let cx = (w * 0.70).clamp(left + hw, (right - hw).max(left + hw));
    (cx, k)
}

// ---------------------------------------------------------------------------------------------
// easing (thread-local, since `Smooth` has no fields)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Ez {
    init: bool,
    /// 0..1 per speaker: playing and not on headphones.
    act: [f64; 6],
    /// 0..1 headphones.
    hp: f64,
    /// Zone size factor (1 = the smallest "on" size).
    rad: f64,
    /// Zone half angle: PI is a full circle.
    half: f64,
    /// Distance of the front wall (Screenprint), in world units.
    wz: f64,
}

thread_local! {
    static EZ: Cell<Ez> = const { Cell::new(Ez { init: false, act: [0.0; 6], hp: 0.0, rad: 1.0, half: PI, wz: 4.6 }) };
}

fn rad_factor(sc: &Scene) -> f64 {
    if sc.headphones {
        0.25
    } else if !sc.sbx_on {
        0.75
    } else {
        1.0 + 0.9 * (sc.size / 2.0).clamp(0.0, 1.0) * (0.85 + 0.3 * spread01(sc))
    }
}

fn wall_distance(sc: &Scene) -> f64 {
    match sc.place {
        Place::Plain => 4.6,
        Place::Studio => 4.4 + 1.2 * sc.studio_grow.clamp(0.0, 1.0),
        Place::Concert => 7.5,
        Place::Stadium => 11.5,
        Place::Cinema => 6.5,
        Place::Gaming => 6.0,
    }
}

/// Moves the eased values towards the scene and says whether they are still moving.
fn ease(sc: &Scene, dt: f64) -> (Ez, bool) {
    let mut e = EZ.with(|c| c.get());
    let tgt_act: [f64; 6] = std::array::from_fn(|i| if sc.active[i] && !sc.headphones { 1.0 } else { 0.0 });
    let tgt_hp = if sc.headphones { 1.0 } else { 0.0 };
    let tgt_rad = rad_factor(sc);
    let tgt_half = if sc.around || sc.headphones { PI } else { (30.0 + spread01(sc) * 55.0).to_radians() };
    let tgt_wz = wall_distance(sc);
    if !e.init {
        e = Ez { init: true, act: tgt_act, hp: tgt_hp, rad: tgt_rad, half: tgt_half, wz: tgt_wz };
        EZ.with(|c| c.set(e));
        return (e, false);
    }
    let k = 1.0 - (-dt.clamp(0.0, 0.1) * 6.0).exp();
    let mut moving = false;
    let mut step = |v: &mut f64, t: f64, eps: f64| {
        *v += (t - *v) * k;
        if (t - *v).abs() < eps {
            *v = t;
        } else {
            moving = true;
        }
    };
    for i in 0..6 {
        step(&mut e.act[i], tgt_act[i], 0.004);
    }
    step(&mut e.hp, tgt_hp, 0.004);
    step(&mut e.rad, tgt_rad, 0.003);
    step(&mut e.half, tgt_half, 0.004);
    step(&mut e.wz, tgt_wz, 0.01);
    EZ.with(|c| c.set(e));
    (e, moving)
}

// ---------------------------------------------------------------------------------------------
// shared drawing: screens, neon, covers
// ---------------------------------------------------------------------------------------------

fn surf_size(s: &ImageSurface) -> (f64, f64) {
    (s.width().max(1) as f64, s.height().max(1) as f64)
}

/// Draws `s` scaled to fit (contain) inside the box, centred.
fn draw_surface_fit(cr: &Context, s: &ImageSurface, x: f64, y: f64, w: f64, h: f64) {
    let (sw, sh) = surf_size(s);
    let k = (w / sw).min(h / sh);
    let (dw, dh) = (sw * k, sh * k);
    cr.save().ok();
    cr.translate(x + (w - dw) / 2.0, y + (h - dh) / 2.0);
    cr.scale(k, k);
    let _ = cr.set_source_surface(s, 0.0, 0.0);
    cr.rectangle(0.0, 0.0, sw, sh);
    fill(cr);
    cr.restore().ok();
}

/// The alpha of `s` painted in a colour, scaled into the box (used for logos).
fn mask_surface_box(cr: &Context, s: &ImageSurface, x: f64, y: f64, w: f64, h: f64, c: Rgb, a: f64) {
    let (sw, sh) = surf_size(s);
    cr.save().ok();
    cr.translate(x, y);
    cr.scale(w / sw, h / sh);
    src(cr, c, a);
    let _ = cr.mask_surface(s, 0.0, 0.0);
    cr.restore().ok();
}

/// Largest font size (up to `max`) at which `s` fits in `w` wide.
fn fit_font(cr: &Context, family_display: bool, s: &str, w: f64, max: f64, tracking: f64) -> f64 {
    if family_display {
        display_font(cr, 100.0);
    } else {
        small_font(cr, 100.0);
    }
    let tw = text_w(cr, s, tracking * 100.0 / max.max(1.0)).max(1.0);
    (100.0 * w / tw).min(max)
}

/// A wall screen made smaller (about the middle of its bottom edge) so its top
/// stays below the page's top-right corner, when the two would overlap.
fn below_corner(sc: &Scene, rect: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let Some((cx, cy, cw, ch)) = sc.corner else { return rect };
    let (x, y, w, h) = rect;
    let top = cy + ch + 14.0;
    if x + w < cx - 10.0 || x > cx + cw || y >= top || y + h <= top + 40.0 {
        return rect;
    }
    // Small enough to clear it either below or to the left, whichever keeps it bigger.
    let below = (y + h - top) / h;
    let left = (cx - 12.0 - (x + w / 2.0)) / (w / 2.0);
    let k = below.max(left).clamp(0.45, 1.0);
    let (nw, nh) = (w * k, h * k);
    (x + (w - nw) / 2.0, y + h - nh, nw, nh)
}

/// What a screen shows, drawn into the (already dark) rectangle. `Video` stays dark: the app overlays it.
fn draw_screen_content(cr: &Context, screen: &Screen, x: f64, y: f64, w: f64, h: f64) {
    match screen {
        Screen::None | Screen::Video => {}
        Screen::Logo(s) => draw_surface_fit(cr, s, x + w * 0.12, y + h * 0.14, w * 0.76, h * 0.72),
        Screen::Picture(s) => draw_surface_fit(cr, s, x, y, w, h),
        Screen::Title(t) => {
            let t = t.to_uppercase();
            let size = fit_font(cr, true, &t, w * 0.8, h * 0.42, 0.0);
            display_font(cr, size);
            src(cr, WHITE, 0.96);
            text_at(cr, &t, x + w / 2.0, y + h / 2.0 + size * 0.36, 0, 0.0);
        }
        Screen::Score { home, away, home_badge, away_badge } => {
            let bs = h * 0.5;
            let (cxh, cxa) = (x + w * 0.27, x + w * 0.73);
            let by = y + h * 0.5 - bs * 0.62;
            for (cx0, badge, name) in [(cxh, home_badge, home), (cxa, away_badge, away)] {
                if let Some(b) = badge {
                    draw_surface_fit(cr, b, cx0 - bs / 2.0, by, bs, bs);
                } else {
                    cr.new_sub_path();
                    cr.arc(cx0, by + bs / 2.0, bs * 0.42, 0.0, TAU);
                    src(cr, WHITE, 0.14);
                    cr.fill_preserve().ok();
                    solid_line(cr, 2.0);
                    src(cr, WHITE, 0.7);
                    stroke(cr);
                    display_font(cr, bs * 0.5);
                    src(cr, WHITE, 0.95);
                    let ini: String = name.chars().take(1).collect::<String>().to_uppercase();
                    text_at(cr, &ini, cx0, by + bs / 2.0 + bs * 0.18, 0, 0.0);
                }
                let nm = name.to_uppercase();
                let size = fit_font(cr, false, &nm, w * 0.38, h * 0.1, 1.0);
                small_font(cr, size);
                src(cr, WHITE, 0.9);
                text_at(cr, &nm, cx0, y + h * 0.5 + bs * 0.62 + size * 1.6, 0, size * 0.1);
            }
            display_font(cr, h * 0.2);
            src(cr, RED, 1.0);
            text_at(cr, "V", x + w / 2.0, y + h * 0.5 + h * 0.07, 0, 0.0);
        }
    }
}

/// A neon sign: the logo (alpha mask) or the text in Anton, white tube with a red glow.
/// Fitted into the box centred on (cx, cy). Returns nothing; `t` only adds a faint hum when playing.
fn draw_neon(cr: &Context, sc: &Scene, cx: f64, cy: f64, max_w: f64, max_h: f64, glow: bool) {
    neon_sign(cr, sc.neon_logo, sc.neon_text, sc.playing, sc.t, cx, cy, max_w, max_h, glow);
}

/// The neon sign itself, for the page and Over your shoulder, mini (mini.rs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn neon_sign(cr: &Context, logo: Option<&ImageSurface>, text: &str, playing: bool, t: f64, cx: f64, cy: f64, max_w: f64, max_h: f64, glow: bool) {
    let hum = if playing { 0.93 + 0.07 * (t * 9.0).sin() * (t * 2.3).cos() } else { 1.0 };
    let core = (1.0, 0.93, 0.93);
    if let Some(logo) = logo {
        let (sw, sh) = surf_size(logo);
        let k = (max_w / sw).min(max_h / sh);
        let (dw, dh) = (sw * k, sh * k);
        let (x, y) = (cx - dw / 2.0, cy - dh / 2.0);
        let g = (dh * 0.025).max(1.2);
        for (ring, a) in if glow { [(2.6, 0.10), (1.6, 0.16), (0.8, 0.30)] } else { [(0.0, 0.0); 3] } {
            if a == 0.0 {
                continue;
            }
            for i in 0..10 {
                let ang = i as f64 / 10.0 * TAU;
                mask_surface_box(cr, logo, x + ang.cos() * g * ring, y + ang.sin() * g * ring, dw, dh, RED, a * hum);
            }
        }
        mask_surface_box(cr, logo, x, y, dw, dh, core, hum);
        return;
    }
    let text = text.trim().to_uppercase();
    if text.is_empty() {
        return;
    }
    let size = fit_font(cr, true, &text, max_w, max_h * 0.95, 0.0);
    display_font(cr, size);
    let tw = text_w(cr, &text, 0.0);
    let base = cy + size * 0.36;
    cr.set_line_join(LineJoin::Round);
    cr.set_line_cap(LineCap::Round);
    for (lw, a) in [(size * 0.20, 0.07), (size * 0.12, 0.12), (size * 0.07, 0.22), (size * 0.035, 0.45)] {
        if !glow {
            break;
        }
        cr.move_to(cx - tw / 2.0, base);
        cr.text_path(&text);
        src(cr, RED, a * hum);
        cr.set_line_width(lw);
        stroke(cr);
    }
    src(cr, core, hum);
    text_at(cr, &text, cx, base, 0, 0.0);
}

// ---------------------------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------------------------

/// Draws one of the alternative room looks: `theme` is "blueprint", "screenprint" or "callouts".
/// Returns the speaker hit boxes, the screen rectangle (when a screen is drawn), the listener and
/// whether the page should keep redrawing.
pub fn draw(theme: &str, cr: &Context, w: f64, h: f64, dt: f64, scene: &Scene, _sm: &mut Smooth) -> Layout {
    let (ez, moving) = ease(scene, dt);
    cr.save().ok();
    if scene.left_clip > 0.0 {
        cr.rectangle(scene.left_clip, 0.0, (w - scene.left_clip).max(0.0), h);
        cr.clip();
    }
    cr.set_antialias(gtk::cairo::Antialias::Default);
    let mut layout = match theme {
        "screenprint" => screenprint(cr, w, h, scene, &ez),
        "callouts" => callouts(cr, w, h, scene, &ez),
        _ => blueprint(cr, w, h, scene, &ez),
    };
    cr.restore().ok();
    layout.speakers.sort_by_key(|(id, _)| ORDER.iter().position(|o| o == id).unwrap_or(99));
    layout.animating = scene.playing || moving;
    layout
}

// ---------------------------------------------------------------------------------------------
// 1 · Blueprint: the room as a floor plan
// ---------------------------------------------------------------------------------------------

/// Plan units: the room is 520 x 440 at (40,40); the listener sits at (300,330).
const BP_L: (f64, f64) = (300.0, 330.0);
const BP_NAMES: [&str; 6] = ["FRONT LEFT", "CENTRE", "FRONT RIGHT", "SUBWOOFER", "BACK LEFT", "BACK RIGHT"];
const BP_SUBS: [&str; 6] = ["30\u{b0}", "0\u{b0}", "30\u{b0}", "ON THE FLOOR", "110\u{b0}", "110\u{b0}"];
/// Rotation in degrees for ORDER = FL, FC, FR, LFE, RL, RR.
const BP_ANG: [f64; 6] = [-30.0, 0.0, 30.0, 0.0, -110.0, 110.0];

fn bp_pos(i: usize) -> (f64, f64) {
    if i == 3 {
        return (482.0, 98.0);
    }
    let a = BP_ANG[i].to_radians();
    (BP_L.0 + a.sin() * 205.0, BP_L.1 - a.cos() * 205.0)
}

fn blueprint(cr: &Context, w: f64, h: f64, sc: &Scene, ez: &Ez) -> Layout {
    let (cx, k) = if sc.fill { fit(w, 0.0, 270.0, h * 0.94 / 440.0, -1.0) } else { fit(w, sc.left_clip, 270.0, h * 0.58 / 440.0, 0.44) };
    let cy = h * 0.50;
    let (ox, oy) = (cx - 300.0 * k, cy - 260.0 * k);
    let p = |x: f64, y: f64| (ox + x * k, oy + y * k);
    let mut lay = Layout::default();
    let bass = level(sc, 3);
    let zc = zone_colour(sc);

    cr.save().ok();
    cr.translate(ox, oy);
    cr.scale(k, k);

    // the panel and its grid
    cr.rectangle(40.0, 40.0, 520.0, 440.0);
    src(cr, BLACK, 0.64);
    fill(cr);
    solid_line(cr, 1.0);
    src(cr, WHITE, 0.055);
    for i in 0..=26 {
        let d = 40.0 + i as f64 * 20.0;
        if d <= 560.0 {
            cr.move_to(d, 40.0);
            cr.line_to(d, 480.0);
        }
        if d <= 480.0 {
            cr.move_to(40.0, d);
            cr.line_to(560.0, d);
        }
    }
    stroke(cr);

    // where the sound reaches: a halftone fill and a dotted outline, swelling with the bass
    let rc = 140.0 * ez.rad;
    let full = ez.half >= PI - 0.01;
    let zone_path = |cr: &Context| {
        if full {
            cr.new_sub_path();
            cr.arc(BP_L.0, BP_L.1, rc, 0.0, TAU);
        } else {
            cr.move_to(BP_L.0, BP_L.1);
            cr.arc(BP_L.0, BP_L.1, rc, -PI / 2.0 - ez.half, -PI / 2.0 + ez.half);
            cr.close_path();
        }
    };
    // the halftone: dots on a 9-unit grid, inside the sector, inside the room
    {
        cr.new_path();
        let step = 9.0;
        let n = (rc / step).ceil() as i32 + 1;
        let (gx0, gy0) = (BP_L.0 + step / 2.0, BP_L.1 + step / 2.0);
        for iy in -n..=n {
            for ix in -n..=n {
                let (x, y) = (gx0 + ix as f64 * step, gy0 + iy as f64 * step);
                let (dx, dy) = (x - BP_L.0, y - BP_L.1);
                if dx.hypot(dy) > rc || !(44.0..=556.0).contains(&x) || !(44.0..=476.0).contains(&y) {
                    continue;
                }
                if !full && dx.atan2(-dy).abs() > ez.half {
                    continue;
                }
                cr.new_sub_path();
                cr.arc(x, y, 2.2, 0.0, TAU);
            }
        }
        src(cr, zc, 0.4 + 0.45 * bass);
        fill(cr);
    }
    cr.save().ok();
    cr.rectangle(40.0, 40.0, 520.0, 440.0);
    cr.clip();
    zone_path(cr);
    dotted(cr, 7.0, 3.0);
    src(cr, zc, 0.95);
    stroke(cr);
    cr.restore().ok();

    // walls: dotted sides and back, a solid red front wall with a tab
    dotted(cr, 7.0, 3.2);
    src(cr, WHITE, 0.92);
    cr.move_to(560.0, 40.0);
    cr.line_to(560.0, 480.0);
    cr.line_to(40.0, 480.0);
    cr.line_to(40.0, 40.0);
    stroke(cr);
    let g = LinearGradient::new(0.0, 41.5, 0.0, 67.5);
    g.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.85);
    g.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    let _ = cr.set_source(&g);
    cr.rectangle(40.0, 41.5, 520.0, 26.0);
    fill(cr);
    solid_line(cr, 3.0);
    src(cr, RED, 1.0);
    cr.move_to(40.0, 40.0);
    cr.line_to(560.0, 40.0);
    stroke(cr);
    cr.rectangle(238.0, 43.0, 124.0, 27.0);
    src(cr, BLACK, 0.45);
    fill(cr);
    cr.rectangle(238.0, 40.0, 124.0, 25.0);
    src(cr, RED, 1.0);
    fill(cr);
    display_font(cr, 16.0);
    src(cr, WHITE, 1.0);
    text_at(cr, "SCREEN", 300.0, 59.0, 0, 0.6);

    // you: the sofa from above, the listener, a chevron towards the screen
    cr.set_line_join(LineJoin::Round);
    solid_line(cr, 1.3);
    for (x, y, ww, hh, r) in [
        (232.0, 372.0, 136.0, 20.0, 7.0),
        (232.0, 340.0, 18.0, 52.0, 6.0),
        (350.0, 340.0, 18.0, 52.0, 6.0),
        (252.0, 340.0, 47.0, 32.0, 4.0),
        (301.0, 340.0, 47.0, 32.0, 4.0),
    ] {
        rrect(cr, x, y, ww, hh, r);
        src(cr, (0.04, 0.04, 0.05), 0.92);
        cr.fill_preserve().ok();
        src(cr, WHITE, 0.95);
        stroke(cr);
    }
    cr.arc(BP_L.0, BP_L.1 + 4.0, 10.0, 0.0, TAU);
    src(cr, WHITE, 1.0);
    fill(cr);
    solid_line(cr, 2.0);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    cr.move_to(BP_L.0 - 5.0, BP_L.1 - 8.0);
    cr.line_to(BP_L.0, BP_L.1 - 14.0);
    cr.line_to(BP_L.0 + 5.0, BP_L.1 - 8.0);
    stroke(cr);
    small_font(cr, 9.5);
    label_halo(cr, "YOU", BP_L.0, 409.0, 0, 2.4, WHITE, 0.95, 4.0);
    if ez.hp > 0.01 {
        // headphones: a band and two cups on the listener, and a ring that breathes
        let a = ez.hp;
        let br = 1.0 + 0.06 * (sc.t * 2.0).sin();
        solid_line(cr, 3.0);
        cr.set_line_cap(LineCap::Round);
        src(cr, RED, a);
        cr.new_sub_path();
        cr.arc(BP_L.0, BP_L.1 + 4.0, 14.0, PI, TAU);
        stroke(cr);
        for s in [-1.0, 1.0] {
            rrect(cr, BP_L.0 + s * 14.0 - 2.5, BP_L.1 + 1.0, 5.0, 10.0, 2.0);
            fill(cr);
        }
        dotted(cr, 5.2, 2.4);
        src(cr, RED, a * 0.9);
        cr.arc(BP_L.0, BP_L.1 + 4.0, 26.0 * br, 0.0, TAU);
        stroke(cr);
    }

    // speakers: a box and a cone, the dotted rings round the ones that play
    for i in 0..6 {
        let (x, y) = bp_pos(i);
        let on = ez.act[i];
        let lv = level(sc, i);
        let sub = i == 3;
        let ctr = i == 1;
        cr.save().ok();
        cr.translate(x, y);
        let r_b = if sub { 40.0 } else if ctr { 36.0 } else { 31.0 };
        if on > 0.01 {
            let ph = (sc.t * 0.7 + i as f64 * 0.17).rem_euclid(1.0);
            let live = if sc.playing { lv.max(0.15) } else { 0.0 };
            for j in 0..2 {
                let rr = r_b + 6.0 + j as f64 * 9.0 + 7.0 * lv;
                dotted(cr, 5.4, 2.4);
                src(cr, RED, (1.0 - 0.35 * j as f64) * on * (0.6 + 0.4 * live.max(0.3)));
                cr.new_sub_path();
                cr.arc(0.0, 0.0, rr, 0.0, TAU);
                stroke(cr);
            }
            if sc.playing {
                dotted(cr, 6.0, 2.2);
                src(cr, RED, (1.0 - ph) * 0.55 * live * on);
                cr.new_sub_path();
                cr.arc(0.0, 0.0, r_b + 20.0 + ph * 26.0, 0.0, TAU);
                stroke(cr);
            }
        }
        if !sub {
            cr.rotate(BP_ANG[i].to_radians());
        }
        cr.scale(1.2, 1.2);
        let (bw, bd) = if sub { (40.0, 40.0) } else if ctr { (44.0, 16.0) } else { (26.0, 20.0) };
        let edge_a = 0.44 + 0.56 * on;
        solid_line(cr, 1.3);
        cr.set_line_join(LineJoin::Round);
        rrect(cr, -bw / 2.0, -bd / 2.0, bw, bd, 3.0);
        src(cr, (0.04, 0.04, 0.05), 0.92);
        cr.fill_preserve().ok();
        src(cr, WHITE, edge_a);
        stroke(cr);
        if sub {
            let r = 12.0 * (1.0 + 0.22 * lv);
            cr.arc(0.0, 0.0, r, 0.0, TAU);
            src(cr, WHITE, edge_a);
            cr.set_line_width(1.2 + on);
            stroke(cr);
            cr.arc(0.0, 0.0, 3.4, 0.0, TAU);
            if on > 0.01 {
                src(cr, RED, on);
                fill(cr);
            } else {
                src(cr, WHITE, 0.4);
                cr.set_line_width(1.0);
                stroke(cr);
            }
            rrect(cr, 9.0, -17.0, 8.0, 5.0, 1.5);
            src(cr, WHITE, 0.4);
            cr.set_line_width(1.0);
            stroke(cr);
        } else {
            // the front edge and the cone fanning out of it
            let cw = bw * 0.2;
            let flare = 8.0 * (1.0 + 0.25 * lv);
            cr.move_to(-cw, bd / 2.0);
            cr.line_to(-cw * 1.9, bd / 2.0 + flare);
            cr.line_to(cw * 1.9, bd / 2.0 + flare);
            cr.line_to(cw, bd / 2.0);
            cr.close_path();
            if on > 0.01 {
                src(cr, RED, 0.28 * on);
                cr.fill_preserve().ok();
            }
            src(cr, mix(WHITE, RED, on * 0.85), edge_a);
            cr.set_line_width(1.2 + on * 0.6);
            stroke(cr);
            solid_line(cr, 1.6 + 1.6 * on);
            cr.set_line_cap(LineCap::Round);
            src(cr, mix(WHITE, RED, on), 0.55 + 0.45 * on);
            cr.move_to(-bw / 2.0 + 2.0, bd / 2.0);
            cr.line_to(bw / 2.0 - 2.0, bd / 2.0);
            stroke(cr);
            cr.arc(0.0, -bd / 2.0 + 4.5, 2.2, 0.0, TAU);
            src(cr, mix(WHITE, RED, on), edge_a);
            cr.set_line_width(1.0);
            stroke(cr);
        }
        cr.restore().ok();
        lay.speakers.push((ORDER[i], {
            let (bx, by) = p(x - 30.0, y - 30.0);
            (bx, by, 60.0 * k, 60.0 * k)
        }));
    }

    // names
    small_font(cr, 11.0);
    for i in 0..6 {
        let (x, y) = bp_pos(i);
        let on = ez.act[i];
        let dy = if i >= 4 { 52.0 } else if i == 3 { -44.0 } else { -42.0 };
        let c = mix(WHITE, WHITE, on);
        small_font(cr, 11.0);
        label_halo(cr, BP_NAMES[i], x, y + dy, 0, 1.4, c, 0.45 + 0.55 * on, 4.0);
        small_font(cr, 9.5);
        label_halo(cr, BP_SUBS[i], x, y + dy + 13.0, 0, 1.0, mix(WHITE, RED, on), 0.4 + 0.5 * on, 4.0);
    }
    cr.restore().ok();

    lay.you = p(BP_L.0, BP_L.1 + 4.0);
    lay
}

// ---------------------------------------------------------------------------------------------
// 3 · Callouts: an isometric room, red leader lines to small labels
// ---------------------------------------------------------------------------------------------

const CO_NAMES: [&str; 6] = ["FRONT LEFT", "CENTRE", "FRONT RIGHT", "SUBWOOFER", "BACK LEFT", "BACK RIGHT"];
/// Floor position (x, z) of each speaker in room units; the room is 6 x 6, the listener at z = 0.7.
const CO_LZ: f64 = 0.7;

fn co_pos(i: usize) -> (f64, f64) {
    if i == 3 {
        return (2.25, -2.2);
    }
    let a = BP_ANG[i].to_radians();
    (a.sin() * 2.2, CO_LZ - a.cos() * 2.2)
}

struct Iso {
    ox: f64,
    oy: f64,
    s: f64,
}

impl Iso {
    fn p(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        (self.ox + (x - z) * 0.866 * self.s, self.oy + (x + z) * 0.5 * self.s - y * self.s)
    }
}

/// An isometric box: three visible faces (front-left, front-right, top), each (colour, alpha).
#[allow(clippy::too_many_arguments)]
fn iso_box(cr: &Context, c: &Iso, x: f64, z: f64, bw: f64, bd: f64, bh: f64, y0: f64, fills: [Rgb; 3], line: (Rgb, f64)) {
    let (xa, xb, za, zb, ya, yb) = (x - bw / 2.0, x + bw / 2.0, z - bd / 2.0, z + bd / 2.0, y0, y0 + bh);
    let top = [c.p(xa, yb, za), c.p(xb, yb, za), c.p(xb, yb, zb), c.p(xa, yb, zb)];
    let fx = [c.p(xb, ya, za), c.p(xb, yb, za), c.p(xb, yb, zb), c.p(xb, ya, zb)];
    let fz = [c.p(xa, ya, zb), c.p(xb, ya, zb), c.p(xb, yb, zb), c.p(xa, yb, zb)];
    cr.set_line_join(LineJoin::Round);
    solid_line(cr, 1.3);
    cr.set_line_join(LineJoin::Round);
    for (face, fc) in [(&fz, fills[1]), (&fx, fills[2]), (&top, fills[0])] {
        poly(cr, face);
        src(cr, fc, 1.0);
        cr.fill_preserve().ok();
        src(cr, line.0, line.1);
        stroke(cr);
    }
}

/// A circle of room radius `r` lying on the plane through (x,y,z); `plane` 0 = floor, 1 = the +z face.
fn iso_disc(cr: &Context, c: &Iso, plane: u8, x: f64, y: f64, z: f64, r: f64) {
    let (px, py) = c.p(x, y, z);
    let m = if plane == 0 {
        Matrix::new(0.866 * c.s, 0.5 * c.s, -0.866 * c.s, 0.5 * c.s, px, py)
    } else {
        Matrix::new(0.866 * c.s, 0.5 * c.s, 0.0, -c.s, px, py)
    };
    cr.save().ok();
    cr.transform(m);
    cr.new_sub_path();
    cr.arc(0.0, 0.0, r, 0.0, TAU);
    cr.restore().ok();
}

/// A leader: polyline from the speaker, then an underline carrying the label above it.
#[allow(clippy::too_many_arguments)]
fn leader(cr: &Context, pts: &[(f64, f64)], dir: f64, text: &str, fs: f64, col: Rgb, lw: f64, a: f64) {
    small_font(cr, fs);
    let tracking = fs * 0.12;
    let tw = text_w(cr, text, tracking);
    let (ex, ey) = pts[pts.len() - 1];
    solid_line(cr, lw);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    src(cr, col, a);
    cr.move_to(pts[0].0, pts[0].1);
    for &(x, y) in &pts[1..] {
        cr.line_to(x, y);
    }
    cr.line_to(ex + dir * (tw + fs * 0.9), ey);
    stroke(cr);
    let tx = if dir < 0.0 { ex - fs * 0.45 } else { ex + fs * 0.45 };
    label_halo(cr, text, tx, ey - fs * 0.38, if dir < 0.0 { 1 } else { -1 }, tracking, WHITE, 0.97, 3.2);
}

fn callouts(cr: &Context, w: f64, h: f64, sc: &Scene, ez: &Ez) -> Layout {
    let wall_h = 2.8;
    let fs = (h / 900.0 * 11.5).clamp(9.5, 14.0);
    // the labels hang out past the room by a fixed number of pixels, so fit the room around them
    let (cx, s) = {
        let pad = 10.0;
        let left = if sc.fill { pad } else { sc.left_clip.max(w * 0.44) + pad };
        let right = w - pad;
        let ext = 8.6 * fs + 12.0;
        let mut s = h * if sc.fill { 0.86 } else { 0.60 } / (6.0 + wall_h);
        let avail = (right - left) / 2.0;
        if 5.45 * s + ext > avail {
            s = ((avail - ext) / 5.45).max(8.0);
        }
        let hw = 5.45 * s + ext;
        let at = if sc.fill { 0.5 } else { 0.70 };
        ((w * at).clamp(left + hw, (right - hw).max(left + hw)), s)
    };
    let iso = Iso { ox: cx, oy: h * 0.5 + 1.5 * s, s };
    let mut lay = Layout::default();
    let bass = level(sc, 3);
    let zc = zone_colour(sc);

    // the room: a floor diamond and two back walls, in white lines
    let floor = [iso.p(-3.0, 0.0, -3.0), iso.p(3.0, 0.0, -3.0), iso.p(3.0, 0.0, 3.0), iso.p(-3.0, 0.0, 3.0)];
    let lwall = [iso.p(-3.0, 0.0, -3.0), iso.p(-3.0, 0.0, 3.0), iso.p(-3.0, wall_h, 3.0), iso.p(-3.0, wall_h, -3.0)];
    let bwall = [iso.p(-3.0, 0.0, -3.0), iso.p(3.0, 0.0, -3.0), iso.p(3.0, wall_h, -3.0), iso.p(-3.0, wall_h, -3.0)];
    cr.set_line_join(LineJoin::Round);
    fill_poly(cr, &floor, BLACK, 0.70);
    fill_poly(cr, &lwall, (0.04, 0.04, 0.045), 0.82);
    fill_poly(cr, &bwall, (0.04, 0.04, 0.045), 0.82);

    // the screen: a flat TV set into the back wall, as big as the slanted wall allows
    let wall_px = wall_h * s;
    let tv_w = ((wall_px - 32.0) / (0.5625 + 0.577)).max(40.0);
    let tv_h = tv_w * 0.5625;
    let tv_cx = iso.ox + 3.0 * 0.866 * s;
    let tv_cy = iso.oy - (1.5 + wall_h / 2.0) * s;
    let want_tv = !matches!(sc.screen, Screen::None)
        || matches!(sc.place, Place::Cinema | Place::Gaming | Place::Stadium)
        || sc.neon_logo.is_some()
        || !sc.neon_text.trim().is_empty();
    let tv = (tv_cx - tv_w / 2.0, tv_cy - tv_h / 2.0, tv_w, tv_h);
    if want_tv {
        let (x, y, ww, hh) = tv;
        rrect(cr, x - 5.0, y - 5.0, ww + 10.0, hh + 10.0, 2.5);
        src(cr, (0.02, 0.02, 0.025), 0.97);
        cr.fill_preserve().ok();
        solid_line(cr, 1.5);
        src(cr, WHITE, 0.95);
        stroke(cr);
        cr.save().ok();
        cr.rectangle(x, y, ww, hh);
        cr.clip();
        match &sc.screen {
            Screen::None => draw_neon(cr, sc, x + ww / 2.0, y + hh / 2.0, ww * 0.82, hh * 0.5, false),
            other => draw_screen_content(cr, other, x, y, ww, hh),
        }
        cr.restore().ok();
        solid_line(cr, 3.0);
        cr.set_line_cap(LineCap::Round);
        src(cr, WHITE, 0.95);
        cr.move_to(tv_cx - tv_w * 0.14, y + hh + 5.0 + 4.0);
        cr.line_to(tv_cx + tv_w * 0.14, y + hh + 5.0 + 4.0);
        stroke(cr);
        lay.screen_rect = Some(tv);
    }

    // where the sound reaches: a halftone fill and a dotted red outline on the floor
    let rc = 1.5 * ez.rad;
    let full = ez.half >= PI - 0.01;
    let mut zone: Vec<(f64, f64)> = Vec::new();
    if !full {
        zone.push(iso.p(0.0, 0.0, CO_LZ));
    }
    let n = 72;
    let (a0, a1) = if full { (0.0, TAU) } else { (-ez.half, ez.half) };
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f64 / n as f64;
        zone.push(iso.p(a.sin() * rc, 0.0, CO_LZ - a.cos() * rc));
    }
    cr.save().ok();
    poly(cr, &floor);
    cr.clip();
    poly(cr, &zone);
    if let Some(pat) = halftone(zc, 8.0, 2.1, 1.0) {
        let _ = cr.set_source(&pat);
        cr.save().ok();
        cr.clip_preserve();
        let _ = cr.paint_with_alpha(0.45 + 0.45 * bass);
        cr.restore().ok();
    }
    cr.new_path();
    poly(cr, &zone);
    solid_line(cr, 1.3);
    cr.set_dash(&[4.0, 4.0], 0.0);
    src(cr, zc, 0.95);
    stroke(cr);
    cr.restore().ok();

    // outlines drawn over the fills
    solid_line(cr, 1.6);
    cr.set_line_join(LineJoin::Round);
    src(cr, WHITE, 0.95);
    poly(cr, &floor);
    stroke(cr);
    poly(cr, &lwall);
    stroke(cr);
    poly(cr, &bwall);
    stroke(cr);

    // leaders go under the speakers, so they come out from behind them
    let heads: Vec<(f64, f64)> = (0..6)
        .map(|i| {
            let (x, z) = co_pos(i);
            iso.p(x, if i == 3 { 0.6 } else { 1.32 }, z)
        })
        .collect();
    let rel = |x: f64, y: f64| (iso.ox + x * s, iso.oy + y * s);
    // (label height in room-size units, direction, label x for the long runs)
    let routes: [(f64, f64, Option<f64>); 6] = [
        (-4.7, -1.0, None),
        (-5.7, -1.0, None),
        (-1.75, 1.0, Some(5.45)),
        (-0.95, 1.0, Some(5.45)),
        (f64::NAN, -1.0, Some(-5.45)),
        (2.3, 1.0, None),
    ];
    for i in 0..6 {
        if ez.act[i] < 0.02 {
            continue;
        }
        let lv = level(sc, i);
        let (ax, ay) = heads[i];
        let (ly, dir, far) = routes[i];
        let ly_px = if ly.is_nan() { ay } else { iso.oy + ly * s };
        let ex = if ly.is_nan() { ax } else { ax + dir * (ay - ly_px).abs() };
        let mut pts = vec![(ax, ay), (ex, ly_px)];
        if let Some(fx) = far {
            pts.push((iso.ox + fx * s, ly_px));
        }
        leader(cr, &pts, dir, CO_NAMES[i], fs, RED, 1.0 + 0.8 * lv, 0.95 * ez.act[i]);
    }
    {
        // you: a white leader, so it is not mistaken for a speaker
        let head = iso.p(0.0, 1.05, CO_LZ + 0.25);
        let ly = iso.oy + 0.95 * s;
        let ex = head.0 - (ly - head.1).abs();
        leader(cr, &[head, (ex, ly), (iso.ox - 2.5 * s, ly)], -1.0, "YOU", fs, WHITE, 1.0, 0.75);
    }

    // speakers and the sofa, back to front
    enum Item {
        Spk(usize),
        Sofa,
    }
    let mut items: Vec<(f64, Item)> = (0..6)
        .map(|i| {
            let (x, z) = co_pos(i);
            (x + z, Item::Spk(i))
        })
        .collect();
    items.push((CO_LZ + 0.2, Item::Sofa));
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let sofa_fill = [(0.165, 0.165, 0.18), (0.08, 0.08, 0.09), (0.11, 0.11, 0.12)];
    for (_, it) in items {
        match it {
            Item::Sofa => {
                for (zc0, d, hh) in [(CO_LZ + 0.2, 0.9, 0.45), (CO_LZ + 0.62, 0.18, 0.95)] {
                    iso_box(cr, &iso, 0.0, zc0, 1.2, d, hh, 0.0, sofa_fill, (WHITE, 0.7));
                }
                let head = iso.p(0.0, 1.05, CO_LZ + 0.25);
                cr.new_sub_path();
                cr.arc(head.0, head.1, 9.0 * (s / 60.0).clamp(0.7, 1.4), 0.0, TAU);
                src(cr, WHITE, 1.0);
                fill(cr);
                if ez.hp > 0.01 {
                    let r = 13.0 * (s / 60.0).clamp(0.7, 1.4);
                    solid_line(cr, 3.0);
                    cr.set_line_cap(LineCap::Round);
                    src(cr, RED, ez.hp);
                    cr.new_sub_path();
                    cr.arc(head.0, head.1 + 2.0, r, PI, TAU);
                    stroke(cr);
                    dotted(cr, 5.0, 2.2);
                    src(cr, RED, 0.9 * ez.hp);
                    cr.new_sub_path();
                    cr.arc(head.0, head.1, r * 2.0 * (1.0 + 0.06 * (sc.t * 2.0).sin()), 0.0, TAU);
                    stroke(cr);
                }
            }
            Item::Spk(i) => {
                let (x, z) = co_pos(i);
                let on = ez.act[i];
                let lv = level(sc, i);
                let sub = i == 3;
                let (bw, bd, bh, y0) = if sub { (0.6, 0.6, 0.6, 0.0) } else { (0.38, 0.30, 0.62, 0.7) };
                if !sub {
                    let (bx, by) = iso.p(x, 0.0, z);
                    let (tx, ty) = iso.p(x, y0, z);
                    solid_line(cr, 1.4);
                    src(cr, WHITE, 0.45 + 0.5 * on);
                    cr.move_to(bx, by);
                    cr.line_to(tx, ty);
                    stroke(cr);
                }
                let act = |lo: Rgb, hi: Rgb| mix(lo, hi, on);
                let fills = [
                    act((0.085, 0.085, 0.09), WHITE),
                    act((0.043, 0.043, 0.047), mix(WHITE, RED, 0.10)),
                    act((0.063, 0.063, 0.07), mix(WHITE, RED, 0.22)),
                ];
                iso_box(cr, &iso, x, z, bw, bd, bh, y0, fills, (WHITE, 0.5 + 0.5 * on));
                // the driver on the front-left face
                let r = if sub { 0.17 } else { 0.075 } * (1.0 + 0.25 * lv);
                iso_disc(cr, &iso, 1, x, y0 + if sub { 0.3 } else { 0.4 }, z + bd / 2.0, r);
                if on > 0.02 {
                    src(cr, RED, 1.0);
                    fill(cr);
                } else {
                    solid_line(cr, 1.0);
                    src(cr, WHITE, 0.4);
                    stroke(cr);
                }
                let mut pts = Vec::new();
                for (ix, iy, iz) in [
                    (x - bw / 2.0, y0, z + bd / 2.0),
                    (x + bw / 2.0, y0, z - bd / 2.0),
                    (x + bw / 2.0, y0, z + bd / 2.0),
                    (x - bw / 2.0, y0 + bh, z - bd / 2.0),
                    (x + bw / 2.0, y0 + bh, z + bd / 2.0),
                    (x, 0.0, z),
                ] {
                    pts.push(iso.p(ix, iy, iz));
                }
                let minx = pts.iter().map(|q| q.0).fold(f64::MAX, f64::min) - 6.0;
                let maxx = pts.iter().map(|q| q.0).fold(f64::MIN, f64::max) + 6.0;
                let miny = pts.iter().map(|q| q.1).fold(f64::MAX, f64::min) - 6.0;
                let maxy = pts.iter().map(|q| q.1).fold(f64::MIN, f64::max) + 6.0;
                lay.speakers.push((ORDER[i], (minx, miny, maxx - minx, maxy - miny)));
            }
        }
    }
    // the anchor dots sit on top of everything
    for i in 0..6 {
        if ez.act[i] < 0.02 {
            continue;
        }
        let (ax, ay) = heads[i];
        let lv = level(sc, i);
        cr.new_sub_path();
        cr.arc(ax, ay, 2.8 + 1.6 * lv, 0.0, TAU);
        src(cr, RED, ez.act[i]);
        fill(cr);
        if lv > 0.05 {
            dotted(cr, 4.0, 1.6);
            src(cr, RED, 0.8 * lv);
            cr.new_sub_path();
            cr.arc(ax, ay, 8.0 + 8.0 * lv, 0.0, TAU);
            stroke(cr);
        }
    }
    let _ = rel;
    lay.you = iso.p(0.0, 1.05, CO_LZ + 0.25);
    lay
}

// ---------------------------------------------------------------------------------------------
// 2 · Screenprint: a one-point-perspective room in flat print colours
// ---------------------------------------------------------------------------------------------

const EYE_Y: f64 = 5.2;
const EYE_Z: f64 = 10.4;
/// The listener (and the middle of the speaker circle) in the room.
const SP_L: (f64, f64) = (0.0, 1.25);
const SP_SOFA_Z: f64 = 1.3;
const SP_HALF: f64 = 4.5;
const SP_HEIGHT: f64 = 6.5;
const SP_NEAR: f64 = 6.0;

/// A camera behind the sofa with a vertical image plane (so verticals stay vertical).
#[derive(Clone, Copy)]
struct Cam {
    cx: f64,
    yh: f64,
    f: f64,
}

impl Cam {
    fn p(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        let d = EYE_Z - z;
        (self.cx + self.f * x / d, self.yh + self.f * (EYE_Y - y) / d)
    }
    /// Pixels per world unit at depth z.
    fn sc(&self, z: f64) -> f64 {
        self.f / (EYE_Z - z)
    }
}

/// A horizontal alpha ramp for the big shapes on the left, so the room melts into the text side.
#[derive(Clone, Copy)]
struct Fade {
    x0: f64,
    x1: f64,
}

fn fade_src(cr: &Context, f: Fade, c: Rgb, a: f64) {
    let g = LinearGradient::new(f.x0, 0.0, f.x1.max(f.x0 + 1.0), 0.0);
    g.add_color_stop_rgba(0.0, c.0, c.1, c.2, 0.0);
    g.add_color_stop_rgba(0.4, c.0, c.1, c.2, a * 0.35);
    g.add_color_stop_rgba(1.0, c.0, c.1, c.2, a);
    let _ = cr.set_source(&g);
}

fn fill_fade(cr: &Context, pts: &[(f64, f64)], f: Fade, c: Rgb, a: f64) {
    poly(cr, pts);
    fade_src(cr, f, c, a);
    fill(cr);
}

#[derive(Clone, Copy)]
struct Bounds {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Bounds {
    fn new() -> Self {
        Bounds { x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN }
    }
    fn add(&mut self, p: (f64, f64)) {
        self.x0 = self.x0.min(p.0);
        self.y0 = self.y0.min(p.1);
        self.x1 = self.x1.max(p.0);
        self.y1 = self.y1.max(p.1);
    }
    fn rect(&self, pad: f64) -> (f64, f64, f64, f64) {
        (self.x0 - pad, self.y0 - pad, self.x1 - self.x0 + 2.0 * pad, self.y1 - self.y0 + 2.0 * pad)
    }
}

/// An oriented 3D frame: a centre and a yaw (rotation about the vertical, local +z facing `yaw`).
#[derive(Clone, Copy)]
struct Frame3 {
    c: [f64; 3],
    yaw: f64,
}

impl Frame3 {
    fn w(&self, l: [f64; 3]) -> [f64; 3] {
        let (s, c) = self.yaw.sin_cos();
        [self.c[0] + l[0] * c + l[2] * s, self.c[1] + l[1], self.c[2] - l[0] * s + l[2] * c]
    }
    fn n(&self, l: [f64; 3]) -> [f64; 3] {
        let (s, c) = self.yaw.sin_cos();
        [l[0] * c + l[2] * s, l[1], -l[0] * s + l[2] * c]
    }
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The key light of the print: from above, a little right and in front.
fn lum(n: [f64; 3]) -> f64 {
    let l = [0.45, 0.8, 0.55];
    let len = dot3(l, l).sqrt();
    (dot3(n, l) / len).max(0.0)
}

/// A box with half sizes `h` in `fr`: draws the faces the camera can see. `col` maps a light value
/// (0..1) to a colour; `edge` is an outline colour/alpha.
fn box3(cr: &Context, cam: &Cam, fr: Frame3, h: [f64; 3], col: &dyn Fn(f64) -> Rgb, alpha: f64, edge: Option<(Rgb, f64)>, b: &mut Bounds) {
    let [hx, hy, hz] = h;
    let faces: [([[f64; 3]; 4], [f64; 3]); 5] = [
        ([[-hx, -hy, hz], [hx, -hy, hz], [hx, hy, hz], [-hx, hy, hz]], [0.0, 0.0, 1.0]),
        ([[hx, -hy, -hz], [-hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz]], [0.0, 0.0, -1.0]),
        ([[hx, -hy, hz], [hx, -hy, -hz], [hx, hy, -hz], [hx, hy, hz]], [1.0, 0.0, 0.0]),
        ([[-hx, -hy, -hz], [-hx, -hy, hz], [-hx, hy, hz], [-hx, hy, -hz]], [-1.0, 0.0, 0.0]),
        ([[-hx, hy, hz], [hx, hy, hz], [hx, hy, -hz], [-hx, hy, -hz]], [0.0, 1.0, 0.0]),
    ];
    for (corners, n) in faces {
        let nw = fr.n(n);
        // centre of the face in world space
        let cw = fr.w([n[0] * hx, n[1] * hy, n[2] * hz]);
        let to_eye = [0.0 - cw[0], EYE_Y - cw[1], EYE_Z - cw[2]];
        if dot3(nw, to_eye) <= 0.0 {
            continue;
        }
        let pts: Vec<(f64, f64)> = corners
            .iter()
            .map(|c| {
                let w = fr.w(*c);
                cam.p(w[0], w[1], w[2])
            })
            .collect();
        for p in &pts {
            b.add(*p);
        }
        poly(cr, &pts);
        src(cr, col(lum(nw)), alpha);
        if let Some((ec, ea)) = edge {
            cr.fill_preserve().ok();
            solid_line(cr, 1.1);
            cr.set_line_join(LineJoin::Round);
            src(cr, ec, ea);
            stroke(cr);
        } else {
            fill(cr);
        }
    }
}

/// A circle on a plane of `fr` (z = zl, local), as a projected polygon.
fn disc3(cam: &Cam, fr: Frame3, lx: f64, ly: f64, lz: f64, r: f64, n: usize, b: &mut Bounds) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * TAU;
            let w = fr.w([lx + a.cos() * r, ly + a.sin() * r, lz]);
            let p = cam.p(w[0], w[1], w[2]);
            b.add(p);
            p
        })
        .collect()
}

/// A circle lying on the floor.
fn floor_ring(cam: &Cam, x: f64, z: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * TAU;
            cam.p(x + a.cos() * r, 0.0, z + a.sin() * r)
        })
        .collect()
}

fn shade_on(l: f64) -> Rgb {
    mix(mix(kred(0.62), WHITE, 0.22), WHITE, l.powf(0.8))
}

fn shade_off(l: f64) -> Rgb {
    mix(kred(0.13), kred(0.40), l)
}

/// Draws one speaker; returns its hit box.
fn sp_speaker(cr: &Context, cam: &Cam, i: usize, act: f64, lv: f64, sc: &Scene) -> (f64, f64, f64, f64) {
    let (x, z) = sp_pos(i);
    let yaw = (SP_L.0 - x).atan2(SP_L.1 - z);
    let mut b = Bounds::new();
    let sub = i == 3;
    let ctr = i == 1;
    // a shadow on the floor
    let sh = floor_ring(cam, x, z, if sub { 0.5 } else { 0.32 }, 20);
    fill_poly(cr, &sh, BLACK, 0.30);
    // ripples on the floor while it plays
    if act > 0.02 && sc.playing {
        let ph = (sc.t * 0.8 + i as f64 * 0.21).rem_euclid(1.0);
        let live = lv.max(0.12);
        let ring = floor_ring(cam, x, z, 0.35 + ph * 0.55, 36);
        poly(cr, &ring);
        solid_line(cr, 1.6 + 2.0 * (1.0 - ph));
        src(cr, WHITE, (1.0 - ph) * 0.7 * live * act);
        stroke(cr);
    }
    let col = move |l: f64| mix(shade_off(l), shade_on((l + 0.3 * lv).min(1.0)), act);
    let stand = mix(kred(0.22), WHITE, 0.15 + 0.45 * act);
    let (cy, hh) = if sub { (0.33, [0.31, 0.30, 0.31]) } else if ctr { (0.70, [0.33, 0.125, 0.15]) } else { (1.12, [0.18, 0.29, 0.15]) };
    if !sub {
        let top = if ctr { 0.575 } else { 0.82 };
        let base = floor_ring(cam, x, z, if ctr { 0.14 } else { 0.15 }, 20);
        fill_poly(cr, &base, stand, 1.0);
        for p in &base {
            b.add(*p);
        }
        let (px0, py0) = cam.p(x, 0.0, z);
        let (px1, py1) = cam.p(x, top, z);
        solid_line(cr, (if ctr { 0.05 } else { 0.045 }) * cam.sc(z));
        src(cr, stand, 1.0);
        cr.move_to(px0, py0);
        cr.line_to(px1, py1);
        stroke(cr);
    }
    let bf = Frame3 { c: [x, cy, z], yaw };
    box3(cr, cam, bf, hh, &col, 1.0, Some((kred(0.08), 0.5)), &mut b);
    // drivers on the front face
    let front = hh[2];
    let cone_c = |l: f64| mix(mix(kred(0.10), kred(0.30), l), kred(0.55), 0.0);
    let drv = |lx: f64, ly: f64, r: f64, dust: bool, b: &mut Bounds| {
        let r = r * (1.0 + 0.18 * lv);
        let poly_pts = disc3(cam, bf, lx, ly, front + 0.004, r, 24, b);
        poly(cr, &poly_pts);
        src(cr, cone_c(act), 1.0);
        cr.fill_preserve().ok();
        solid_line(cr, 1.3);
        src(cr, mix(kred(0.45), WHITE, 0.55 * act), 0.9);
        stroke(cr);
        if dust {
            let d = disc3(cam, bf, lx, ly, front + 0.006, r * 0.32, 12, b);
            fill_poly(cr, &d, mix(kred(0.5), WHITE, 0.4 * act), 0.55);
        }
    };
    if sub {
        drv(0.0, 0.03, 0.2, true, &mut b);
        drv(0.21, -0.22, 0.04, false, &mut b);
    } else if ctr {
        drv(-0.19, 0.0, 0.075, true, &mut b);
        drv(0.19, 0.0, 0.075, true, &mut b);
        drv(0.0, 0.0, 0.03, false, &mut b);
    } else {
        drv(0.0, -0.09, 0.105, true, &mut b);
        drv(0.0, 0.18, 0.04, false, &mut b);
    }
    b.rect(6.0)
}

fn sp_pos(i: usize) -> (f64, f64) {
    if i == 3 {
        let a = 48f64.to_radians();
        return (a.sin() * 2.3, SP_L.1 - a.cos() * 2.3);
    }
    let a = BP_ANG[i].to_radians();
    (a.sin() * 2.5, SP_L.1 - a.cos() * 2.5)
}

/// Halftone dots on the floor where the sound reaches (perspective ellipses, big and bright near
/// you, small at the edge, swelling with the bass).
#[allow(clippy::too_many_arguments)]
fn floor_dots(cr: &Context, cam: &Cam, rad: f64, half: f64, amp: f64, col: Rgb, alpha: f64, wz: f64) {
    let step = 1.0 / 6.0;
    let n = (rad / step).ceil() as i32;
    cr.new_path();
    for iz in -n..=n {
        for ix in -n..=n {
            let dx = ix as f64 * step;
            let dz = iz as f64 * step;
            let r = dx.hypot(dz);
            if r > rad {
                continue;
            }
            if half < PI - 0.002 && dx.atan2(-dz).abs() > half + 0.002 {
                continue;
            }
            let z = SP_L.1 + dz;
            if z < -wz + 0.1 || dx.abs() > SP_HALF - 0.1 {
                continue;
            }
            let fall = 1.0 - smoothstep(rad * 0.3, rad, r);
            let s = 0.05 + (0.2 + 0.14 * amp) * fall;
            let d = EYE_Z - z;
            let rx = cam.f * s * step / d;
            if rx < 0.4 {
                continue;
            }
            let (px, py) = cam.p(dx, 0.0, z);
            cr.save().ok();
            cr.translate(px, py);
            cr.scale(1.0, EYE_Y / d);
            cr.new_sub_path();
            cr.arc(0.0, 0.0, rx, 0.0, TAU);
            cr.restore().ok();
        }
    }
    src(cr, col, alpha);
    fill(cr);
}

fn screenprint(cr: &Context, w: f64, h: f64, sc: &Scene, ez: &Ez) -> Layout {
    let kk = (h.min(w * 0.5625) / 900.0).max(0.2);
    let (cx, kk) = if sc.fill { fit(w, 0.0, 450.0, kk, -1.0) } else { fit(w, sc.left_clip, 450.0, kk, 0.34) };
    let cam = Cam { cx, f: 1356.0 * kk, yh: -70.0 * kk + (h - 900.0 * kk) * 0.45 };
    let wz = ez.wz;
    let zb = -wz;
    let place = sc.place;
    let stadium = place == Place::Stadium;
    let bass = level(sc, 3);
    let zc = zone_colour(sc);
    let mut lay = Layout::default();

    // the left part of the room fades out into the text side
    let xbl = cam.p(-SP_HALF, 0.0, zb).0;
    let fade = if sc.fill { Fade { x0: -2.0, x1: -1.0 } } else { Fade { x0: (xbl - 0.14 * w).max(sc.left_clip), x1: xbl.max(sc.left_clip + 40.0) } };

    // tones: the page red, darkened
    let (floor_c, side_c, back_c) = match place {
        Place::Cinema | Place::Gaming => (kred(0.17), kred(0.15), kred(0.09)),
        Place::Concert => (kred(0.17), kred(0.20), kred(0.30)),
        Place::Stadium => (kred(0.19), kred(0.0), kred(0.0)),
        Place::Studio => (kred(0.20), kred(0.30), kred(0.50)),
        Place::Plain => (kred(0.20), kred(0.30), kred(0.46)),
    };
    let gx = if stadium { 14.0 } else { SP_HALF };

    // floor
    let floor = [cam.p(-gx, 0.0, zb), cam.p(gx, 0.0, zb), cam.p(gx, 0.0, SP_NEAR), cam.p(-gx, 0.0, SP_NEAR)];
    fill_fade(cr, &floor, fade, floor_c, 1.0);
    if stadium {
        // mown stripes on the pitch
        let mut z = zb;
        let mut alt = false;
        while z < SP_NEAR {
            let z1 = (z + 1.5).min(SP_NEAR);
            if alt {
                let q = [cam.p(-gx, 0.0, z), cam.p(gx, 0.0, z), cam.p(gx, 0.0, z1), cam.p(-gx, 0.0, z1)];
                fill_fade(cr, &q, fade, kred(0.25), 1.0);
            }
            alt = !alt;
            z = z1;
        }
    }
    if !stadium {
        // side walls and the front wall
        for s in [-1.0, 1.0] {
            let q = [cam.p(s * SP_HALF, 0.0, zb), cam.p(s * SP_HALF, 0.0, SP_NEAR), cam.p(s * SP_HALF, SP_HEIGHT, SP_NEAR), cam.p(s * SP_HALF, SP_HEIGHT, zb)];
            fill_fade(cr, &q, fade, side_c, 1.0);
        }
        let back = [cam.p(-SP_HALF, 0.0, zb), cam.p(SP_HALF, 0.0, zb), cam.p(SP_HALF, SP_HEIGHT, zb), cam.p(-SP_HALF, SP_HEIGHT, zb)];
        fill_fade(cr, &back, fade, back_c, 1.0);
        // the corners, drawn as black ink lines
        solid_line(cr, 1.4);
        src(cr, BLACK, 0.45);
        for s in [-1.0, 1.0] {
            let a = cam.p(s * SP_HALF, 0.0, zb);
            let t = cam.p(s * SP_HALF, SP_HEIGHT, zb);
            cr.move_to(a.0, a.1);
            cr.line_to(t.0, t.1);
        }
        let (l, r) = (cam.p(-SP_HALF, 0.0, zb), cam.p(SP_HALF, 0.0, zb));
        cr.move_to(l.0, l.1);
        cr.line_to(r.0, r.1);
        stroke(cr);
    }

    // the wall is dressed for the place
    let wp = |x: f64, y: f64| cam.p(x, y, zb);
    let wsc = cam.sc(zb);
    let mut screen: Option<(f64, f64, f64, f64)> = None;
    match place {
        Place::Plain => {}
        Place::Studio => {
            // acoustic foam on both walls
            for s in [-1.0, 1.0] {
                for i in 0..4 {
                    for j in 0..2 {
                        let zc0 = zb + 0.9 + i as f64 * 1.08;
                        let y0 = 1.35 + j as f64 * 1.08;
                        let fr = Frame3 { c: [s * 4.42, y0, zc0], yaw: 0.0 };
                        let mut bb = Bounds::new();
                        let a = 1.0;
                        box3(cr, &cam, fr, [0.05, 0.475, 0.475], &|l| mix(kred(0.26), kred(0.62), l), a, Some((kred(0.10), 0.6)), &mut bb);
                    }
                }
            }
            // a shelf with the records, and the neon name above
            let n = sc.records.len().min(6);
            let sh_y = 1.05;
            let (sx0, sx1) = (-3.3, 3.3);
            let top = [wp(sx0, sh_y), wp(sx1, sh_y), cam.p(sx1, sh_y, zb + 0.4), cam.p(sx0, sh_y, zb + 0.4)];
            let front = [cam.p(sx0, sh_y, zb + 0.4), cam.p(sx1, sh_y, zb + 0.4), cam.p(sx1, sh_y - 0.1, zb + 0.4), cam.p(sx0, sh_y - 0.1, zb + 0.4)];
            if n > 0 {
                let shadow = [wp(sx0, sh_y - 0.1), wp(sx1, sh_y - 0.1), wp(sx1, sh_y - 0.5), wp(sx0, sh_y - 0.5)];
                fill_poly(cr, &shadow, BLACK, 0.18);
                fill_poly(cr, &top, mix(kred(0.8), WHITE, 0.4), 1.0);
                fill_poly(cr, &front, kred(0.9), 1.0);
                let size = (6.0 / (n as f64 * 1.28)).min(1.0);
                let total = n as f64 * size + (n as f64 - 1.0) * size * 0.28;
                for (k, rec) in sc.records.iter().take(n).enumerate() {
                    let x0 = -total / 2.0 + k as f64 * size * 1.28;
                    let (px0, py0) = cam.p(x0, sh_y + 0.04 + size, zb + 0.2);
                    let (px1, py1) = cam.p(x0 + size, sh_y + 0.04, zb + 0.2);
                    let (rw, rh) = (px1 - px0, py1 - py0);
                    rrect(cr, px0 + 3.0, py0 + 3.0, rw + 3.0, rh, 1.0);
                    src(cr, BLACK, 0.35);
                    fill(cr);
                    cr.rectangle(px0 - 3.0, py0 - 3.0, rw + 6.0, rh + 6.0);
                    src(cr, (0.96, 0.94, 0.90), 1.0);
                    fill(cr);
                    draw_surface_fit(cr, rec, px0, py0, rw, rh);
                }
            }
            let (nx, ny) = wp(0.0, if n > 0 { 3.4 } else { 2.9 });
            draw_neon(cr, sc, nx, ny, 6.4 * wsc, 1.15 * wsc, true);
        }
        Place::Concert => {
            // a stage with a truss and lamps; the name in neon behind it
            let (nx, ny) = wp(0.0, 2.45);
            draw_neon(cr, sc, nx, ny, 5.8 * wsc, 1.0 * wsc, true);
            let sf = Frame3 { c: [0.0, 0.25, zb + 0.9], yaw: 0.0 };
            let mut bb = Bounds::new();
            box3(cr, &cam, sf, [3.9, 0.25, 0.9], &|l| mix(kred(0.42), kred(0.78), l), 1.0, Some((kred(0.08), 0.55)), &mut bb);
            let zt = zb + 1.4;
            let beam_a = 0.05 + 0.10 * level(sc, 1);
            for i in 0..5 {
                let x = -2.6 + i as f64 * 1.3;
                let lamp = cam.p(x, 3.55, zt);
                let spread = (i as f64 - 2.0) * 0.45;
                let q = [lamp, cam.p(x + spread - 0.8, 0.5, zt + 1.4), cam.p(x + spread + 0.8, 0.5, zt + 1.4)];
                fill_poly(cr, &q, WHITE, beam_a);
            }
            for s in [-1.0, 1.0] {
                let fr = Frame3 { c: [s * 3.6, 2.2, zt], yaw: 0.0 };
                box3(cr, &cam, fr, [0.07, 1.7, 0.07], &|l| mix(kred(0.25), kred(0.7), l), 1.0, None, &mut bb);
            }
            let bar = Frame3 { c: [0.0, 3.9, zt], yaw: 0.0 };
            box3(cr, &cam, bar, [3.65, 0.07, 0.07], &|l| mix(kred(0.25), kred(0.7), l), 1.0, None, &mut bb);
            for i in 0..5 {
                let x = -2.6 + i as f64 * 1.3;
                let fr = Frame3 { c: [x, 3.72, zt], yaw: 0.0 };
                box3(cr, &cam, fr, [0.11, 0.14, 0.11], &|l| mix(kred(0.5), WHITE, 0.3 + 0.7 * l), 1.0, Some((kred(0.1), 0.5)), &mut bb);
            }
        }
        Place::Stadium => {
            // a night sky, a bank of stands as halftone dots, floodlight beams, and a big screen on legs
            let hor = cam.p(0.0, 0.0, zb).1;
            {
                let sky = LinearGradient::new(0.0, 0.0, 0.0, hor);
                sky.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.92);
                sky.add_color_stop_rgba(1.0, kred(0.30).0, kred(0.30).1, kred(0.30).2, 0.92);
                let msk = LinearGradient::new(fade.x0, 0.0, fade.x1.max(fade.x0 + 1.0), 0.0);
                msk.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
                msk.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 1.0);
                cr.save().ok();
                cr.rectangle(fade.x0.max(0.0), 0.0, (w - fade.x0).max(0.0), hor);
                cr.clip();
                let _ = cr.set_source(&sky);
                let _ = cr.mask(&msk);
                cr.restore().ok();
            }
            {
                let band = [wp(-14.0, 0.0), wp(14.0, 0.0), wp(14.0, 1.7), wp(-14.0, 1.7)];
                fill_fade(cr, &band, fade, kred(0.10), 1.0);
                for row in 0..5 {
                    let y = 0.25 + row as f64 * 0.32;
                    let mut x = -9.0 + (row % 2) as f64 * 0.17;
                    while x < 9.0 {
                        let p0 = wp(x, y);
                        if p0.0 > fade.x0 {
                            cr.new_sub_path();
                            cr.arc(p0.0, p0.1, 0.07 * wsc, 0.0, TAU);
                        }
                        x += 0.34;
                    }
                }
                src(cr, WHITE, 0.2);
                fill(cr);
            }
            let beam_a = 0.025 + 0.05 * level(sc, 0).max(level(sc, 2));
            for s in [-1.0, 1.0] {
                let top = cam.p(s * 7.8, 9.0, zb + 2.0);
                let q = [top, cam.p(s * 4.6, 0.0, zb + 7.0), cam.p(s * 1.4, 0.0, zb + 6.0), cam.p(s * 6.0, 9.0, zb + 2.0)];
                fill_poly(cr, &q, WHITE, beam_a);
            }
            let (x0, x1, y0, y1) = (-3.9, 3.9, 0.95, 3.85);
            for s in [-1.0, 1.0] {
                let (a0, a1) = (wp(s * 2.6 - 0.12, y0), wp(s * 2.6 + 0.12, 0.0));
                cr.rectangle(a0.0, a0.1, a1.0 - a0.0, a1.1 - a0.1);
                src(cr, kred(0.33), 1.0);
                fill(cr);
            }
            let (p0, p1) = (wp(x0, y1), wp(x1, y0));
            let rect = below_corner(sc, (p0.0, p0.1, p1.0 - p0.0, p1.1 - p0.1));
            let m = 0.2 * wsc;
            rrect(cr, rect.0 - m, rect.1 - m, rect.2 + 2.0 * m, rect.3 + 2.0 * m, 3.0);
            src(cr, kred(0.30), 1.0);
            fill(cr);
            cr.rectangle(rect.0, rect.1, rect.2, rect.3);
            src(cr, (0.02, 0.0, 0.01), 1.0);
            cr.fill_preserve().ok();
            solid_line(cr, 2.0);
            src(cr, RED, 0.9);
            stroke(cr);
            match &sc.screen {
                Screen::None => draw_neon(cr, sc, rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0, rect.2 * 0.8, rect.3 * 0.46, true),
                other => {
                    cr.save().ok();
                    cr.rectangle(rect.0, rect.1, rect.2, rect.3);
                    cr.clip();
                    draw_screen_content(cr, other, rect.0, rect.1, rect.2, rect.3);
                    cr.restore().ok();
                }
            }
            if !matches!(sc.screen, Screen::None) {
                screen = Some(rect);
            }
        }
        Place::Cinema => {
            // curtains either side, a dark wall and the screen
            let (sw, sh, cy) = (5.4, 3.04, 2.45);
            let (c0, c1) = (sw / 2.0, SP_HALF);
            let n = 8;
            for s in [-1.0, 1.0] {
                for i in 0..n {
                    let xa = s * (c0 + 0.05 + (c1 - c0 - 0.05) * i as f64 / n as f64);
                    let xb = s * (c0 + 0.05 + (c1 - c0 - 0.05) * (i + 1) as f64 / n as f64);
                    let q = [cam.p(xa, 0.0, zb + 0.35), cam.p(xb, 0.0, zb + 0.35), cam.p(xb, SP_HEIGHT, zb + 0.35), cam.p(xa, SP_HEIGHT, zb + 0.35)];
                    fill_fade(cr, &q, fade, if i % 2 == 0 { kred(0.58) } else { kred(0.40) }, 1.0);
                }
            }
            let (p0, p1) = (wp(-sw / 2.0, cy + sh / 2.0), wp(sw / 2.0, cy - sh / 2.0));
            let rect = below_corner(sc, (p0.0, p0.1, p1.0 - p0.0, p1.1 - p0.1));
            cr.rectangle(rect.0 - 4.0, rect.1 - 4.0, rect.2 + 8.0, rect.3 + 8.0);
            src(cr, BLACK, 0.9);
            fill(cr);
            cr.rectangle(rect.0, rect.1, rect.2, rect.3);
            src(cr, (0.03, 0.0, 0.01), 1.0);
            cr.fill_preserve().ok();
            solid_line(cr, 1.6);
            src(cr, WHITE, 0.45);
            stroke(cr);
            cr.save().ok();
            cr.rectangle(rect.0, rect.1, rect.2, rect.3);
            cr.clip();
            draw_screen_content(cr, &sc.screen, rect.0, rect.1, rect.2, rect.3);
            cr.restore().ok();
            screen = Some(rect);
        }
        Place::Gaming => {
            // a TV in a solid wall, red LED strips on the walls and under the screen
            let (sw, sh, cy) = (5.4, 3.04, 2.3);
            let led = RED;
            for s in [-1.0, 1.0] {
                let a = cam.p(s * (SP_HALF - 0.02), 0.04, zb);
                let b2 = cam.p(s * (SP_HALF - 0.02), 0.04, SP_NEAR);
                for (lw, al) in [(7.0, 0.14), (3.5, 0.3), (1.6, 0.95)] {
                    solid_line(cr, lw);
                    cr.set_line_cap(LineCap::Round);
                    fade_src(cr, fade, led, al);
                    cr.move_to(a.0, a.1);
                    cr.line_to(b2.0, b2.1);
                    stroke(cr);
                }
            }
            let (p0, p1) = (wp(-sw / 2.0, cy + sh / 2.0), wp(sw / 2.0, cy - sh / 2.0));
            let rect = below_corner(sc, (p0.0, p0.1, p1.0 - p0.0, p1.1 - p0.1));
            let bz = 0.18 * wsc;
            rrect(cr, rect.0 - bz, rect.1 - bz, rect.2 + 2.0 * bz, rect.3 + 2.0 * bz + 0.1 * wsc, 4.0);
            src(cr, kred(0.20), 1.0);
            cr.fill_preserve().ok();
            solid_line(cr, 1.4);
            src(cr, WHITE, 0.30);
            stroke(cr);
            cr.rectangle(rect.0, rect.1, rect.2, rect.3);
            src(cr, (0.03, 0.0, 0.01), 1.0);
            fill(cr);
            cr.save().ok();
            cr.rectangle(rect.0, rect.1, rect.2, rect.3);
            cr.clip();
            draw_screen_content(cr, &sc.screen, rect.0, rect.1, rect.2, rect.3);
            cr.restore().ok();
            let ly = rect.1 + rect.3 + bz + 0.1 * wsc + 6.0;
            for (lw, al) in [(8.0, 0.14), (4.0, 0.3), (2.0, 1.0)] {
                solid_line(cr, lw);
                cr.set_line_cap(LineCap::Round);
                src(cr, led, al);
                cr.move_to(rect.0, ly);
                cr.line_to(rect.0 + rect.2, ly);
                stroke(cr);
            }
            screen = Some(rect);
        }
    }
    lay.screen_rect = screen;

    // where the sound reaches
    floor_dots(cr, &cam, 2.2 * ez.rad, ez.half, bass, zc, 0.95, wz);

    // you: a red ring on the floor round the sofa, with a dark keyline so it stands off the dots
    let ring_r = 1.0 + 0.03 * bass;
    cr.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
    for (ro, ri, c, a) in [(1.12, 0.86, (0.07, 0.01, 0.02), 0.7), (1.04 * ring_r, 0.93, RED, 1.0)] {
        poly(cr, &floor_ring(&cam, 0.0, SP_SOFA_Z, ro, 72));
        poly(cr, &floor_ring(&cam, 0.0, SP_SOFA_Z, ri, 72));
        src(cr, c, a);
        fill(cr);
    }
    cr.set_fill_rule(gtk::cairo::FillRule::Winding);

    // speakers and the sofa, far to near
    enum It {
        Spk(usize),
        Sofa,
    }
    let mut items: Vec<(f64, It)> = (0..6).map(|i| (sp_pos(i).1, It::Spk(i))).collect();
    items.push((SP_SOFA_Z, It::Sofa));
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (_, it) in items {
        match it {
            It::Spk(i) => {
                let r = sp_speaker(cr, &cam, i, ez.act[i], level(sc, i), sc);
                lay.speakers.push((ORDER[i], r));
            }
            It::Sofa => {
                let mut bb = Bounds::new();
                let seat = |l: f64| mix(kred(0.30), kred(0.78), l);
                let dark = |l: f64| mix(kred(0.22), kred(0.58), l);
                let z0 = SP_SOFA_Z;
                let edge = Some((kred(0.08), 0.5));
                for sx in [-1.0, 1.0] {
                    let leg = Frame3 { c: [sx * 0.7, 0.1, z0 + 0.3], yaw: 0.0 };
                    box3(cr, &cam, leg, [0.03, 0.1, 0.03], &dark, 1.0, None, &mut bb);
                }
                box3(cr, &cam, Frame3 { c: [0.0, 0.40, z0], yaw: 0.0 }, [0.70, 0.12, 0.46], &seat, 1.0, edge, &mut bb);
                for sx in [-1.0, 1.0] {
                    let arm = Frame3 { c: [sx * 0.79, 0.50, z0], yaw: 0.0 };
                    box3(cr, &cam, arm, [0.09, 0.30, 0.46], &dark, 1.0, edge, &mut bb);
                }
                box3(cr, &cam, Frame3 { c: [0.0, 0.80, z0 + 0.36], yaw: 0.0 }, [0.70, 0.42, 0.10], &seat, 1.0, edge, &mut bb);
                // the split between the two back cushions
                let (a, b2) = (cam.p(0.0, 0.42, z0 + 0.47), cam.p(0.0, 1.22, z0 + 0.47));
                solid_line(cr, 1.4);
                src(cr, BLACK, 0.35);
                cr.move_to(a.0, a.1);
                cr.line_to(b2.0, b2.1);
                stroke(cr);
                // headphones float over the headrest
                if ez.hp > 0.01 {
                    let head = cam.p(0.0, 1.46, SP_SOFA_Z + 0.05);
                    let u = cam.sc(SP_SOFA_Z) * ez.hp * (1.0 + 0.1 * (ez.hp * PI).sin());
                    cr.new_sub_path();
                    cr.arc(head.0, head.1 + 0.02 * u, 0.17 * u, 0.0, TAU);
                    src(cr, kred(0.5), ez.hp);
                    fill(cr);
                    solid_line(cr, 0.05 * u);
                    cr.set_line_cap(LineCap::Round);
                    src(cr, WHITE, 1.0);
                    cr.new_sub_path();
                    cr.arc(head.0, head.1, 0.20 * u, PI, TAU);
                    stroke(cr);
                    for sx in [-1.0, 1.0] {
                        rrect(cr, head.0 + sx * 0.20 * u - 0.06 * u, head.1 - 0.05 * u, 0.12 * u, 0.2 * u, 0.05 * u);
                        src(cr, WHITE, 1.0);
                        fill(cr);
                        solid_line(cr, 1.0);
                        src(cr, RED, 1.0);
                        rrect(cr, head.0 + sx * 0.20 * u - 0.06 * u, head.1 - 0.05 * u, 0.12 * u, 0.2 * u, 0.05 * u);
                        stroke(cr);
                    }
                }
            }
        }
    }
    lay.you = cam.p(0.0, 1.35, SP_SOFA_Z);
    lay
}

// ---------------------------------------------------------------------------------------------
// render tests (ignored by default): cargo test --release -- --ignored
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn solid(w: i32, h: i32, c: Rgb) -> ImageSurface {
        let s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        let cr = Context::new(&s).unwrap();
        let g = LinearGradient::new(0.0, 0.0, w as f64, h as f64);
        g.add_color_stop_rgb(0.0, c.0, c.1, c.2);
        g.add_color_stop_rgb(1.0, c.0 * 0.55, c.1 * 0.55, c.2 * 0.55);
        let _ = cr.set_source(&g);
        let _ = cr.paint();
        s
    }

    /// White-on-transparent "logo": a ring and a bar, alpha only.
    fn logo(w: i32, h: i32) -> ImageSurface {
        let s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        let cr = Context::new(&s).unwrap();
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        cr.set_line_width(h as f64 * 0.12);
        cr.arc(h as f64 * 0.5, h as f64 * 0.5, h as f64 * 0.38, 0.0, TAU);
        let _ = cr.stroke();
        cr.rectangle(h as f64 * 1.0, h as f64 * 0.42, w as f64 - h as f64 * 1.1, h as f64 * 0.16);
        let _ = cr.fill();
        s
    }

    fn crc32(data: &[u8], table: &[u32; 256], mut c: u32) -> u32 {
        for &b in data {
            c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
        }
        c
    }

    /// A dependency-free PNG writer (RGB, stored deflate blocks); shrunk by ImageMagick if present.
    fn write_png(path: &str, s: &mut ImageSurface) {
        s.flush();
        let (w, h, stride) = (s.width() as usize, s.height() as usize, s.stride() as usize);
        let data = s.data().unwrap();
        let mut raw = Vec::with_capacity((w * 3 + 1) * h);
        for y in 0..h {
            raw.push(0u8);
            for x in 0..w {
                let o = y * stride + x * 4;
                raw.extend_from_slice(&[data[o + 2], data[o + 1], data[o]]);
            }
        }
        let mut table = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
            }
            table[n as usize] = c;
        }
        let mut z = vec![0x78u8, 0x01];
        let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
        for (i, c) in chunks.iter().enumerate() {
            z.push(if i + 1 == chunks.len() { 1 } else { 0 });
            z.extend_from_slice(&(c.len() as u16).to_le_bytes());
            z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
            z.extend_from_slice(c);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &raw {
            a = (a + x as u32) % 65521;
            b = (b + a) % 65521;
        }
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut chunk = |kind: &[u8; 4], body: &[u8]| {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            let c = !crc32(body, &table, crc32(kind, &table, 0xffff_ffff));
            out.extend_from_slice(&c.to_be_bytes());
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        chunk(b"IDAT", &z);
        chunk(b"IEND", &[]);
        std::fs::File::create(path).unwrap().write_all(&out).unwrap();
        for tool in ["magick", "convert"] {
            let ok = std::process::Command::new(tool)
                .arg(path)
                .arg(path)
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                break;
            }
        }
    }

    fn page(cr: &Context, w: f64, h: f64) {
        // the page: near-black red with a brighter red haze on the right, like the prototype's photo
        src(cr, (0.05, 0.01, 0.015), 1.0);
        let _ = cr.paint();
        let g = gtk::cairo::RadialGradient::new(w * 0.8, h * 0.4, 0.0, w * 0.8, h * 0.4, w * 0.6);
        g.add_color_stop_rgba(0.0, RED.0, RED.1, RED.2, 0.55);
        g.add_color_stop_rgba(1.0, RED.0, RED.1, RED.2, 0.0);
        let _ = cr.set_source(&g);
        let _ = cr.paint();
        // the text block on the left
        display_font(cr, 150.0);
        src(cr, WHITE, 0.9);
        cr.move_to(46.0, 285.0);
        let _ = cr.show_text("Everlong");
    }

    struct Fake {
        records: Vec<ImageSurface>,
        logo: ImageSurface,
        pic: ImageSurface,
    }

    fn fake() -> Fake {
        Fake {
            records: vec![solid(240, 240, (0.1, 0.35, 0.6)), solid(240, 240, (0.9, 0.7, 0.1)), solid(240, 240, (0.2, 0.55, 0.3))],
            logo: logo(500, 160),
            pic: solid(640, 360, (0.9, 0.5, 0.15)),
        }
    }

    fn scene<'a>(f: &'a Fake, place: Place) -> Scene<'a> {
        Scene {
            place,
            studio_grow: 0.0,
            active: [true; 6],
            levels: [0.75, 0.55, 0.85, 0.95, 0.4, 0.6],
            headphones: false,
            sbx_on: true,
            tone_rgb: (0.89, 0.15, 0.18),
            size: 0.6,
            spread: 0.5,
            around: true,
            playing: true,
            t: 2.3,
            neon_text: if matches!(place, Place::Studio | Place::Concert | Place::Stadium) { "FOO FIGHTERS" } else { "" },
            neon_logo: None,
            records: if place == Place::Studio { &f.records } else { &[] },
            screen: match place {
                Place::Cinema => Screen::Title("DUNE"),
                Place::Gaming => Screen::Picture(&f.pic),
                _ => Screen::None,
            },
            left_clip: 0.0,
            corner: None,
            fill: false,
        }
    }

    fn render(theme: &str, name: &str, sc: &Scene, w: i32, h: i32, boxes: bool) -> Layout {
        EZ.with(|c| c.set(Ez { init: false, act: [0.0; 6], hp: 0.0, rad: 1.0, half: PI, wz: 4.6 }));
        let mut s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        let lay;
        {
            let cr = Context::new(&s).unwrap();
            page(&cr, w as f64, h as f64);
            lay = draw(theme, &cr, w as f64, h as f64, 0.033, sc, &mut Smooth::default());
            if boxes {
                cr.set_line_width(1.0);
                for (_, (x, y, bw, bh)) in &lay.speakers {
                    cr.set_source_rgba(0.0, 1.0, 0.0, 0.8);
                    cr.rectangle(*x, *y, *bw, *bh);
                    let _ = cr.stroke();
                }
                if let Some((x, y, bw, bh)) = lay.screen_rect {
                    cr.set_source_rgba(1.0, 1.0, 0.0, 0.8);
                    cr.rectangle(x, y, bw, bh);
                    let _ = cr.stroke();
                }
                cr.set_source_rgba(0.0, 1.0, 1.0, 0.9);
                cr.arc(lay.you.0, lay.you.1, 5.0, 0.0, TAU);
                let _ = cr.stroke();
            }
        }
        let suffix = if boxes { "-boxes" } else { "" };
        write_png(&format!("/tmp/theme-{}-{}{}.png", theme, name, suffix), &mut s);
        lay
    }

    fn run_theme(theme: &str, places: &[(Place, &str)]) {
        let f = fake();
        for (pl, nm) in places {
            let lay = render(theme, nm, &scene(&f, *pl), 1600, 900, false);
            println!("{theme}/{nm}: screen_rect={:?} you={:?} animating={}", lay.screen_rect, lay.you, lay.animating);
            if *nm == "studio" {
                render(theme, nm, &scene(&f, *pl), 1600, 900, true);
            }
        }
    }

    /// Not ignored: every theme draws every place without panicking, and returns six speaker boxes.
    #[test]
    fn smoke_draw_all_themes() {
        let f = fake();
        for theme in ["blueprint", "screenprint", "callouts", "something-else"] {
            for place in [Place::Plain, Place::Studio, Place::Concert, Place::Stadium, Place::Cinema, Place::Gaming] {
                let s = ImageSurface::create(Format::ARgb32, 640, 360).unwrap();
                let cr = Context::new(&s).unwrap();
                let mut sc = scene(&f, place);
                sc.left_clip = 100.0;
                let lay = draw(theme, &cr, 640.0, 360.0, 0.033, &sc, &mut Smooth::default());
                assert_eq!(lay.speakers.len(), 6, "{theme}/{place:?}");
                assert!(lay.animating);
                let ids: Vec<&str> = lay.speakers.iter().map(|(id, _)| *id).collect();
                assert_eq!(ids, ORDER.to_vec());
                sc.playing = false;
                sc.headphones = true;
                let lay = draw(theme, &cr, 640.0, 360.0, 0.033, &sc, &mut Smooth::default());
                assert!(lay.animating || sc.headphones, "easing runs on");
            }
        }
    }

    const MAIN: [(Place, &str); 3] = [(Place::Studio, "studio"), (Place::Cinema, "cinema"), (Place::Gaming, "gaming")];

    #[test]
    #[ignore]
    fn render_blueprint() {
        run_theme("blueprint", &MAIN);
    }

    #[test]
    #[ignore]
    fn render_screenprint() {
        run_theme("screenprint", &MAIN);
        run_theme("screenprint", &[(Place::Concert, "concert"), (Place::Stadium, "stadium"), (Place::Plain, "plain")]);
    }

    #[test]
    #[ignore]
    fn render_callouts() {
        run_theme("callouts", &MAIN);
    }

    /// Headphones, a quiet stereo setup, and a page with the text column taking half the width.
    #[test]
    #[ignore]
    fn render_variants() {
        let f = fake();
        for theme in ["blueprint", "screenprint", "callouts"] {
            let mut hp = scene(&f, Place::Cinema);
            hp.headphones = true;
            hp.size = 0.0;
            render(theme, "hp", &hp, 1600, 900, false);

            let mut quiet = scene(&f, Place::Studio);
            quiet.playing = false;
            quiet.around = false;
            quiet.active = [true, false, true, true, false, false];
            quiet.size = 1.8;
            quiet.spread = 0.8;
            quiet.sbx_on = true;
            let lay = render(theme, "quiet", &quiet, 1600, 900, false);
            assert!(!lay.animating || theme.is_empty(), "a paused room must not keep redrawing");

            let mut clip = scene(&f, Place::Gaming);
            clip.left_clip = 800.0;
            let lay = render(theme, "clip", &clip, 1600, 900, false);
            for (id, (x, _, _, _)) in &lay.speakers {
                assert!(*x >= 790.0, "{theme}: speaker {id} starts at {x}, left of the clip");
            }

            let sml = scene(&f, Place::Studio);
            render(theme, "small", &sml, 1000, 640, false);
        }
    }

    /// Every kind of screen, in the places that draw one.
    #[test]
    #[ignore]
    fn render_screens() {
        let f = fake();
        let badge = solid(120, 120, (0.9, 0.9, 0.95));
        let kinds: Vec<(&str, Screen)> = vec![
            ("logo", Screen::Logo(&f.logo)),
            ("video", Screen::Video),
            ("score", Screen::Score { home: "Arsenal", away: "Spurs", home_badge: Some(&badge), away_badge: None }),
            ("none", Screen::None),
        ];
        for theme in ["screenprint", "callouts"] {
            for (nm, _) in &kinds {
                for place in [Place::Cinema, Place::Stadium] {
                    let screen = match *nm {
                        "logo" => Screen::Logo(&f.logo),
                        "video" => Screen::Video,
                        "score" => Screen::Score { home: "Arsenal", away: "Spurs", home_badge: Some(&badge), away_badge: None },
                        _ => Screen::None,
                    };
                    let mut sc = scene(&f, place);
                    sc.screen = screen;
                    sc.neon_logo = if *nm == "logo" { Some(&f.logo) } else { None };
                    let lay = render(theme, &format!("screen-{nm}-{place:?}").to_lowercase(), &sc, 1600, 900, false);
                    println!("{theme}/{nm}/{place:?}: screen_rect={:?}", lay.screen_rect);
                }
            }
        }
    }

    /// The neon logo path (an alpha mask) in the studio and on a concert stage.
    #[test]
    #[ignore]
    fn render_neon_logo() {
        let f = fake();
        for theme in ["screenprint", "callouts"] {
            for place in [Place::Studio, Place::Concert] {
                let mut sc = scene(&f, place);
                sc.neon_logo = Some(&f.logo);
                render(theme, &format!("logo-{place:?}").to_lowercase(), &sc, 1600, 900, false);
            }
        }
    }

    /// Average cost of one frame (playing, so everything animates).
    #[test]
    #[ignore]
    fn bench_themes() {
        let f = fake();
        for theme in ["blueprint", "screenprint", "callouts"] {
            for place in [Place::Studio, Place::Stadium, Place::Cinema] {
                let sc = scene(&f, place);
                let s = ImageSurface::create(Format::ARgb32, 1600, 900).unwrap();
                let cr = Context::new(&s).unwrap();
                let mut sm = Smooth::default();
                let mut total = std::time::Duration::ZERO;
                let n = 60;
                for i in 0..n {
                    let mut sc2 = scene(&f, place);
                    sc2.t = i as f64 / 30.0;
                    cr.save().unwrap();
                    page(&cr, 1600.0, 900.0);
                    cr.restore().unwrap();
                    let t0 = std::time::Instant::now();
                    let _ = draw(theme, &cr, 1600.0, 900.0, 0.033, &sc2, &mut sm);
                    s.flush();
                    total += t0.elapsed();
                }
                let _ = sc;
                println!("{theme}/{place:?}: {:.2} ms/frame", total.as_secs_f64() * 1000.0 / n as f64);
            }
        }
    }
}
