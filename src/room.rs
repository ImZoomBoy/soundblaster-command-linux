//! The room: your speakers at the standard 5.1 angles seen from above and
//! behind, drawn as real cabinets (woofer, tweeter, shadow), each with a live
//! LED meter, sound ripples when it plays, and you in the middle.

use gtk::cairo::{Context, LinearGradient, RadialGradient};
use std::f64::consts::PI;

const PINK: (f64, f64, f64) = (1.0, 0.255, 0.424);

/// (channel, name, angle in degrees: 0 = straight ahead)
pub const SPEAKERS: [(&str, &str, f64); 6] = [
    ("FL", "Front left", -30.0), ("FC", "Centre", 0.0), ("FR", "Front right", 30.0),
    ("LFE", "Subwoofer", 54.0), ("RL", "Rear left", -112.0), ("RR", "Rear right", 112.0),
];

pub struct RoomState {
    /// Which speakers carry sound in this setup (FL, FC, FR, LFE, RL, RR order).
    pub plays: [bool; 6],
    pub levels: [f32; 6],
    /// A speaker being tested.
    pub lit: Option<String>,
    pub headphones: bool,
    /// Seconds, for the ripples.
    pub time: f64,
    /// Space kept free at the top (for the title).
    pub top: f64,
    /// Space kept free at the bottom (for the capsule).
    pub bottom: f64,
    /// Note under a silent speaker, e.g. "off in 2.1".
    pub off_note: &'static str,
    /// The speakers' size range and the least height the room plans for: the
    /// room card used 0.7..1.35 and 200 px; the compact Remote's room is smaller.
    pub kmin: f64,
    pub kmax: f64,
    pub min_avail: f64,
}

pub struct Geometry {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    k: f64,
}

pub fn geometry(w: f64, h: f64, st: &RoomState) -> Geometry {
    let avail = (h - st.top - st.bottom).max(st.min_avail);
    let k = (avail / 430.0).min(w / 640.0).clamp(st.kmin, st.kmax);
    Geometry { cx: w / 2.0, cy: st.top + avail * 0.52, rx: (w * 0.39).min(avail * 0.95), ry: avail * 0.36, k }
}

fn pos(g: &Geometry, deg: f64) -> (f64, f64) {
    let t = deg.to_radians();
    (g.cx + g.rx * t.sin(), g.cy - g.ry * t.cos())
}

/// The speaker under a point, for click-to-test.
pub fn hit(w: f64, h: f64, st: &RoomState, x: f64, y: f64) -> Option<&'static str> {
    let g = geometry(w, h, st);
    SPEAKERS.iter()
        .map(|(c, _, d)| {
            let (sx, sy) = pos(&g, *d);
            (*c, ((sx - x).powi(2) + (sy - y).powi(2)).sqrt())
        })
        .filter(|(_, d)| *d < 46.0 * g.k)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c)
}

fn rrect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

fn ellipse(cr: &Context, cx: f64, cy: f64, rx: f64, ry: f64) {
    cr.save().ok();
    cr.translate(cx, cy);
    cr.scale(rx.max(0.01), ry.max(0.01));
    cr.new_path();
    cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
    cr.restore().ok();
}

fn text_centered(cr: &Context, text: &str, x: f64, y: f64) {
    if let Ok(e) = cr.text_extents(text) {
        cr.move_to(x - e.width() / 2.0 - e.x_bearing(), y);
        let _ = cr.show_text(text);
    }
}

