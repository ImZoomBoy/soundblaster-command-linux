//! "Over your shoulder": the room seen from just behind your seat, drawn with cairo.
//!
//! A port of the design prototype's "5 · From your seat" canvas drawing (`T5.paint`). The
//! perspective room, the halftone red-dot floor (the dots grow where the sound reaches), six
//! speakers printed as dot grids, dotted wall edges and the "place" dressing that cross-fades
//! by weight: studio (neon name plus a shelf of records), concert (stage, lamp beams, neon
//! name), stadium (big screen or neon name, stands), cinema (screen) and gaming (monitor).
//!
//! Everything is plain data in (`Scene`), a `Smooth` that carries the eased state between frames,
//! and a `Layout` back out (speaker hit boxes, the screen rectangle for a video overlay).
//! No globals, no image decoding and no per-frame image processing: the caller passes prepared
//! surfaces. The only cached image work is the neon glow, which is rebuilt when the sign changes.
//!
//! Album covers: hand over small copies (see `thumbnail`, about 100-200 px); they are painted
//! scaled into slots of about 40 px every frame.
//!
//! Fonts are cairo "toy" fonts: "Anton" for the neon and titles, "Archivo Narrow" for the small
//! caps labels; fontconfig falls back to the default sans when they are not installed.
#![allow(dead_code)]

use gtk::cairo::{
    Context, FillRule, Filter, FontSlant, FontWeight, Format, ImageSurface, LineCap, LineJoin,
    LinearGradient, Operator, RadialGradient,
};
use std::f64::consts::{PI, TAU};
use std::hash::{Hash, Hasher};

/// The speakers in the order used by `Scene::active`, `Scene::levels` and `Layout::speakers`.
pub const ORDER: [&str; 6] = ["FL", "FC", "FR", "LFE", "RL", "RR"];
const I_FL: usize = 0;
const I_FC: usize = 1;
const I_FR: usize = 2;
const I_LFE: usize = 3;
const I_RL: usize = 4;
const I_RR: usize = 5;

/// What the room is dressed as.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Place {
    Plain,
    Studio,
    Concert,
    Stadium,
    Cinema,
    Gaming,
}

impl Place {
    fn idx(self) -> usize {
        match self {
            Place::Plain => 0,
            Place::Studio => 1,
            Place::Concert => 2,
            Place::Stadium => 3,
            Place::Cinema => 4,
            Place::Gaming => 5,
        }
    }
}

/// What the cinema screen, the game monitor or the stadium big screen shows.
#[derive(Clone, Copy)]
pub enum Screen<'a> {
    None,
    /// A white-on-transparent logo (a film title card, a game logo).
    Logo(&'a ImageSurface),
    /// A picture, cropped to fill the screen.
    Picture(&'a ImageSurface),
    Title(&'a str),
    /// Leave the rectangle dark; the app overlays a video (see `Layout::screen_rect`).
    Video,
    Score {
        home: &'a str,
        away: &'a str,
        home_badge: Option<&'a ImageSurface>,
        away_badge: Option<&'a ImageSurface>,
    },
}

/// Everything one frame needs. Colours are 0..1 (values above 1 are read as 0..255).
pub struct Scene<'a> {
    pub place: Place,
    /// 0..1: a studio song set to concert/stadium sound makes the studio a little bigger.
    pub studio_grow: f64,
    /// Which speakers are lit, in `ORDER`.
    pub active: [bool; 6],
    /// 0..1 live meter levels, in `ORDER`.
    pub levels: [f64; 6],
    pub headphones: bool,
    pub sbx_on: bool,
    /// The dot colour (SBX tone).
    pub tone_rgb: (f64, f64, f64),
    /// 0 small, 1 medium, 2 big (the SBX profile's size).
    pub size: f64,
    /// 0..1.
    pub spread: f64,
    /// Sound all around (5.1 or the rears on).
    pub around: bool,
    pub playing: bool,
    /// Seconds, for pulsing and for the speaker-test ripples (see `Smooth::ping`).
    pub t: f64,
    /// The artist name for the neon sign when there is no logo.
    pub neon_text: &'a str,
    /// A white-on-transparent artist/game logo, already prepared.
    pub neon_logo: Option<&'a ImageSurface>,
    /// Album covers for the studio shelf; one gap is left for the playing album.
    pub records: &'a [ImageSurface],
    pub screen: Screen<'a>,
    /// The x where the room may start (split page); 0 = none.
    pub left_clip: f64,
    /// The page's top-right corner (the big 5.1 or the venue tag): x, y, w, h.
    /// Wall screens in the other themes stay below it.
    pub corner: Option<(f64, f64, f64, f64)>,
}

impl Default for Scene<'_> {
    fn default() -> Self {
        Scene {
            place: Place::Plain,
            studio_grow: 0.0,
            active: [false; 6],
            levels: [0.0; 6],
            headphones: false,
            sbx_on: true,
            tone_rgb: (1.0, 0.231, 0.267),
            size: 1.0,
            spread: 0.5,
            around: false,
            playing: false,
            t: 0.0,
            neon_text: "",
            neon_logo: None,
            records: &[],
            screen: Screen::None,
            left_clip: 0.0,
            corner: None,
        }
    }
}

/// The state that eases from frame to frame (place cross-fades, zone size, headphones) plus the
/// neon glow cache and the speaker-test ripples. Start with `Smooth::default()`; the first frame
/// snaps to the scene instead of easing from nothing.
#[derive(Default, Clone)]
pub struct Smooth {
    init: bool,
    /// Cross-fade weight per `Place`.
    w: [f64; 6],
    /// Depth of the floor / the back wall.
    zw: f64,
    /// Half width of the floor.
    x: f64,
    /// Sound zone radius and half angle.
    rad: f64,
    half: f64,
    hp: f64,
    /// Studio growth.
    sg: f64,
    /// Where the playing album's gap sits on the studio shelf (index among all slots); `None`
    /// puts it second, as in the prototype.
    pub record_gap: Option<usize>,
    ripples: Vec<(usize, f64)>,
    hot: Option<(usize, f64)>,
    neon: NeonState,
}

impl Smooth {
    /// A speaker test: a ripple round the speaker and its dots go white for `hold` seconds.
    /// `speaker` is an index into `ORDER`; `t` is `Scene::t` at that moment.
    pub fn ping(&mut self, speaker: usize, t: f64, hold: f64) {
        if speaker < 6 {
            self.ripples.push((speaker, t));
            self.hot = Some((speaker, t + hold));
        }
    }

    /// Throw away the cached neon glow (it is also rebuilt by itself when the logo changes).
    pub fn forget_neon(&mut self) {
        self.neon = NeonState::default();
    }
}

/// The neon sign's cached images. The sign is rendered at the size it is drawn at (whole pixels),
/// so a frame only paints unscaled masks; the images are rebuilt when the logo changes or the
/// drawn size does (a place cross-fade, a window resize).
#[derive(Default, Clone)]
struct NeonState {
    fp: Option<ImageSurface>,
    /// Content key of the sign (logo fingerprint, or hash of the text).
    key: u64,
    /// The sign drawn from text (when there is no logo).
    text: Option<ImageSurface>,
    sized: Option<NeonSized>,
}

#[derive(Clone)]
struct NeonSized {
    wi: i32,
    hi: i32,
    /// The sign itself, as alpha.
    tube: ImageSurface,
    /// A wide glow and a tight glow round it (alpha), each padded by `pad_*` on every side.
    big: ImageSurface,
    small: ImageSurface,
    pad_big: i32,
    pad_small: i32,
}

/// A scaled-down copy of `src` (larger side `max_px`), for album covers: painting a big cover
/// scaled into a 40 px slot every frame costs more than the whole rest of the room.
pub fn thumbnail(src: &ImageSurface, max_px: i32) -> Option<ImageSurface> {
    let (w, h) = (src.width().max(1), src.height().max(1));
    let k = (max_px.max(8) as f64 / w.max(h) as f64).min(1.0);
    let (tw, th) = (((w as f64 * k).round() as i32).max(1), ((h as f64 * k).round() as i32).max(1));
    let out = ImageSurface::create(Format::ARgb32, tw, th).ok()?;
    {
        let c = Context::new(&out).ok()?;
        c.scale(tw as f64 / w as f64, th as f64 / h as f64);
        c.set_source_surface(src, 0.0, 0.0).ok()?;
        c.source().set_filter(Filter::Best);
        let _ = c.paint();
    }
    Some(out)
}