fn woofer(cr: &Context, x: f64, y: f64, r: f64, s: f64, glow: Option<f64>) {
    cr.new_path();
    cr.arc(x, y, r + 2.5 * s, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.043, 0.043, 0.05);
    let _ = cr.fill();
    let g = RadialGradient::new(x - r * 0.2, y - r * 0.3, 0.0, x, y, r);
    g.add_color_stop_rgb(0.0, 0.23, 0.23, 0.25);
    g.add_color_stop_rgb(0.6, 0.086, 0.086, 0.10);
    g.add_color_stop_rgb(1.0, 0.02, 0.02, 0.024);
    cr.arc(x, y, r, 0.0, 2.0 * PI);
    let _ = cr.set_source(&g);
    let _ = cr.fill();
    let g = RadialGradient::new(x - r * 0.1, y - r * 0.12, 0.0, x, y, r * 0.32);
    g.add_color_stop_rgb(0.0, 0.42, 0.42, 0.45);
    g.add_color_stop_rgb(1.0, 0.1, 0.1, 0.12);
    cr.arc(x, y, r * 0.32, 0.0, 2.0 * PI);
    let _ = cr.set_source(&g);
    let _ = cr.fill();
    if let Some(l) = glow {
        cr.new_path();
        cr.arc(x, y, r + 2.5 * s, 0.0, 2.0 * PI);
        cr.set_source_rgba(PINK.0, PINK.1, PINK.2, 0.15 + 0.85 * l);
        cr.set_line_width(1.3);
        let _ = cr.stroke();
    }
}

fn tweeter(cr: &Context, x: f64, y: f64, s: f64) {
    cr.new_path();
    cr.arc(x, y, 5.5 * s, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.043, 0.043, 0.05);
    let _ = cr.fill();
    let g = RadialGradient::new(x - s, y - s * 1.2, 0.0, x, y, 3.6 * s);
    g.add_color_stop_rgb(0.0, 0.72, 0.72, 0.75);
    g.add_color_stop_rgb(0.5, 0.33, 0.33, 0.36);
    g.add_color_stop_rgb(1.0, 0.1, 0.1, 0.12);
    cr.arc(x, y, 3.6 * s, 0.0, 2.0 * PI);
    let _ = cr.set_source(&g);
    let _ = cr.fill();
}

pub fn draw(cr: &Context, w: f64, h: f64, st: &RoomState) {
    let g = geometry(w, h, st);
    // Floor glow and rings.
    let fl = RadialGradient::new(g.cx, g.cy, 0.0, g.cx, g.cy, g.rx * 1.35);
    fl.add_color_stop_rgba(0.0, PINK.0, PINK.1, PINK.2, 0.20);
    fl.add_color_stop_rgba(0.5, 0.78, 0.06, 0.18, 0.07);
    fl.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    cr.save().ok();
    cr.translate(g.cx, g.cy);
    cr.scale(1.0, g.ry / g.rx);
    cr.translate(-g.cx, -g.cy);
    cr.arc(g.cx, g.cy, g.rx * 1.35, 0.0, 2.0 * PI);
    let _ = cr.set_source(&fl);
    let _ = cr.fill();
    cr.restore().ok();
    for q in [0.33, 0.66, 1.0, 1.3] {
        ellipse(cr, g.cx, g.cy, g.rx * q, g.ry * q);
        cr.set_source_rgba(1.0, 1.0, 1.0, if q == 1.0 { 0.11 } else { 0.05 });
        cr.set_line_width(1.0);
        if q == 1.0 { cr.set_dash(&[2.0, 5.0], 0.0) }
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
    }
    // Lines to you, and ripples from the speakers that play.
    for (i, (c, _, d)) in SPEAKERS.iter().enumerate() {
        if !st.plays[i] {
            continue;
        }
        let (x, y) = pos(&g, *d);
        if *c != "LFE" {
            cr.set_source_rgba(PINK.0, PINK.1, PINK.2, 0.2);
            cr.set_dash(&[2.0, 6.0], 0.0);
            cr.move_to(x, y);
            cr.line_to(g.cx, g.cy);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }
        let lv = (st.levels[i] as f64 * 1.3).min(1.0);
        if lv > 0.02 {
            for k in 0..3 {
                let ph = (st.time / 2.2 + k as f64 / 3.0).fract();
                let sc = 0.55 + 1.75 * ph;
                ellipse(cr, x, y, 36.0 * g.k * sc, 22.0 * g.k * sc);
                cr.set_source_rgba(PINK.0, PINK.1, PINK.2, 0.7 * (1.0 - ph) * lv);
                cr.set_line_width(1.4);
                let _ = cr.stroke();
            }
        }
    }
    // You.
    let k = g.k;
    ellipse(cr, g.cx, g.cy + 18.0 * k, 44.0 * k, 13.0 * k);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    let _ = cr.fill();
    cr.new_path();
    cr.arc(g.cx, g.cy, 30.0 * k, 0.0, 2.0 * PI);
    cr.set_source_rgba(PINK.0, PINK.1, PINK.2, 0.12);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(PINK.0, PINK.1, PINK.2, 0.35);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    cr.move_to(g.cx, g.cy - 44.0 * k);
    cr.line_to(g.cx + 7.0 * k, g.cy - 34.0 * k);
    cr.line_to(g.cx - 7.0 * k, g.cy - 34.0 * k);
    cr.close_path();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
    let _ = cr.fill();
    let you = RadialGradient::new(g.cx, g.cy - 8.0 * k, 0.0, g.cx, g.cy, 22.0 * k);
    you.add_color_stop_rgb(0.0, 1.0, 0.42, 0.55);
    you.add_color_stop_rgb(1.0, 0.69, 0.05, 0.16);
    cr.new_path();
    cr.arc(g.cx, g.cy - 4.0 * k, 10.0 * k, 0.0, 2.0 * PI);
    let _ = cr.set_source(&you);
    let _ = cr.fill();
    cr.new_path();
    cr.arc(g.cx, g.cy + 18.0 * k, 17.0 * k, PI, 2.0 * PI);
    cr.close_path();
    let _ = cr.set_source(&you);
    let _ = cr.fill();
    if st.headphones {
        cr.new_path();
        cr.arc(g.cx, g.cy - 2.0 * k, 14.0 * k, PI, 2.0 * PI);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_line_width(3.0);
        let _ = cr.stroke();
        for side in [-1.0, 1.0] {
            rrect(cr, g.cx + side * 14.0 * k - 3.0 * k, g.cy - 6.0 * k, 6.0 * k, 10.0 * k, 2.0);
            let _ = cr.fill();
        }
    }
    // Cabinets, back to front.
    let mut order: Vec<usize> = (0..6).collect();
    order.sort_by(|a, b| pos(&g, SPEAKERS[*a].2).1.total_cmp(&pos(&g, SPEAKERS[*b].2).1));
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    for i in order {
        let (c, name, d) = SPEAKERS[i];
        let (x, y) = pos(&g, d);
        let s = k * (0.82 + 0.36 * ((y - (g.cy - g.ry)) / (2.0 * g.ry)));
        let lit = st.lit.as_deref() == Some(c);
        // Small rooms (the compact Remote) leave the names off: a tap still names the speaker.
        let name = if k < 0.55 { "" } else { name };
        cabinet(cr, c, x, y, s, st.plays[i], st.levels[i] as f64, lit, name, if st.plays[i] { "" } else { st.off_note }, k);
    }
}