/// Where things ended up this frame.
#[derive(Default, Clone, Debug)]
pub struct Layout {
    /// Where the cinema screen / game monitor / stadium big screen is drawn, in the drawing's
    /// own pixels (x, y, w, h). Only set while that place is the dominant one.
    pub screen_rect: Option<(f64, f64, f64, f64)>,
    /// How much of that place is faded in (0..1), for fading a video overlay with it.
    pub screen_alpha: f64,
    /// Hit boxes for tap-to-test, in `ORDER`.
    #[allow(clippy::type_complexity)]
    pub speakers: Vec<(&'static str, (f64, f64, f64, f64))>,
    /// The ring on the floor (where you sit).
    pub you: (f64, f64),
    /// True while cross-fades, ripples or pulses need more frames.
    pub animating: bool,
}

// ---------------------------------------------------------------------------------------------
// Room geometry, from the prototype

const CZ: f64 = 5.2;
const EYE: f64 = 2.3;

struct Pl {
    zw: f64,
    x: f64,
    yh: f64,
}

const PL: [Pl; 6] = [
    Pl { zw: 60.0, x: 99.0, yh: 0.0 },
    Pl { zw: 4.3, x: 3.3, yh: 3.1 },
    Pl { zw: 13.0, x: 6.5, yh: 7.6 },
    Pl { zw: 60.0, x: 99.0, yh: 0.0 },
    Pl { zw: 8.0, x: 5.4, yh: 5.4 },
    Pl { zw: 6.2, x: 3.9, yh: 3.7 },
];

type Rgb = (f64, f64, f64);
const WHITE: Rgb = (255.0, 255.0, 255.0);
const RED: Rgb = (227.0, 38.0, 47.0);
const NEON_RED: Rgb = (255.0, 42.0, 53.0);
const NEON_CORE: Rgb = (255.0, 226.0, 228.0);
const FLOOR_RED: Rgb = (255.0, 59.0, 68.0);
const WALL_RED: Rgb = (255.0, 75.0, 83.0);
const BLACK: Rgb = (0.0, 0.0, 0.0);

const ANTON: &str = "Anton";
const NARROW: &str = "Archivo Narrow";

/// The sound zone's radius: `small`..`big` by profile size and spread (zoneR in the prototype).
fn zone_r(sc: &Scene, spread: f64, small: f64, big: f64) -> f64 {
    if sc.headphones {
        small * 0.25
    } else if !sc.sbx_on {
        small * 0.75
    } else {
        small + (big - small) * (unit(sc.size / 2.0)) * (0.85 + 0.3 * spread)
    }
}

/// Clamps to 0..1 and turns NaN into 0.
#[allow(clippy::manual_clamp)] // clamp() would pass a NaN through
fn unit(x: f64) -> f64 {
    x.max(0.0).min(1.0)
}

fn tone_color(sc: &Scene) -> Rgb {
    if !sc.sbx_on {
        return FLOOR_RED;
    }
    let (r, g, b) = sc.tone_rgb;
    if r > 1.0 || g > 1.0 || b > 1.0 {
        (r, g, b)
    } else {
        (r * 255.0, g * 255.0, b * 255.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Small cairo helpers (the canvas calls the prototype uses)

fn src(cr: &Context, c: Rgb, a: f64) {
    cr.set_source_rgba(c.0 / 255.0, c.1 / 255.0, c.2 / 255.0, a.clamp(0.0, 1.0));
}
fn fill(cr: &Context) {
    let _ = cr.fill();
}
fn stroke(cr: &Context) {
    let _ = cr.stroke();
}
fn save(cr: &Context) {
    let _ = cr.save();
}
fn restore(cr: &Context) {
    let _ = cr.restore();
}

fn font(cr: &Context, family: &str, bold: bool, size: f64) {
    cr.select_font_face(
        family,
        FontSlant::Normal,
        if bold { FontWeight::Bold } else { FontWeight::Normal },
    );
    cr.set_font_size(size.max(1.0));
}

fn text_w(cr: &Context, s: &str) -> f64 {
    cr.text_extents(s).map(|e| e.x_advance()).unwrap_or(0.0)
}

/// Centred on x, vertically on the middle of the em box (canvas textAlign center / middle).
fn text_mid(cr: &Context, s: &str, x: f64, y: f64) {
    let (asc, desc) = cr.font_extents().map(|e| (e.ascent(), e.descent())).unwrap_or((0.0, 0.0));
    let w = text_w(cr, s);
    cr.move_to(x - w / 2.0, y + (asc - desc) / 2.0);
    let _ = cr.show_text(s);
}

/// Centred on x on the alphabetic baseline, with letter spacing (canvas letterSpacing).
fn text_spaced(cr: &Context, s: &str, x: f64, y: f64, spacing: f64) {
    let mut adv = Vec::with_capacity(s.len());
    let mut total = 0.0;
    for ch in s.chars() {
        let mut b = [0u8; 4];
        let a = text_w(cr, ch.encode_utf8(&mut b));
        adv.push(a);
        total += a + spacing;
    }
    let mut px = x - total / 2.0;
    for (ch, a) in s.chars().zip(adv) {
        let mut b = [0u8; 4];
        cr.move_to(px, y);
        let _ = cr.show_text(ch.encode_utf8(&mut b));
        px += a + spacing;
    }
}

/// The prototype's fitText: as tall as `max_h`, squeezed to `max_w`; centred on x, y.
fn fit_text(cr: &Context, s: &str, x: f64, y: f64, max_w: f64, max_h: f64) -> f64 {
    let mut fs = max_h.max(1.0);
    font(cr, ANTON, false, fs);
    let w = text_w(cr, s);
    if w > max_w && w > 0.0 {
        fs = (fs * max_w / w).max(8.0);
        font(cr, ANTON, false, fs);
    }
    text_mid(cr, s, x, y);
    fs
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

/// An ellipse path (the transform is undone before stroking, so the line keeps its width).
fn ellipse(cr: &Context, cx: f64, cy: f64, rx: f64, ry: f64) {
    save(cr);
    cr.translate(cx, cy);
    cr.scale(rx.max(1e-3), ry.max(1e-3));
    cr.new_sub_path();
    cr.arc(0.0, 0.0, 1.0, 0.0, TAU);
    restore(cr);
}

/// Adds a filled-dot subpath: a regular polygon with as many sides as its size needs (same area
/// as the circle). Far cheaper for cairo to tessellate than `arc` when there are thousands.
fn dot(cr: &Context, x: f64, y: f64, r: f64) {
    const SIDES: [usize; 5] = [6, 8, 12, 16, 24];
    static TABLE: std::sync::OnceLock<[Vec<(f64, f64)>; 5]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        SIDES.map(|n| {
            let nf = n as f64;
            let s = (TAU / (nf * (TAU / nf).sin())).sqrt();
            (0..n).map(|i| ((TAU * i as f64 / nf).cos() * s, (TAU * i as f64 / nf).sin() * s)).collect()
        })
    });
    let k = if r < 1.6 {
        0
    } else if r < 3.0 {
        1
    } else if r < 6.0 {
        2
    } else if r < 12.0 {
        3
    } else {
        4
    };
    cr.new_sub_path();
    for (i, (c, s)) in t[k].iter().enumerate() {
        if i == 0 {
            cr.move_to(x + c * r, y + s * r);
        } else {
            cr.line_to(x + c * r, y + s * r);
        }
    }
    cr.close_path();
}

fn set_filter(cr: &Context) {
    cr.source().set_filter(Filter::Good);
}

/// An image stretched into the box.
fn draw_img(cr: &Context, img: &ImageSurface, x: f64, y: f64, w: f64, h: f64, alpha: f64) {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    if iw < 1.0 || ih < 1.0 || w < 0.5 || h < 0.5 || alpha < 0.004 {
        return;
    }
    save(cr);
    cr.translate(x, y);
    cr.scale(w / iw, h / ih);
    if cr.set_source_surface(img, 0.0, 0.0).is_ok() {
        set_filter(cr);
        let _ = cr.paint_with_alpha(alpha.clamp(0.0, 1.0));
    }
    restore(cr);
}

/// An image cropped to fill the box.
fn draw_cover(cr: &Context, img: &ImageSurface, x: f64, y: f64, w: f64, h: f64, alpha: f64) {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    if iw < 1.0 || ih < 1.0 || w < 0.5 || h < 0.5 || alpha < 0.004 {
        return;
    }
    let s = (w / iw).max(h / ih);
    let (dw, dh) = (iw * s, ih * s);
    save(cr);
    cr.rectangle(x, y, w, h);
    cr.clip();
    cr.translate(x + (w - dw) / 2.0, y + (h - dh) / 2.0);
    cr.scale(s, s);
    if cr.set_source_surface(img, 0.0, 0.0).is_ok() {
        set_filter(cr);
        let _ = cr.paint_with_alpha(alpha.clamp(0.0, 1.0));
    }
    restore(cr);
}

/// An image fitted inside a box, centred.
fn draw_fit(cr: &Context, img: &ImageSurface, cx: f64, cy: f64, max_w: f64, max_h: f64, alpha: f64) {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    if iw < 1.0 || ih < 1.0 {
        return;
    }
    let mut w = max_w;
    let mut h = w * ih / iw;
    if h > max_h {
        h = max_h;
        w = h * iw / ih;
    }
    draw_img(cr, img, cx - w / 2.0, cy - h / 2.0, w, h, alpha);
}

/// Strokes the current path with a soft halo round it (a stand-in for the canvas shadowBlur),
/// then the line itself. Consumes the path.
fn glow_stroke(cr: &Context, c: Rgb, alpha: f64, lw: f64, blur: f64) {
    // canvas shadowBlur is a gaussian of sigma = blur / 2; a thin line spreads its energy, so
    // the glow's strength follows the line width (a few wider, fainter strokes stand in for it)
    let sigma = (blur / 2.0).max(0.5);
    let peak = (lw / (sigma * 2.5066)).min(1.0);
    cr.set_line_join(LineJoin::Round);
    for m in [3.0, 2.0, 1.0] {
        cr.set_line_width(lw + 2.0 * sigma * m);
        src(cr, c, alpha * peak * 0.3);
        let _ = cr.stroke_preserve();
    }
    cr.set_line_width(lw);
    src(cr, c, alpha);
    stroke(cr);
}

/// Fills the current path with a soft halo round it. Consumes the path.
fn glow_fill(cr: &Context, c: Rgb, alpha: f64, blur: f64) {
    let sigma = (blur / 2.0).max(0.5);
    cr.set_line_join(LineJoin::Round);
    for m in [1.5, 1.0, 0.5] {
        cr.set_line_width(2.0 * sigma * m);
        src(cr, c, alpha * 0.12);
        let _ = cr.stroke_preserve();
    }
    src(cr, c, alpha);
    fill(cr);
}

/// A dot with a glow (canvas shadowBlur), as a few stacked discs; a small dot glows only faintly.
fn glow_dot(cr: &Context, x: f64, y: f64, r: f64, c: Rgb, alpha: f64, blur: f64) {
    let sigma = (blur / 2.0).max(0.5);
    let peak = (r * r / (2.0 * sigma * sigma)).min(1.0);
    for (m, am) in [(2.6, 0.2), (1.7, 0.3), (0.9, 0.5)] {
        cr.new_sub_path();
        cr.arc(x, y, r + sigma * m, 0.0, TAU);
        src(cr, c, alpha * peak * am);
        fill(cr);
    }
    cr.new_sub_path();
    cr.arc(x, y, r, 0.0, TAU);
    src(cr, c, alpha);
    fill(cr);
}

// ---------------------------------------------------------------------------------------------
// Neon: the logo (or the artist name) as a red tube with a pale core and a glow

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

/// A cheap content key for a logo: its size and 24x12 nearest samples of its alpha.
fn fingerprint(fp: &mut Option<ImageSurface>, img: &ImageSurface) -> u64 {
    let (w, h) = (img.width(), img.height());
    if fp.is_none() {
        *fp = ImageSurface::create(Format::A8, 24, 12).ok();
    }
    let Some(s) = fp.as_mut() else { return hash_of(&(w, h)) };
    {
        let Ok(c) = Context::new(&*s) else { return hash_of(&(w, h)) };
        c.set_operator(Operator::Clear);
        let _ = c.paint();
        c.set_operator(Operator::Over);
        c.scale(24.0 / (w.max(1) as f64), 12.0 / (h.max(1) as f64));
        if c.set_source_surface(img, 0.0, 0.0).is_ok() {
            c.source().set_filter(Filter::Nearest);
            let _ = c.paint();
        }
    }
    s.flush();
    let mut hh = std::collections::hash_map::DefaultHasher::new();
    (w, h).hash(&mut hh);
    if let Ok(d) = s.data() {
        d.hash(&mut hh);
    }
    hh.finish()
}

/// The artist name as a white-on-transparent sign, cropped to its ink. The letters are drawn as
/// outlines, like the bent tubes of a neon sign (the prototype's logos are line art too).
fn text_surface(txt: &str) -> Option<ImageSurface> {
    let up = txt.to_uppercase();
    let tmp = ImageSurface::create(Format::A8, 1, 1).ok()?;
    let c = Context::new(&tmp).ok()?;
    font(&c, ANTON, false, 120.0);
    let e = c.text_extents(&up).ok()?;
    let lw = 4.5;
    let pad = 4.0 + lw / 2.0;
    let w = (e.width() + 2.0 * pad).ceil().clamp(8.0, 4000.0);
    let h = (e.height() + 2.0 * pad).ceil().clamp(8.0, 600.0);
    drop(c);
    let s = ImageSurface::create(Format::A8, w as i32, h as i32).ok()?;
    {
        let c = Context::new(&s).ok()?;
        font(&c, ANTON, false, 120.0);
        c.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        c.move_to(pad - e.x_bearing(), pad - e.y_bearing());
        c.text_path(&up);
        c.set_line_width(lw);
        c.set_line_join(LineJoin::Round);
        let _ = c.stroke();
    }
    s.flush();
    Some(s)
}

/// Three box blurs make a gaussian; works on the alpha of an A8 surface.
fn blur_a8(s: &mut ImageSurface, sigma: f64) {
    let (w, h, stride) = (s.width() as usize, s.height() as usize, s.stride() as usize);
    s.flush();
    let Ok(mut data) = s.data() else { return };
    // box sizes for a gaussian of this sigma in three passes
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n)
        / (-4.0 * wl as f64 - 4.0))
        .round() as i32;
    let sizes: Vec<i32> = (0..3).map(|i| if i < m { wl } else { wu }).collect();
    let mut a: Vec<f32> = vec![0.0; w * h];
    let mut b: Vec<f32> = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            a[y * w + x] = data[y * stride + x] as f32;
        }
    }
    for sz in sizes {
        let r = ((sz - 1) / 2) as usize;
        let inv = 1.0 / (2 * r + 1) as f32;
        // horizontal
        for y in 0..h {
            let row = &a[y * w..(y + 1) * w];
            let out = &mut b[y * w..(y + 1) * w];
            let mut acc: f32 = row[..(r + 1).min(w)].iter().sum();
            for x in 0..w {
                out[x] = acc * inv;
                if x + r + 1 < w {
                    acc += row[x + r + 1];
                }
                if x >= r {
                    acc -= row[x - r];
                }
            }
        }
        // vertical
        for x in 0..w {
            let mut acc: f32 = (0..(r + 1).min(h)).map(|y| b[y * w + x]).sum();
            for y in 0..h {
                a[y * w + x] = acc * inv;
                if y + r + 1 < h {
                    acc += b[(y + r + 1) * w + x];
                }
                if y >= r {
                    acc -= b[(y - r) * w + x];
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            data[y * stride + x] = a[y * w + x].round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// A copy of the alpha of `src` placed in the middle of a bigger A8 surface, then blurred.
fn glow_surface(src: &ImageSurface, pad: i32, sigma: f64) -> Option<ImageSurface> {
    let (w, h) = (src.width(), src.height());
    let mut s = ImageSurface::create(Format::A8, w + 2 * pad, h + 2 * pad).ok()?;
    {
        let c = Context::new(&s).ok()?;
        c.set_source_surface(src, pad as f64, pad as f64).ok()?;
        let _ = c.paint();
    }
    blur_a8(&mut s, sigma);
    Some(s)
}

impl NeonState {
    /// The sign's source image (the logo, or the artist name drawn as outlined letters), or
    /// `None` when there is nothing to show. Rebuilds the cache when the content changed.
    fn refresh(&mut self, sc: &Scene) -> Option<ImageSurface> {
        let key = if let Some(l) = sc.neon_logo {
            fingerprint(&mut self.fp, l)
        } else if !sc.neon_text.trim().is_empty() {
            hash_of(&sc.neon_text)
        } else {
            return None;
        };
        if key != self.key || (sc.neon_logo.is_none() && self.text.is_none()) {
            self.key = key;
            self.sized = None;
            self.text = if sc.neon_logo.is_none() { text_surface(sc.neon_text) } else { None };
        }
        sc.neon_logo.cloned().or_else(|| self.text.clone())
    }

    /// Makes sure the images match the size the sign is drawn at.
    fn size_to(&mut self, img: &ImageSurface, wi: i32, hi: i32) -> Option<&NeonSized> {
        if self.sized.as_ref().is_none_or(|s| s.wi != wi || s.hi != hi) {
            let (iw, ih) = (img.width() as f64, img.height() as f64);
            let tube = ImageSurface::create(Format::A8, wi, hi).ok()?;
            {
                let c = Context::new(&tube).ok()?;
                c.scale(wi as f64 / iw, hi as f64 / ih);
                c.set_source_surface(img, 0.0, 0.0).ok()?;
                c.source().set_filter(Filter::Good);
                let _ = c.paint();
            }
            tube.flush();
            // the prototype's shadow blurs: max(10, 6% of the width) and max(4, 1.5% of the width)
            let wf = wi as f64;
            let sig_big = (wf * 0.06).max(10.0) / 2.0;
            let sig_small = (wf * 0.015).max(4.0) / 2.0;
            let pad_big = (sig_big * 3.0).ceil() as i32;
            let pad_small = (sig_small * 3.0).ceil() as i32;
            let big = glow_surface(&tube, pad_big, sig_big)?;
            let small = glow_surface(&tube, pad_small, sig_small)?;
            self.sized = Some(NeonSized { wi, hi, tube, big, small, pad_big, pad_small });
        }
        self.sized.as_ref()
    }
}

// ---------------------------------------------------------------------------------------------
// The painter: the prototype's closures, as methods

struct Painter<'a> {
    cr: &'a Context,
    cx: f64,
    hy: f64,
    f: f64,
    w: f64,
    h: f64,
    t: f64,
    flick: f64,
    gap: Option<usize>,
    neon: &'a mut NeonState,
    /// Where each place's screen was drawn (by place index).
    rects: [Option<[f64; 4]>; 6],
}

impl Painter<'_> {
    fn pr(&self, x: f64, z: f64, y: f64) -> (f64, f64) {
        (self.cx + self.f * x / (z + CZ), self.hy + self.f * (EYE - y) / (z + CZ))
    }

    fn path(&self, pts: &[(f64, f64)]) {
        self.cr.new_path();
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                self.cr.move_to(p.0, p.1);
            } else {
                self.cr.line_to(p.0, p.1);
            }
        }
    }

    /// A dotted line (zero-length round dashes).
    fn dots(&self, pts: &[(f64, f64)], a: f64, lw: f64, gap: f64, c: Rgb) {
        if a < 0.01 || pts.len() < 2 {
            return;
        }
        let cr = self.cr;
        save(cr);
        cr.set_dash(&[0.0, gap], 0.0);
        cr.set_line_cap(LineCap::Round);
        cr.set_line_width(lw);
        src(cr, c, a);
        self.path(pts);
        stroke(cr);
        restore(cr);
    }

    fn quad(&self, pts: &[(f64, f64)], c: Rgb, a: f64) {
        self.path(pts);
        self.cr.close_path();
        src(self.cr, c, a);
        fill(self.cr);
    }

    /// The side walls and the ceiling, as dark glass.
    fn room(&self, x: f64, yh: f64, zw: f64, a: f64, fill_a: f64, ceil_a: f64) {
        if a < 0.01 {
            return;
        }
        let zn = -CZ + 0.4;
        for sx in [-1.0, 1.0] {
            let q = [self.pr(sx * x, zn, 0.0), self.pr(sx * x, zw, 0.0), self.pr(sx * x, zw, yh), self.pr(sx * x, zn, yh)];
            self.quad(&q, BLACK, fill_a * a);
        }
        let q = [self.pr(-x, zn, yh), self.pr(x, zn, yh), self.pr(x, zw, yh), self.pr(-x, zw, yh)];
        self.quad(&q, BLACK, ceil_a * a);
    }

    fn edges(&self, x: f64, yh: f64, zw: f64, a: f64) {
        for sx in [-1.0, 1.0] {
            self.dots(&[self.pr(sx * x, 0.0, 0.0), self.pr(sx * x, zw, 0.0)], 0.5 * a, 2.2, 7.0, WHITE);
            self.dots(&[self.pr(sx * x, 0.0, yh), self.pr(sx * x, zw, yh)], 0.4 * a, 2.2, 7.0, WHITE);
        }
        self.dots(
            &[self.pr(-x, zw, 0.0), self.pr(x, zw, 0.0), self.pr(x, zw, yh), self.pr(-x, zw, yh), self.pr(-x, zw, 0.0)],
            0.45 * a,
            2.2,
            7.0,
            WHITE,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn wall_dots(&self, sx: f64, x: f64, z0: f64, z1: f64, y0: f64, y1: f64, a: f64) {
        let (st, r, al) = (0.15, 0.024, 0.4);
        let cr = self.cr;
        let mut any = false;
        cr.new_path();
        let nz = ((z1 - z0) / st + 1e-6).floor() as i32;
        let ny = ((y1 - y0) / st + 1e-6).floor() as i32;
        for iz in 0..=nz {
            let z = z0 + iz as f64 * st;
            for iy in 0..=ny {
                let y = y0 + iy as f64 * st;
                let (px, py) = self.pr(sx * x, z, y);
                let rr = r * self.f / (z + CZ);
                if rr < 0.35 {
                    continue;
                }
                dot(cr, px, py, rr);
                any = true;
            }
        }
        if any {
            src(cr, WHITE, al * a);
            fill(cr);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn beam(&self, x0: f64, z0: f64, y0: f64, x1: f64, z1: f64, w1: f64, a: f64) {
        let (ax, ay) = self.pr(x0, z0, y0);
        let (bx, by) = self.pr(x1 - w1, z1, 0.0);
        let (cx2, cy2) = self.pr(x1 + w1, z1, 0.0);
        let gr = LinearGradient::new(ax, ay, (bx + cx2) / 2.0, by);
        gr.add_color_stop_rgba(0.0, 1.0, 215.0 / 255.0, 218.0 / 255.0, (0.34 * a).clamp(0.0, 1.0));
        gr.add_color_stop_rgba(0.6, 1.0, 75.0 / 255.0, 83.0 / 255.0, (0.12 * a).clamp(0.0, 1.0));
        gr.add_color_stop_rgba(1.0, 1.0, 75.0 / 255.0, 83.0 / 255.0, 0.0);
        let _ = self.cr.set_source(&gr);
        self.path(&[(ax, ay), (bx, by), (cx2, cy2)]);
        self.cr.close_path();
        fill(self.cr);
    }

    /// A little rig of stage lights: n x m white dots with a red glow.
    fn lamp(&self, x: f64, z: f64, y: f64, a: f64, n: i32, m: i32) {
        let k = self.f / (z + CZ);
        let st = 0.32 * k;
        let (px, py) = self.pr(x, z, y);
        for i in 0..n {
            for j in 0..m {
                let dx = px + (i as f64 - (n - 1) as f64 / 2.0) * st;
                let dy = py + (j as f64 - (m - 1) as f64 / 2.0) * st;
                glow_dot(self.cr, dx, dy, (0.1 * k).max(1.2), (255.0, 240.0, 240.0), 0.95 * a, 14.0);
            }
        }
    }

    fn panel(&self, r: [f64; 4], a: f64) {
        let cr = self.cr;
        src(cr, (6.0, 6.0, 7.0), 0.88 * a);
        cr.rectangle(r[0] - 5.0, r[1] - 5.0, r[2] + 10.0, r[3] + 10.0);
        fill(cr);
        src(cr, WHITE, 0.45 * a);
        cr.set_line_width(1.2);
        cr.rectangle(r[0] - 5.0, r[1] - 5.0, r[2] + 10.0, r[3] + 10.0);
        stroke(cr);
    }

    /// A box on the wall plane at depth z, from world x/y ranges, as screen x, y, w, h.
    fn scr(&self, x0: f64, x1: f64, y0: f64, y1: f64, z: f64) -> [f64; 4] {
        let (a0, b0) = self.pr(x0, z, y1);
        let (a1, b1) = self.pr(x1, z, y0);
        [a0, b0, a1 - a0, b1 - b0]
    }

    /// The neon sign: the logo (or the artist's name) as a red tube with a pale core.
    fn neon(&mut self, sc: &Scene, r: [f64; 4], a: f64) -> bool {
        if a < 0.01 {
            return false;
        }
        let Some(img) = self.neon.refresh(sc) else { return false };
        let (iw, ih) = (img.width() as f64, img.height() as f64);
        if iw < 1.0 || ih < 1.0 {
            return false;
        }
        let ar = iw / ih;
        // a sign made from plain text gets a little air round it
        let k = if sc.neon_logo.is_some() { 1.0 } else { 0.85 };
        let mut w = r[2] * k;
        let mut h = w / ar;
        if h > r[3] * k {
            h = r[3] * k;
            w = h * ar;
        }
        if w < 4.0 || h < 2.0 || w > 4000.0 || h > 4000.0 {
            return false;
        }
        // whole pixels, so the cached images are painted without scaling
        let (wi, hi) = (w.round().max(2.0) as i32, h.round().max(2.0) as i32);
        let x = (r[0] + (r[2] - wi as f64) / 2.0).round();
        let y = (r[1] + (r[3] - hi as f64) / 2.0).round();
        let cr = self.cr;
        let fl = self.flick;
        let Some(n) = self.neon.size_to(&img, wi, hi) else { return false };
        // the glow, twice as in the prototype
        src(cr, NEON_RED, a * fl);
        for _ in 0..2 {
            let _ = cr.mask_surface(&n.big, x - n.pad_big as f64, y - n.pad_big as f64);
        }
        // the tube
        for _ in 0..2 {
            let _ = cr.mask_surface(&n.tube, x, y);
        }
        // the tight glow round the core, then the core
        src(cr, NEON_RED, a * 0.78 * fl);
        let _ = cr.mask_surface(&n.small, x - n.pad_small as f64, y - n.pad_small as f64);
        src(cr, NEON_CORE, a * 0.78 * fl);
        let _ = cr.mask_surface(&n.tube, x, y);
        true
    }

    /// The studio's shelf of records, with a gap (a dotted frame and a glow) for the playing album.
    fn records_shelf(&self, sc: &Scene, r: [f64; 4], a: f64) {
        let others = sc.records;
        if others.is_empty() || a < 0.01 {
            return;
        }
        let cr = self.cr;
        let n = others.len() + 1;
        let gi = self.gap.unwrap_or(1).min(others.len());
        let nf = n as f64;
        let gap = r[2] * 0.035;
        let cw = r[3].min((r[2] - gap * (nf - 1.0)) / nf);
        let tw = cw * nf + gap * (nf - 1.0);
        let mut x = r[0] + (r[2] - tw) / 2.0;
        let yb = r[1] + r[3];
        self.dots(&[(r[0] - cw * 0.2, yb + 3.0), (r[0] + r[2] + cw * 0.2, yb + 3.0)], 0.6 * a, 2.4, 5.0, WHITE);
        for i in 0..n {
            if i == gi {
                let gr = RadialGradient::new(x + cw / 2.0, yb, 0.0, x + cw / 2.0, yb, cw * 0.9);
                gr.add_color_stop_rgba(0.0, 1.0, 42.0 / 255.0, 53.0 / 255.0, (0.55 * a).clamp(0.0, 1.0));
                gr.add_color_stop_rgba(1.0, 1.0, 42.0 / 255.0, 53.0 / 255.0, 0.0);
                let _ = cr.set_source(&gr);
                cr.rectangle(x - cw * 0.2, yb - cw * 1.1, cw * 1.4, cw * 1.1);
                fill(cr);
                save(cr);
                cr.set_dash(&[0.0, 5.0], 0.0);
                cr.set_line_cap(LineCap::Round);
                cr.rectangle(x + 2.0, yb - cw + 2.0, cw - 4.0, cw - 4.0);
                glow_stroke(cr, (255.0, 120.0, 126.0), a, 2.6, 8.0);
                restore(cr);
            } else {
                let img = &others[if i < gi { i } else { i - 1 }];
                draw_img(cr, img, x, yb - cw, cw, cw, a * 0.85);
            }
            x += cw + gap;
        }
    }

    fn has_neon(sc: &Scene) -> bool {
        sc.neon_logo.is_some() || !sc.neon_text.trim().is_empty()
    }

    /// What hangs on the studio's back wall: the "RECORDING" light, the neon name, the shelf.
    fn venue_studio(&mut self, sc: &Scene, z: f64, a: f64) {
        let cr = self.cr;
        if matches!(sc.screen, Screen::None) {
            let rr = self.scr(1.95, 3.05, 2.72, 2.98, z);
            let lit = sc.playing;
            save(cr);
            rrect(cr, rr[0], rr[1], rr[2], rr[3], 3.0);
            if lit {
                glow_fill(cr, (255.0, 42.0, 53.0), a, 16.0);
            } else {
                src(cr, (30.0, 10.0, 12.0), 0.9 * a);
                fill(cr);
            }
            restore(cr);
            src(cr, WHITE, if lit { a } else { 0.3 * a });
            font(cr, NARROW, true, (rr[3] * 0.62).max(7.0).round());
            text_spaced(cr, "RECORDING", rr[0] + rr[2] / 2.0, rr[1] + rr[3] * 0.74, 1.5);
        }
        if Self::has_neon(sc) {
            let r = self.scr(-1.9, 1.9, 2.16, 3.06, z);
            self.neon(sc, r, a);
        }
        let r = self.scr(-3.0, 3.0, 1.58, 2.12, z);
        self.records_shelf(sc, r, a * 0.72);
    }

    /// The concert stage shows the neon name only.
    fn venue_concert(&mut self, sc: &Scene, z: f64, a: f64) {
        if Self::has_neon(sc) {
            let r = self.scr(-4.4, 4.4, 3.3, 6.1, z);
            self.neon(sc, r, a);
        }
    }

    /// The stadium's big screen: a score, a picture or a title; or the neon name for a live song.
    fn venue_stadium(&mut self, sc: &Scene, z: f64, a: f64) {
        let r = self.scr(-10.0, 10.0, 4.0, 15.0, z);
        if !matches!(sc.screen, Screen::None) {
            self.rects[Place::Stadium.idx()] = Some(r);
            self.big_screen(sc, r, a);
        } else if Self::has_neon(sc) {
            self.panel(r, a);
            let inner = [r[0] + r[2] * 0.12, r[1] + r[3] * 0.24, r[2] * 0.76, r[3] * 0.48];
            self.neon(sc, inner, a);
        }
    }

    fn big_screen(&self, sc: &Scene, r: [f64; 4], a: f64) {
        let cr = self.cr;
        self.panel(r, a);
        let [x, y, w, h] = r;
        save(cr);
        cr.rectangle(x, y, w, h);
        cr.clip();
        match sc.screen {
            Screen::Score { home, away, home_badge, away_badge } => {
                src(cr, WHITE, a);
                let bs = (w * 0.3).min(h * 0.42);
                for (i, b) in [home_badge, away_badge].into_iter().enumerate() {
                    if let Some(im) = b {
                        let bx = x + w * if i == 1 { 0.78 } else { 0.22 };
                        draw_img(cr, im, bx - bs / 2.0, y + h * 0.4 - bs / 2.0, bs, bs, a);
                    }
                }
                src(cr, WHITE, a);
                fit_text(cr, "v", x + w / 2.0, y + h * 0.4, w * 0.2, h * 0.16);
                for (i, t) in [home, away].into_iter().enumerate() {
                    let bx = x + w * if i == 1 { 0.78 } else { 0.22 };
                    fit_text(cr, &t.to_uppercase(), bx, y + h * 0.78, w * 0.3, h * 0.16);
                }
            }
            Screen::Logo(im) => draw_fit(cr, im, x + w / 2.0, y + h / 2.0, w * 0.56, h * 0.5, a),
            Screen::Picture(im) => draw_cover(cr, im, x, y, w, h, a),
            Screen::Title(t) => {
                src(cr, WHITE, a);
                fit_text(cr, &t.to_uppercase(), x + w / 2.0, y + h / 2.0, w * 0.86, h * 0.2);
            }
            Screen::Video | Screen::None => {}
        }
        restore(cr);
    }

    /// The cinema screen's picture (inside the dark frame).
    fn cinema_screen(&self, sc: &Scene, ax: f64, ay: f64, bx: f64, by: f64, a: f64) {
        let cr = self.cr;
        let (w, h) = (bx - ax, by - ay);
        src(cr, BLACK, 0.5 * a);
        cr.rectangle(ax, ay, w, h);
        fill(cr);
        save(cr);
        cr.rectangle(ax, ay, w, h);
        cr.clip();
        match sc.screen {
            Screen::Logo(im) => draw_fit(cr, im, (ax + bx) / 2.0, (ay + by) / 2.0, w * 0.66, h * 0.8, a),
            Screen::Picture(im) => draw_cover(cr, im, ax, ay, w, h, a),
            Screen::Title(t) => {
                src(cr, WHITE, a);
                font(cr, ANTON, false, (h * 0.2).round());
                text_mid(cr, &t.to_uppercase(), (ax + bx) / 2.0, (ay + by) / 2.0);
            }
            Screen::Score { home, away, .. } => {
                src(cr, WHITE, a);
                font(cr, ANTON, false, (h * 0.2).round());
                text_mid(cr, &format!("{} v {}", home.to_uppercase(), away.to_uppercase()), (ax + bx) / 2.0, (ay + by) / 2.0);
            }
            Screen::Video | Screen::None => {}
        }
        restore(cr);
    }

    /// The game monitor: the logo (or the title), a picture, or a dark pane for a video.
    fn monitor(&self, sc: &Scene, ax: f64, ay: f64, bx: f64, by: f64, a: f64) {
        let cr = self.cr;
        let (w, h) = (bx - ax, by - ay);
        save(cr);
        cr.rectangle(ax, ay, w, h);
        cr.clip();
        match sc.screen {
            Screen::Video => {
                src(cr, (4.0, 4.0, 5.0), 0.92 * a);
                cr.paint().ok();
            }
            Screen::Picture(im) => {
                src(cr, (4.0, 4.0, 5.0), 0.92 * a);
                cr.paint().ok();
                draw_cover(cr, im, ax, ay, w, h, a);
            }
            Screen::Logo(im) => {
                src(cr, (4.0, 4.0, 5.0), 0.88 * a);
                cr.paint().ok();
                draw_fit(cr, im, ax + w / 2.0, ay + h / 2.0, w * 0.62, h * 0.6, a);
            }
            Screen::Title(t) => {
                src(cr, (4.0, 4.0, 5.0), 0.88 * a);
                cr.paint().ok();
                src(cr, WHITE, a);
                fit_text(cr, &t.to_uppercase(), ax + w / 2.0, ay + h / 2.0, w * 0.82, h * 0.2);
            }
            Screen::Score { home, away, .. } => {
                src(cr, (4.0, 4.0, 5.0), 0.88 * a);
                cr.paint().ok();
                src(cr, WHITE, a);
                fit_text(cr, &format!("{} v {}", home.to_uppercase(), away.to_uppercase()), ax + w / 2.0, ay + h / 2.0, w * 0.82, h * 0.2);
            }
            Screen::None => {
                src(cr, (4.0, 4.0, 5.0), 0.88 * a);
                cr.paint().ok();
            }
        }
        restore(cr);
    }

    /// A speaker printed in dots: lit ones bright, with red dots for the cones.
    #[allow(clippy::too_many_arguments)]
    fn speaker(&self, x0: f64, y0: f64, w: f64, h: f64, on: bool, hot: bool, lv: f64, sub: bool, ga: f64) {
        let cr = self.cr;
        let step = (w / 8.5).max(3.2);
        let cx = x0 + w / 2.0;
        let cy = y0 + h * if sub { 0.5 } else { 0.62 };
        let cone = w.min(h) * if sub { 0.36 } else { 0.32 } * (1.0 + 0.1 * lv);
        let ty = y0 + h * 0.22;
        let tr = cone * 0.42;
        let mut cone_path = false;
        // body dots, one fill per column (their brightness depends on the column only)
        let mut x = x0 + step / 2.0;
        while x < x0 + w {
            let lit = 0.6 + 0.4 * (1.0 - (x - x0) / w);
            cr.new_path();
            let mut any = false;
            let mut y = y0 + step / 2.0;
            while y < y0 + h {
                let dc = (x - cx).hypot(y - cy);
                let dt = if sub { 1e9 } else { (x - cx).hypot(y - ty) };
                if dc < cone || dt < tr {
                    cone_path = true;
                } else {
                    dot(cr, x, y, step * 0.42 * lit);
                    any = true;
                }
                y += step;
            }
            if any {
                if hot {
                    src(cr, RED, ga);
                } else if on {
                    src(cr, WHITE, (0.5 + 0.45 * lit) * ga);
                } else {
                    src(cr, WHITE, (0.13 + 0.12 * lit) * ga);
                }
                fill(cr);
            }
            x += step;
        }
        if cone_path {
            cr.new_path();
            let mut x = x0 + step / 2.0;
            while x < x0 + w {
                let mut y = y0 + step / 2.0;
                while y < y0 + h {
                    let dc = (x - cx).hypot(y - cy);
                    let dt = if sub { 1e9 } else { (x - cx).hypot(y - ty) };
                    if dc < cone || dt < tr {
                        dot(cr, x, y, step * 0.47);
                    }
                    y += step;
                }
                x += step;
            }
            if hot {
                src(cr, WHITE, ga);
            } else if on {
                src(cr, RED, ga);
            } else {
                src(cr, WHITE, 0.3 * ga);
            }
            fill(cr);
        }
    }

    fn stand(&self, sx: f64, a: f64, b: f64, alpha: f64) {
        let cr = self.cr;
        src(cr, WHITE, 0.5 * alpha);
        cr.set_line_width(1.6);
        cr.new_path();
        cr.move_to(sx, a);
        cr.line_to(sx, b);
        stroke(cr);
    }
}

// ---------------------------------------------------------------------------------------------
// The frame

pub fn draw(cr: &Context, w: f64, h: f64, dt: f64, sc: &Scene, sm: &mut Smooth) -> Layout {
    let mut lay = Layout::default();
    if w < 2.0 || h < 2.0 {
        return lay;
    }
    let hy = h * 0.44;
    let mut f = h * 0.82;
    let mut cx = w * 0.72;
    // with a split page, the room moves (and if needed shrinks) so it sits wholly right of the seam
    let seam = sc.left_clip.clamp(0.0, w - 1.0);
    if seam > 0.0 {
        let kq = 0.465;
        f = f.min((w - seam - 44.0) / (2.0 * kq)).max(60.0);
        cx = cx.max(seam + 30.0 + kq * f).min(w - 12.0 - kq * f);
    }

    // what the scene asks for
    let pk = if sc.sbx_on { sc.place } else { Place::Plain };
    let pi = pk.idx();
    let spread = if sc.sbx_on { unit(sc.spread) } else { 0.0 };
    let hp = sc.headphones;
    let sg_t = if pk == Place::Studio { unit(sc.studio_grow) } else { 0.0 };
    let (zw_t, x_t) = if pk == Place::Studio {
        (PL[1].zw + 1.7 * sg_t, PL[1].x + 0.9 * sg_t)
    } else {
        (PL[pi].zw, PL[pi].x)
    };
    let rad_t = zone_r(sc, spread, 2.4, 5.6);
    let half_t = if sc.around || hp { PI } else { (30.0 + spread * 55.0) * PI / 180.0 };
    let hp_t = if hp { 1.0 } else { 0.0 };

    // smoothed state: the place cross-fades, the sound area and the headphones ease in
    if !sm.init {
        sm.init = true;
        for k in 0..6 {
            sm.w[k] = if k == pi { 1.0 } else { 0.0 };
        }
        sm.zw = zw_t;
        sm.x = x_t;
        sm.rad = rad_t;
        sm.half = half_t;
        sm.hp = hp_t;
        sm.sg = sg_t;
    }
    let dt = dt.clamp(0.0, 0.1);
    let kk = 1.0 - (-dt * 4.5).exp();
    for k in 0..6 {
        let tg = if k == pi { 1.0 } else { 0.0 };
        sm.w[k] += (tg - sm.w[k]) * kk;
    }
    sm.sg += (sg_t - sm.sg) * kk;
    sm.zw += (zw_t - sm.zw) * kk;
    sm.x += (x_t - sm.x) * kk;
    sm.rad += (rad_t - sm.rad) * kk;
    sm.half += (half_t - sm.half) * kk;
    sm.hp += (hp_t - sm.hp) * kk;
    let mut animating = sm.w.iter().any(|v| *v > 0.003 && *v < 0.997)
        || (sm.hp - hp_t).abs() > 0.003
        || (sm.sg - sg_t).abs() > 0.003
        || (sm.rad - rad_t).abs() > 0.01
        || (sm.half - half_t).abs() > 0.003
        || (sm.zw - zw_t).abs() > 0.05 * (1.0 + 0.01 * zw_t)
        || (sm.x - x_t).abs() > 0.05 * (1.0 + 0.01 * x_t);

    // the energy: the live meters stand in for the prototype's synthetic beat
    let lv_of = |i: usize| -> f64 {
        if sc.active[i] && !hp && sc.playing { unit(sc.levels[i]) } else { 0.0 }
    };
    let (bass, hi) = if sc.playing {
        let lfe = unit(sc.levels[I_LFE]);
        let fronts = (unit(sc.levels[I_FL]) + unit(sc.levels[I_FR])) / 2.0;
        let all = sc.levels.iter().map(|v| unit(*v)).sum::<f64>() / 6.0;
        ((0.3 + 0.7 * lfe.max(0.6 * fronts)).min(1.0), (0.3 + 0.7 * all).min(1.0))
    } else {
        (0.0, 0.0)
    };

    let col = tone_color(sc);
    let flick = 0.9 + 0.1 * (sc.t * 23.0).sin() * (sc.t * 7.1).sin();

    // split borrows: the painter takes the neon cache, the rest of `sm` is read here
    let wts = sm.w;
    let (sm_zw, sm_x, sm_rad, sm_half, sm_hp, sm_sg) = (sm.zw, sm.x, sm.rad, sm.half, sm.hp, sm.sg);
    let gap = sm.record_gap;
    let hot_id = match sm.hot {
        Some((i, until)) if sc.t < until => Some(i),
        _ => None,
    };
    let ripples: Vec<(usize, f64)> = sm.ripples.iter().copied().filter(|(_, t0)| sc.t >= *t0 && sc.t - *t0 <= 1.2).collect();
    sm.ripples = ripples.clone();
    if hot_id.is_none() {
        sm.hot = None;
    }
    if !ripples.is_empty() || hot_id.is_some() {
        animating = true;
    }
    let mut p = Painter { cr, cx, hy, f, w, h, t: sc.t, flick, gap, neon: &mut sm.neon, rects: [None; 6] };

    save(cr);
    if seam > 0.0 {
        let cx0 = seam.floor();
        cr.rectangle(cx0, 0.0, w - cx0, h);
        cr.clip();
    }

    // dressed for the place: the room first (glass), then what hangs on its walls
    // studio
    let sgv = sm_sg;
    let studio = (PL[1].x + 0.9 * sgv, PL[1].yh + 0.5 * sgv, PL[1].zw + 1.7 * sgv);
    if wts[1] > 0.01 {
        p.room(studio.0, studio.1, studio.2, wts[1], 0.24, 0.32);
    }
    if wts[2] > 0.01 {
        p.room(PL[2].x, PL[2].yh, PL[2].zw, wts[2], 0.2, 0.36);
    }
    if wts[4] > 0.01 {
        p.room(PL[4].x, PL[4].yh, PL[4].zw, wts[4], 0.38, 0.48);
    }
    if wts[5] > 0.01 {
        p.room(PL[5].x, PL[5].yh, PL[5].zw, wts[5], 0.3, 0.42);
    }

    // the walls' dressing
    if wts[1] > 0.01 {
        let (x, yh, zw) = studio;
        let a = wts[1];
        p.edges(x, yh, zw, a);
        for sx in [-1.0, 1.0] {
            for (z0, z1) in [(1.3, 2.1), (2.4, 3.2), (3.45, 4.15)] {
                p.wall_dots(sx, x, z0, z1, 0.85, 2.3, a);
            }
        }
        p.venue_studio(sc, zw, a);
    }
    if wts[2] > 0.01 {
        let Pl { zw, x, yh } = PL[2];
        let a = wts[2];
        p.edges(x, yh, zw, a);
        for sx in [-1.0, 1.0] {
            for y in [2.6, 4.6] {
                p.dots(&[p.pr(sx * x, 1.0, y), p.pr(sx * x, zw, y)], 0.42 * a, 2.4, 8.0, WHITE);
                p.dots(&[p.pr(sx * x * 0.92, 1.0, y - 0.5), p.pr(sx * x * 0.92, zw, y - 0.5)], 0.22 * a, 1.8, 8.0, WHITE);
            }
        }
        // the stage
        let q = [p.pr(-5.4, 9.6, 0.0), p.pr(5.4, 9.6, 0.0), p.pr(5.4, 9.6, 0.9), p.pr(-5.4, 9.6, 0.9)];
        p.quad(&q, BLACK, 0.6 * a);
        let q = [p.pr(-5.4, 9.6, 0.9), p.pr(5.4, 9.6, 0.9), p.pr(5.4, zw, 0.9), p.pr(-5.4, zw, 0.9)];
        p.quad(&q, BLACK, 0.6 * a);
        p.dots(&[p.pr(-5.4, 9.6, 0.9), p.pr(5.4, 9.6, 0.9)], 0.75 * a, 2.6, 6.0, WHITE);
        p.venue_concert(sc, zw, a);
        p.dots(&[p.pr(-5.8, 10.4, 7.0), p.pr(5.8, 10.4, 7.0)], 0.55 * a, 2.6, 6.0, WHITE);
        for i in 0..5 {
            p.lamp(-4.0 + i as f64 * 2.0, 10.4, 6.85, a, 2, 1);
        }
    }
    if wts[3] > 0.01 {
        let a = wts[3];
        let (zc, rr) = (30.0, 27.0);
        for (y, al, wd) in [(16.0, 0.5, 2.6), (9.0, 0.32, 2.2), (2.2, 0.22, 2.0)] {
            let mut pts = Vec::with_capacity(81);
            for i in 0..=80 {
                let an = -1.25 + 2.5 * i as f64 / 80.0;
                let (x, z) = (an.sin() * rr, zc + an.cos() * rr);
                if z + CZ < 1.0 {
                    continue;
                }
                pts.push(p.pr(x, z, y));
            }
            p.dots(&pts, al * a, wd, 7.0, WHITE);
        }
        for i in 0..=12 {
            let an = -1.2 + 2.4 * i as f64 / 12.0;
            let (x, z) = (an.sin() * rr, zc + an.cos() * rr);
            p.dots(&[p.pr(x, z, 2.2), p.pr(x, z, 16.0)], 0.16 * a, 1.8, 8.0, WHITE);
        }
        p.venue_stadium(sc, zc + rr - 3.0, a);
        for an in [-0.95_f64, -0.45, 0.45, 0.95] {
            let (x, z) = (an.sin() * rr, zc + an.cos() * rr);
            p.dots(&[p.pr(x, z, 16.0), p.pr(x, z, 21.0)], 0.5 * a, 2.0, 5.0, WHITE);
            p.lamp(x, z, 21.6, a, 4, 3);
        }
    }
    if wts[4] > 0.01 {
        let Pl { zw, x, yh } = PL[4];
        let a = wts[4];
        let (sx0, sx1, sy0, sy1) = (-3.3, 3.3, 0.9, 4.6);
        let cr = p.cr;
        // the wall with a hole where the screen is
        let q = [p.pr(-x, zw, 0.0), p.pr(x, zw, 0.0), p.pr(x, zw, yh), p.pr(-x, zw, yh)];
        let s2 = [p.pr(sx0, zw, sy0), p.pr(sx1, zw, sy0), p.pr(sx1, zw, sy1), p.pr(sx0, zw, sy1)];
        cr.set_fill_rule(FillRule::EvenOdd);
        p.path(&q);
        cr.close_path();
        for (i, pt) in s2.iter().enumerate() {
            if i == 0 {
                cr.move_to(pt.0, pt.1);
            } else {
                cr.line_to(pt.0, pt.1);
            }
        }
        cr.close_path();
        src(cr, BLACK, 0.72 * a);
        fill(cr);
        cr.set_fill_rule(FillRule::Winding);
        let (ax, ay) = p.pr(sx0, zw, sy1);
        let (bx, by) = p.pr(sx1, zw, sy0);
        p.cinema_screen(sc, ax, ay, bx, by, a);
        p.rects[4] = Some([ax, ay, bx - ax, by - ay]);
        // the screen's frame, glowing
        p.path(&[s2[0], s2[1], s2[2], s2[3], s2[0]]);
        glow_stroke(cr, WHITE, 0.55 * a, 1.4, 18.0);
        for sd in [-1.0, 1.0] {
            for i in 0..9 {
                let xx = sd * (sx1 + 0.12 + i as f64 * 0.26);
                if xx.abs() > x - 0.1 {
                    continue;
                }
                p.dots(
                    &[p.pr(xx, zw - 0.05, 0.0), p.pr(xx, zw - 0.05, yh)],
                    (0.18 + 0.3 * (0.5 + 0.5 * (i as f64 * 1.9).sin())) * a,
                    2.2,
                    6.0,
                    WALL_RED,
                );
            }
        }
        p.dots(&[p.pr(-x, zw, yh - 0.2), p.pr(x, zw, yh - 0.2)], 0.45 * a, 2.4, 6.0, WALL_RED);
        p.edges(x, yh, zw, a * 0.7);
    }
    if wts[5] > 0.01 {
        let Pl { zw, x, yh } = PL[5];
        let a = wts[5];
        let (mx, my0, my1) = (2.55, 0.95, 3.75);
        let cr = p.cr;
        // a red glow behind the monitor
        let (gx, gy) = p.pr(0.0, zw, (my0 + my1) / 2.0);
        let gw = p.f * mx * 2.0 / (zw + CZ);
        let gr = RadialGradient::new(gx, gy, gw * 0.2, gx, gy, gw * 0.95);
        gr.add_color_stop_rgba(0.0, 1.0, 40.0 / 255.0, 50.0 / 255.0, (0.42 * a).clamp(0.0, 1.0));
        gr.add_color_stop_rgba(1.0, 1.0, 40.0 / 255.0, 50.0 / 255.0, 0.0);
        let _ = cr.set_source(&gr);
        cr.rectangle(gx - gw, gy - gw, gw * 2.0, gw * 2.0);
        fill(cr);
        // the wall, with the monitor cut out (the page behind it shows through a little)
        let q = [p.pr(-x, zw, 0.0), p.pr(x, zw, 0.0), p.pr(x, zw, yh), p.pr(-x, zw, yh)];
        let m = [p.pr(-mx, zw, my0), p.pr(mx, zw, my0), p.pr(mx, zw, my1), p.pr(-mx, zw, my1)];
        cr.set_fill_rule(FillRule::EvenOdd);
        p.path(&q);
        cr.close_path();
        for (i, pt) in m.iter().enumerate() {
            if i == 0 {
                cr.move_to(pt.0, pt.1);
            } else {
                cr.line_to(pt.0, pt.1);
            }
        }
        cr.close_path();
        src(cr, BLACK, 0.28 * a);
        fill(cr);
        cr.set_fill_rule(FillRule::Winding);
        // the bezel
        p.path(&[m[0], m[1], m[2], m[3], m[0]]);
        cr.close_path();
        src(cr, (20.0, 20.0, 22.0), 0.95 * a);
        cr.set_line_width((0.12 * p.f / (zw + CZ)).max(4.0));
        let _ = cr.stroke_preserve();
        src(cr, WHITE, 0.5 * a);
        cr.set_line_width(1.2);
        stroke(cr);
        // the foot
        let foot = [p.pr(-0.18, zw, my0), p.pr(0.18, zw, my0), p.pr(0.18, zw, 0.35), p.pr(-0.18, zw, 0.35)];
        p.quad(&foot, (20.0, 20.0, 22.0), a);
        p.edges(x, yh, zw, a * 0.6);
        // the red light strips along the floor edges
        cr.set_line_cap(LineCap::Butt);
        let l1 = [p.pr(-x, 0.0, 0.03), p.pr(-x, zw, 0.03)];
        let l2 = [p.pr(x, 0.0, 0.03), p.pr(x, zw, 0.03)];
        let l3 = [p.pr(-x, zw, 0.03), p.pr(x, zw, 0.03)];
        for l in [l1, l2, l3] {
            p.path(&l);
            glow_stroke(cr, NEON_RED, 0.9 * a, 2.4, 12.0);
        }
        let (ax, ay) = p.pr(-mx, zw, my1);
        let (bx, by) = p.pr(mx, zw, my0);
        p.monitor(sc, ax, ay, bx, by, a);
        p.rects[5] = Some([ax, ay, bx - ax, by - ay]);
    }

    // the floor
    let zf = sm_zw.min(60.0);
    let yb = hy.max(p.pr(0.0, zf, 0.0).1);
    let xc = sm_x;
    {
        let gr = LinearGradient::new(0.0, yb, 0.0, h);
        gr.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
        gr.add_color_stop_rgba(0.3, 0.0, 0.0, 0.0, 0.5);
        gr.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.94);
        let _ = cr.set_source(&gr);
        cr.rectangle(0.0, yb, w, h - yb);
        fill(cr);
    }
    cr.set_line_width(1.0);
    for xi in -8..=8 {
        let x = xi as f64;
        if x.abs() > xc + 1e-6 {
            continue;
        }
        let a = p.pr(x, -3.6, 0.0);
        let b = p.pr(x, zf.min(16.0), 0.0);
        let gr = LinearGradient::new(a.0, a.1, b.0, b.1);
        gr.add_color_stop_rgba(0.0, 1.0, 59.0 / 255.0, 68.0 / 255.0, 0.22);
        gr.add_color_stop_rgba(1.0, 1.0, 59.0 / 255.0, 68.0 / 255.0, 0.0);
        let _ = cr.set_source(&gr);
        cr.new_path();
        cr.move_to(a.0, a.1);
        cr.line_to(b.0, b.1);
        stroke(cr);
    }
    for z in [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 9.0] {
        if z >= zf {
            continue;
        }
        let xl = xc.min(40.0);
        let (x1, y) = p.pr(-xl, z, 0.0);
        let (x2, _) = p.pr(xl, z, 0.0);
        src(cr, FLOOR_RED, 0.22 * (1.0 - z / 11.0).max(0.0));
        cr.new_path();
        cr.move_to(x1.max(0.0), y);
        cr.line_to(x2.min(w), y);
        stroke(cr);
    }

    // the beams of light, in front of the walls and over the floor
    if wts[2] > 0.01 {
        let a = wts[2];
        for i in 0..5 {
            let x = -4.0 + i as f64 * 2.0;
            p.beam(x, 10.4, 6.8, x * 0.55, 10.2, 1.1, a * (0.75 + 0.25 * hi));
        }
    }
    if wts[3] > 0.01 {
        let a = wts[3];
        let (zc, rr) = (30.0, 27.0);
        for an in [-0.95_f64, -0.45, 0.45, 0.95] {
            let (x, z) = (an.sin() * rr, zc + an.cos() * rr);
            p.beam(x, z, 21.0, x * 0.25, zc * 0.8, 4.0, a * 0.8);
        }
    }

    // where the sound reaches: red dots that grow with the bass, as far as the zone goes
    {
        let (rad, half) = (sm_rad, sm_half);
        let st = 0.18;
        cr.new_path();
        let mut any = false;
        let z_start = -rad.min(3.4);
        let nz = ((rad + 0.01 - z_start) / st).ceil().max(0.0) as i32;
        let nx = ((2.0 * rad + 1e-9) / st).floor().max(0.0) as i32;
        for iz in 0..=nz {
            let z = z_start + iz as f64 * st;
            if z >= rad + 0.01 {
                break;
            }
            for ix in 0..=nx {
                let x = -rad + ix as f64 * st;
                if z >= zf - 0.1 || x.abs() > xc - 0.08 {
                    continue;
                }
                let r = x.hypot(z);
                if r > rad || r < 0.3 {
                    continue;
                }
                if x.abs().atan2(z) > half {
                    continue;
                }
                let fall = 1.0 - ((r - rad * 0.3) / (rad * 0.7)).max(0.0);
                let (sx, sy) = p.pr(x, z, 0.0);
                if sx < -20.0 || sx > w + 20.0 || sy > h + 20.0 {
                    continue;
                }
                let rd = (0.03 + 0.035 * bass) * fall * f / (z + CZ);
                if rd < 0.4 {
                    continue;
                }
                dot(cr, sx, sy, rd);
                any = true;
            }
        }
        if any {
            src(cr, col, 1.0);
            cr.set_antialias(gtk::cairo::Antialias::Fast);
            fill(cr);
            cr.set_antialias(gtk::cairo::Antialias::Default);
        }
    }

    // your speakers, printed in dots; they fade back when the headphones go on
    let sa = 1.0 - 0.5 * sm_hp;
    let mut hits: [(f64, f64, f64, f64); 6] = [(0.0, 0.0, 0.0, 0.0); 6];
    let mut put = |p: &Painter, idx: usize, x0: f64, y0: f64, w_: f64, h_: f64| {
        let on = sc.active[idx] && !hp;
        let hot = hot_id == Some(idx);
        let ga = if hot { 1.0 } else { sa };
        p.speaker(x0, y0, w_, h_, on, hot, lv_of(idx), idx == I_LFE, ga);
        hits[idx] = (x0 - 8.0, y0 - 8.0, w_ + 16.0, h_ + 16.0);
    };
    // far to near: centre, the two fronts, the sub
    let mut front = [(I_LFE, 2.1, 1.9), (I_FL, -1.25, 2.17), (I_FC, 0.0, 2.5), (I_FR, 1.25, 2.17)];
    front.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    for (idx, x, z) in front {
        let sub = idx == I_LFE;
        let (top, bot) = if sub { (0.62, 0.0) } else { (1.55, 0.82) };
        let sw = (if sub { 0.62 } else { 0.44 }) * f / (z + CZ);
        let (sx, sy0) = p.pr(x, z, top);
        let sy1 = p.pr(x, z, bot).1;
        let fy = p.pr(x, z, 0.0).1;
        if !sub {
            p.stand(sx, sy1, fy, sa);
        }
        put(&p, idx, sx - sw / 2.0, sy0, sw, sy1 - sy0);
    }

    // you: a ring on the floor; headphones appear above it, in dots
    let (ux, uy) = p.pr(0.0, 0.0, 0.0);
    {
        let k = f / CZ;
        let hpv = sm_hp;
        let sc_ = 1.0 - 0.4 * hpv;
        lay.you = (ux, uy);
        ellipse(cr, ux, uy, 0.42 * k * sc_, 0.1 * k * sc_);
        src(cr, RED, 1.0);
        cr.set_line_width(2.6);
        stroke(cr);
        ellipse(cr, ux, uy, 0.16 * k * sc_, 0.038 * k * sc_);
        src(cr, WHITE, 1.0);
        fill(cr);
        src(cr, WHITE, 0.85 * (1.0 - hpv));
        font(cr, NARROW, true, 12.0);
        text_spaced(cr, "YOU", ux, uy + 0.1 * k + 18.0, 2.0);
        if hpv > 0.01 {
            let hy0 = p.pr(0.0, 0.0, 0.62).1;
            let rr = 0.17 * k * (0.7 + 0.3 * hpv);
            let r1 = (0.016 * k).max(1.6);
            let r2 = (0.02 * k).max(1.8);
            for i in 0..=12 {
                let an = PI + PI * i as f64 / 12.0;
                glow_dot(cr, ux + an.cos() * rr, hy0 + an.sin() * rr, r1, RED, hpv, 10.0);
            }
            for sd in [-1.0, 1.0] {
                for i in 0..2 {
                    for j in 0..3 {
                        glow_dot(
                            cr,
                            ux + sd * (rr + (i as f64 - 0.5) * 0.045 * k),
                            hy0 + (j as f64 - 0.6) * 0.05 * k,
                            r2,
                            RED,
                            hpv,
                            10.0,
                        );
                    }
                }
            }
        }
    }

    // the rears stand at the edges
    for (idx, deg) in [(I_RL, -110.0_f64), (I_RR, 110.0)] {
        let a = deg * PI / 180.0;
        let (x, z) = (a.sin() * 2.0, a.cos() * 2.0);
        let sw = 0.44 * f / (z + CZ);
        let (sx, sy0) = p.pr(x, z, 1.55);
        let sy1 = p.pr(x, z, 0.82).1;
        let fy = p.pr(x, z, 0.0).1;
        p.stand(sx, sy1, fy, sa);
        put(&p, idx, sx - sw / 2.0, sy0, sw, sy1 - sy0);
    }

    // speaker-test ripples
    for (id, t0) in &ripples {
        let k = (sc.t - t0) / 1.2;
        if !(0.0..=1.0).contains(&k) {
            continue;
        }
        let hb = hits[*id];
        ellipse(cr, hb.0 + hb.2 / 2.0, hb.1 + hb.3 + 4.0, hb.2 * (0.6 + k * 2.2), hb.2 * (0.18 + k * 0.6));
        src(cr, WHITE, 1.0 - k);
        cr.set_line_width(2.0);
        stroke(cr);
    }
    restore(cr);

    // report
    lay.speakers = ORDER.iter().enumerate().map(|(i, n)| (*n, hits[i])).collect();
    let dom = (0..6).max_by(|a, b| wts[*a].partial_cmp(&wts[*b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
    if let Some(r) = p.rects[dom] {
        lay.screen_rect = Some((r[0], r[1], r[2], r[3]));
        lay.screen_alpha = wts[dom];
    }
    if sc.playing || (wts[1] > 0.01 || wts[2] > 0.01 || wts[3] > 0.01) && Painter::has_neon(sc) {
        animating = true;
    }
    lay.animating = animating;
    let _ = (p.t, p.w, p.h);
    lay
}

// ---------------------------------------------------------------------------------------------
// Tests: unit checks, and an ignored test that renders the scenes to /tmp/oys-*.png

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: i32, h: i32, c: (f64, f64, f64)) -> ImageSurface {
        let s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        {
            let cr = Context::new(&s).unwrap();
            let g = LinearGradient::new(0.0, 0.0, w as f64, h as f64);
            g.add_color_stop_rgb(0.0, c.0, c.1, c.2);
            g.add_color_stop_rgb(1.0, c.0 * 0.5, c.1 * 0.5, c.2 * 0.5);
            cr.set_source(&g).unwrap();
            cr.paint().unwrap();
        }
        s
    }

    /// White text on transparent, standing in for an artist or game logo.
    fn logo(text: &str, w: i32, h: i32) -> ImageSurface {
        let s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        {
            let cr = Context::new(&s).unwrap();
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            font(&cr, ANTON, false, h as f64 * 0.8);
            fit_text(&cr, text, w as f64 / 2.0, h as f64 / 2.0, w as f64 * 0.96, h as f64 * 0.8);
        }
        s
    }

    fn page(w: i32, h: i32) -> (ImageSurface, Context) {
        let s = ImageSurface::create(Format::ARgb32, w, h).unwrap();
        let cr = Context::new(&s).unwrap();
        let g = RadialGradient::new(w as f64 * 0.72, h as f64 * 0.5, 40.0, w as f64 * 0.72, h as f64 * 0.5, w as f64 * 0.8);
        g.add_color_stop_rgb(0.0, 0.40, 0.07, 0.10);
        g.add_color_stop_rgb(1.0, 0.07, 0.03, 0.04);
        cr.set_source(&g).unwrap();
        cr.paint().unwrap();
        (s, cr)
    }

    /// A minimal PNG writer (stored deflate blocks), since cairo's png feature is not enabled here.
    fn save_png(mut s: ImageSurface, path: &str) {
        s.flush();
        let (w, h, stride) = (s.width() as usize, s.height() as usize, s.stride() as usize);
        let d = s.data().unwrap();
        let mut raw = Vec::with_capacity((w * 3 + 1) * h);
        for y in 0..h {
            raw.push(0u8);
            for x in 0..w {
                let p = &d[y * stride + x * 4..];
                raw.extend_from_slice(&[p[2], p[1], p[0]]);
            }
        }
        let mut z = vec![0x78u8, 0x01];
        let mut chunks = raw.chunks(65535).peekable();
        while let Some(c) = chunks.next() {
            z.push(if chunks.peek().is_none() { 1 } else { 0 });
            z.extend_from_slice(&(c.len() as u16).to_le_bytes());
            z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
            z.extend_from_slice(c);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for v in &raw {
            a = (a + *v as u32) % 65521;
            b = (b + a) % 65521;
        }
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let crc = |bytes: &[u8]| -> u32 {
            let mut c = 0xffff_ffffu32;
            for v in bytes {
                c ^= *v as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        };
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let mut chunk = |kind: &[u8; 4], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc(&body).to_be_bytes());
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        chunk(b"IDAT", &z);
        chunk(b"IEND", &[]);
        std::fs::write(path, out).unwrap();
    }

    fn render(name: &str, sc: &Scene, sm: &mut Smooth) -> Layout {
        let (w, h) = (1600, 900);
        let (s, cr) = page(w, h);
        // settle the easing like a few seconds of frames
        let mut lay = Layout::default();
        for _ in 0..2 {
            lay = draw(&cr, w as f64, h as f64, 0.2, sc, sm);
        }
        drop(cr);
        save_png(s, &format!("/tmp/oys-{name}.png"));
        lay
    }

    #[test]
    fn zone_and_tone() {
        let sc = Scene::default();
        assert!((zone_r(&sc, 0.5, 2.4, 5.6) - (2.4 + 3.2 * 0.5 * 1.0)).abs() < 1e-9);
        assert_eq!(tone_color(&sc), (255.0, 0.231 * 255.0, 0.267 * 255.0));
        let off = Scene { sbx_on: false, ..Scene::default() };
        assert_eq!(tone_color(&off), FLOOR_RED);
    }

    #[test]
    fn draws_without_panicking() {
        let s = ImageSurface::create(Format::ARgb32, 640, 360).unwrap();
        let cr = Context::new(&s).unwrap();
        let mut sm = Smooth::default();
        for place in [Place::Plain, Place::Studio, Place::Concert, Place::Stadium, Place::Cinema, Place::Gaming] {
            let sc = Scene { place, neon_text: "Foo Fighters", playing: true, left_clip: 120.0, ..Scene::default() };
            let lay = draw(&cr, 640.0, 360.0, 0.03, &sc, &mut sm);
            assert_eq!(lay.speakers.len(), 6);
        }
    }

    /// How long a frame takes at 1600x900 (run with --release --ignored --nocapture).
    #[test]
    #[ignore]
    fn frame_time() {
        let recs: Vec<ImageSurface> = (0..5).map(|i| solid(96, 96, (0.2 * i as f64, 0.5, 0.8))).collect();
        let lg = logo("NIRVANA", 700, 170);
        let pic = solid(1280, 720, (0.3, 0.5, 0.9));
        let (s, cr) = page(1600, 900);
        let levels = [0.62, 0.55, 0.7, 0.8, 0.5, 0.58];
        for (name, place, screen) in [
            ("studio", Place::Studio, Screen::None),
            ("concert", Place::Concert, Screen::None),
            ("stadium", Place::Stadium, Screen::None),
            ("cinema", Place::Cinema, Screen::Picture(&pic)),
            ("gaming", Place::Gaming, Screen::Picture(&pic)),
            ("plain", Place::Plain, Screen::None),
        ] {
            let mut sm = Smooth::default();
            let n = 120;
            let t0 = std::time::Instant::now();
            for i in 0..n {
                let sc = Scene {
                    place,
                    active: [true; 6],
                    levels,
                    playing: true,
                    around: true,
                    size: 2.0,
                    spread: 1.0,
                    neon_logo: Some(&lg),
                    records: &recs,
                    screen,
                    t: i as f64 / 30.0,
                    ..Scene::default()
                };
                draw(&cr, 1600.0, 900.0, 1.0 / 30.0, &sc, &mut sm);
            }
            eprintln!("{name}: {:.2} ms/frame", t0.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
        drop(cr);
        drop(s);
    }

    #[test]
    #[ignore]
    fn render_pngs() {
        let recs: Vec<ImageSurface> = [(0.8, 0.5, 0.2), (0.2, 0.5, 0.8), (0.9, 0.9, 0.9), (0.6, 0.2, 0.5), (0.3, 0.7, 0.4)]
            .iter()
            .map(|c| solid(300, 300, *c))
            .collect();
        let lg = logo("NIRVANA", 700, 170);
        let live = [0.62, 0.55, 0.7, 0.8, 0.5, 0.58];
        let all = [true; 6];
        let stereo41 = [true, false, true, true, true, true];

        // studio, neon text, three fake records
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Studio,
            active: stereo41,
            levels: live,
            playing: true,
            around: true,
            size: 0.0,
            spread: 0.45,
            neon_text: "FOO FIGHTERS",
            records: &recs[..3],
            t: 1.0,
            ..Scene::default()
        };
        let lay = render("studio", &sc, &mut sm);
        eprintln!("studio: {:?}", lay);

        // studio song with concert sound: a bit bigger
        let mut sm = Smooth::default();
        let sc2 = Scene { studio_grow: 1.0, size: 2.0, spread: 0.7, records: &recs[..5], ..sc_clone(&sc) };
        render("studio-grown", &sc2, &mut sm);

        // concert, stage neon only
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Concert,
            active: stereo41,
            levels: live,
            playing: true,
            around: true,
            size: 1.0,
            spread: 0.7,
            neon_logo: Some(&lg),
            t: 2.0,
            ..Scene::default()
        };
        render("concert", &sc, &mut sm);

        // stadium, neon in the panel
        let mut sm = Smooth::default();
        let sc = Scene { place: Place::Stadium, size: 2.0, spread: 1.0, ..sc_clone(&sc) };
        render("stadium", &sc, &mut sm);

        // stadium with a score
        let badge = solid(120, 120, (0.9, 0.2, 0.2));
        let badge2 = solid(120, 120, (0.2, 0.3, 0.9));
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Stadium,
            active: stereo41,
            levels: live,
            playing: true,
            size: 2.0,
            spread: 1.0,
            around: true,
            screen: Screen::Score { home: "ARS", away: "CHE", home_badge: Some(&badge), away_badge: Some(&badge2) },
            ..Scene::default()
        };
        let lay = render("stadium-score", &sc, &mut sm);
        eprintln!("stadium-score: {:?}", lay.screen_rect);

        // cinema with a title
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Cinema,
            active: all,
            levels: live,
            playing: true,
            around: true,
            size: 1.0,
            spread: 0.8,
            screen: Screen::Title("DUNE"),
            ..Scene::default()
        };
        let lay = render("cinema", &sc, &mut sm);
        eprintln!("cinema: {:?}", lay.screen_rect);

        // gaming with a picture
        let pic = solid(640, 360, (0.3, 0.5, 0.9));
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Gaming,
            active: all,
            levels: live,
            playing: true,
            around: true,
            size: 1.0,
            spread: 0.75,
            screen: Screen::Picture(&pic),
            ..Scene::default()
        };
        let lay = render("gaming", &sc, &mut sm);
        eprintln!("gaming: {:?}", lay.screen_rect);

        // gaming with a logo
        let gl = logo("MASS EFFECT", 700, 200);
        let mut sm = Smooth::default();
        let sc = Scene { screen: Screen::Logo(&gl), ..sc_clone(&sc) };
        render("gaming-logo", &sc, &mut sm);

        // headphones on (studio)
        let mut sm = Smooth::default();
        let sc = Scene {
            place: Place::Studio,
            headphones: true,
            playing: true,
            size: 0.0,
            spread: 0.45,
            neon_text: "FOO FIGHTERS",
            records: &recs[..3],
            t: 1.0,
            ..Scene::default()
        };
        render("headphones", &sc, &mut sm);

        // SBX off: plain
        let mut sm = Smooth::default();
        let sc = Scene { place: Place::Studio, sbx_on: false, active: [true, false, true, true, false, false], levels: live, playing: true, ..Scene::default() };
        render("plain", &sc, &mut sm);

        // a speaker test and a split page
        let mut sm = Smooth::default();
        sm.ping(I_RL, 0.9, 1.2);
        let sc = Scene {
            place: Place::Concert,
            active: stereo41,
            levels: live,
            playing: true,
            around: true,
            neon_logo: Some(&lg),
            left_clip: 640.0,
            t: 1.3,
            size: 1.0,
            spread: 0.7,
            ..Scene::default()
        };
        render("split-ping", &sc, &mut sm);

        // cross-fade half way studio -> cinema
        let mut sm = Smooth::default();
        let a = Scene { place: Place::Studio, active: stereo41, levels: live, playing: true, neon_text: "FOO FIGHTERS", records: &recs[..3], ..Scene::default() };
        let (s, cr) = page(1600, 900);
        draw(&cr, 1600.0, 900.0, 0.03, &a, &mut sm);
        let b = Scene { place: Place::Cinema, screen: Screen::Title("DUNE"), ..sc_clone(&a) };
        let mut lay = Layout::default();
        for _ in 0..5 {
            lay = draw(&cr, 1600.0, 900.0, 0.1, &b, &mut sm);
        }
        eprintln!("crossfade: animating {} rect {:?} alpha {}", lay.animating, lay.screen_rect, lay.screen_alpha);
        drop(cr);
        save_png(s, "/tmp/oys-crossfade.png");
    }

    /// A copy of the scene (Scene is not Clone, since it borrows).
    fn sc_clone<'a>(s: &Scene<'a>) -> Scene<'a> {
        Scene {
            place: s.place,
            studio_grow: s.studio_grow,
            active: s.active,
            levels: s.levels,
            headphones: s.headphones,
            sbx_on: s.sbx_on,
            tone_rgb: s.tone_rgb,
            size: s.size,
            spread: s.spread,
            around: s.around,
            playing: s.playing,
            t: s.t,
            neon_text: s.neon_text,
            neon_logo: s.neon_logo,
            records: s.records,
            screen: s.screen,
            left_clip: s.left_clip,
            corner: s.corner,
        }
    }
}