/// One cabinet, front on, centred at (x, y) and scaled by `s`: shadow, cabinet,
/// woofer and tweeter, a glow when tested, the LED meter beside it when it plays,
/// and its name under it (`name` empty: none). `k` sizes the name's type.
#[allow(clippy::too_many_arguments)]
pub fn cabinet(cr: &Context, c: &str, x: f64, y: f64, s: f64, on: bool, lv: f64, lit: bool, name: &str, note: &str, k: f64) {
    let (bw, bh) = size(c, s);
    cr.push_group();
    // Shadow
    for j in 0..4 {
        ellipse(cr, x, y + bh / 2.0 + 5.0 * s, bw * (0.72 + 0.05 * j as f64), 7.0 * s + j as f64);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.14);
        let _ = cr.fill();
    }
    if lit {
        let hot = RadialGradient::new(x, y, 0.0, x, y, bw * 0.95);
        hot.add_color_stop_rgba(0.0, PINK.0, PINK.1, PINK.2, 0.55);
        hot.add_color_stop_rgba(1.0, PINK.0, PINK.1, PINK.2, 0.0);
        cr.arc(x, y, bw * 0.95, 0.0, 2.0 * PI);
        let _ = cr.set_source(&hot);
        let _ = cr.fill();
    }
    let cab = LinearGradient::new(0.0, y - bh / 2.0, 0.0, y + bh / 2.0);
    cab.add_color_stop_rgb(0.0, 0.235, 0.235, 0.26);
    cab.add_color_stop_rgb(0.5, 0.145, 0.145, 0.165);
    cab.add_color_stop_rgb(1.0, 0.094, 0.094, 0.11);
    rrect(cr, x - bw / 2.0, y - bh / 2.0, bw, bh, 7.0 * s);
    let _ = cr.set_source(&cab);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    rrect(cr, x - bw / 2.0 + 1.0, y - bh / 2.0 + 1.0, bw - 2.0, 3.0 * s, 1.5 * s);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
    let _ = cr.fill();
    let glow = on.then_some(lv);
    match c {
        "LFE" => woofer(cr, x, y + 3.0 * s, 24.0 * s, s, glow),
        "FC" => {
            woofer(cr, x - 30.0 * s, y, 13.0 * s, s, glow);
            woofer(cr, x + 30.0 * s, y, 13.0 * s, s, glow);
            tweeter(cr, x, y, s);
        }
        _ => {
            tweeter(cr, x, y - bh * 0.29, s);
            woofer(cr, x, y + bh * 0.12, 15.0 * s, s, glow);
        }
    }
    // LED meter beside the cabinet.
    if on {
        for j in 0..9 {
            let lit = lv * 9.0 > j as f64 + 0.3;
            let col = if !lit { (1.0, 1.0, 1.0, 0.07) } else if j >= 7 { (1.0, 0.70, 0.28, 1.0) }
                      else if j >= 5 { (PINK.0, PINK.1, PINK.2, 1.0) } else { (0.78, 0.06, 0.18, 1.0) };
            rrect(cr, x + bw / 2.0 + 6.0 * s, y + bh / 2.0 - (j + 1) as f64 * (bh / 9.2), 4.0 * s, bh / 9.2 - 1.6, 1.0);
            cr.set_source_rgba(col.0, col.1, col.2, col.3);
            let _ = cr.fill();
        }
    }
    if !name.is_empty() {
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(11.0 * k.min(1.15));
        cr.set_source_rgba(1.0, 1.0, 1.0, if on { 0.9 } else { 0.4 });
        text_centered(cr, name, x, y + bh / 2.0 + 21.0 * s);
        if !on && !note.is_empty() {
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
            cr.set_font_size(9.5);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            text_centered(cr, note, x, y + bh / 2.0 + 34.0 * s);
        }
    }
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(if on { 1.0 } else { 0.35 });
}

fn size(c: &str, s: f64) -> (f64, f64) {
    match c { "LFE" => (78.0 * s, 74.0 * s), "FC" => (104.0 * s, 42.0 * s), _ => (48.0 * s, 74.0 * s) }
}

/// The six cabinets front on in one row (the Strip), filling `w` x `h`. Draws
/// them when `cr` is given; returns each one's box, for tapping.
pub fn row(cr: Option<&Context>, w: f64, h: f64, st: &RoomState) -> Vec<(&'static str, [f64; 4])> {
    let s = ((h - 6.0) / 80.0).min((w - 16.0) / 478.0).max(0.2);
    let widths: f64 = SPEAKERS.iter().map(|(c, _, _)| size(c, s).0 + 14.0 * s).sum();
    let gap = ((w - 16.0 * s - widths) / 5.0).max(0.0);
    let mut x = 8.0 * s;
    let y = h * 0.46;
    let mut out = vec![];
    for (i, (c, _, _)) in SPEAKERS.iter().enumerate() {
        let (bw, bh) = size(c, s);
        if let Some(cr) = cr {
            cabinet(cr, c, x + bw / 2.0, y, s, st.plays[i], st.levels[i] as f64, st.lit.as_deref() == Some(*c), "", "", s);
        }
        out.push((*c, [x - 4.0, y - bh / 2.0 - 4.0, bw + 18.0 * s, bh + 8.0]));
        x += bw + 14.0 * s + gap;
    }
    out
}
