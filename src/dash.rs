//! The custom-drawn controls: the equaliser curve you drag, studio knobs you
//! turn, power LEDs, LED meters and the signal-path dots. Each one drives an
//! ordinary gtk::Adjustment / gtk::Scale / gtk::ToggleButton, so the rest of
//! the app reads and writes those.

use gtk::cairo::Context;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

const RED: (f64, f64, f64) = (0.784, 0.063, 0.18);
const PINK: (f64, f64, f64) = (1.0, 0.255, 0.424);

fn fg(w: &impl IsA<gtk::Widget>) -> (f64, f64, f64) {
    let c = w.as_ref().color();
    (c.red() as f64, c.green() as f64, c.blue() as f64)
}

fn text_centered(cr: &Context, text: &str, x: f64, y: f64) {
    if let Ok(e) = cr.text_extents(text) {
        cr.move_to(x - e.width() / 2.0 - e.x_bearing(), y);
        let _ = cr.show_text(text);
    }
}

// ------------------------------------------------------------------ equaliser curve

/// The equaliser as a curve you can drag. `bands` hold the values (−12..12
/// dB); `boost` is what SBX Bass adds on top (drawn dashed); `live` dims the
/// curve when the equaliser is off.
pub struct EqCurve {
    pub area: gtk::DrawingArea,
    pub boost: Rc<RefCell<Vec<f64>>>,
    pub live: Rc<Cell<bool>>,
}

const EQ_LABELS: [&str; 10] = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
const PAD_L: f64 = 44.0;
const PAD_R: f64 = 20.0;
const PAD_T: f64 = 26.0;
const PAD_B: f64 = 30.0;

fn band_x(i: usize, w: f64) -> f64 {
    PAD_L + (w - PAD_L - PAD_R) * i as f64 / 9.0
}
fn db_y(db: f64, h: f64) -> f64 {
    PAD_T + (h - PAD_T - PAD_B) * (12.0 - db.clamp(-12.0, 12.0)) / 24.0
}

/// A smooth line through the points (Catmull-Rom as Bézier curves).
fn smooth(cr: &Context, pts: &[(f64, f64)]) {
    cr.move_to(pts[0].0, pts[0].1);
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let (p1, p2) = (pts[i], pts[i + 1]);
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        cr.curve_to(
            p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0,
            p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0,
            p2.0, p2.1,
        );
    }
}

pub fn eq_curve(bands: &[gtk::Scale]) -> EqCurve {
    let area = gtk::DrawingArea::builder()
        .content_height(300)
        .hexpand(true)
        .focusable(true)
        .can_focus(true)
        .tooltip_text("Drag a point up or down. Keys: ← → pick a band, ↑ ↓ change it")
        .css_classes(["eq-curve"])
        .build();
    let boost = Rc::new(RefCell::new(vec![0.0; 10]));
    let live = Rc::new(Cell::new(true));
    let active: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let hover: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let bands: Vec<gtk::Scale> = bands.to_vec();
    {
        let (bands, boost, live, active, hover) = (bands.clone(), boost.clone(), live.clone(), active.clone(), hover.clone());
        area.set_draw_func(move |da, cr, w, h| {
            let (w, h) = (w as f64, h as f64);
            let f = fg(da);
            let on = live.get();
            let alpha = if on { 1.0 } else { 0.4 };
            cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
            cr.set_font_size(10.5);
            // Grid (a short curve leaves out the ±6 lines so their labels don't touch)
            let short = db_y(6.0, h) - db_y(12.0, h) < 16.0;
            for db in [12.0_f64, 6.0, 0.0, -6.0, -12.0] {
                if short && db.abs() == 6.0 {
                    continue;
                }
                let y = db_y(db, h);
                cr.set_source_rgba(f.0, f.1, f.2, if db == 0.0 { 0.22 } else { 0.07 });
                cr.set_line_width(1.0);
                cr.move_to(PAD_L, y.round() + 0.5);
                cr.line_to(w - PAD_R, y.round() + 0.5);
                let _ = cr.stroke();
                cr.set_source_rgba(f.0, f.1, f.2, 0.45);
                let t = if db == 0.0 { "0 dB".to_string() } else { format!("{db:+}") };
                if let Ok(e) = cr.text_extents(&t) {
                    cr.move_to(PAD_L - 8.0 - e.width(), y + 4.0);
                    let _ = cr.show_text(&t);
                }
            }
            for (i, l) in EQ_LABELS.iter().enumerate() {
                let x = band_x(i, w);
                cr.set_source_rgba(f.0, f.1, f.2, 0.05);
                cr.move_to(x.round() + 0.5, PAD_T);
                cr.line_to(x.round() + 0.5, h - PAD_B);
                let _ = cr.stroke();
                cr.set_source_rgba(f.0, f.1, f.2, 0.5);
                text_centered(cr, l, x, h - PAD_B + 18.0);
            }
            let vals: Vec<f64> = bands.iter().map(|b| b.value()).collect();
            let pts: Vec<(f64, f64)> = vals.iter().enumerate().map(|(i, v)| (band_x(i, w), db_y(*v, h))).collect();
            let zero = db_y(0.0, h);
            // Fill between the curve and 0 dB.
            smooth(cr, &pts);
            cr.line_to(pts[9].0, zero);
            cr.line_to(pts[0].0, zero);
            cr.close_path();
            let g = gtk::cairo::LinearGradient::new(0.0, PAD_T, 0.0, h - PAD_B);
            g.add_color_stop_rgba(0.0, RED.0, RED.1, RED.2, 0.35 * alpha);
            g.add_color_stop_rgba(0.5, RED.0, RED.1, RED.2, 0.10 * alpha);
            g.add_color_stop_rgba(1.0, RED.0, RED.1, RED.2, 0.35 * alpha);
            let _ = cr.set_source(&g);
            let _ = cr.fill();
            cr.new_path();
            // What SBX Bass adds on top, dashed.
            let b = boost.borrow();
            if on && b.iter().any(|x| *x != 0.0) {
                let bp: Vec<(f64, f64)> = vals.iter().zip(b.iter()).enumerate()
                    .map(|(i, (v, x))| (band_x(i, w), db_y(v + x, h))).collect();
                cr.set_source_rgba(f.0, f.1, f.2, 0.75);
                cr.set_line_width(2.0);
                cr.set_dash(&[5.0, 4.0], 0.0);
                smooth(cr, &bp);
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
                cr.set_font_size(10.5);
                cr.set_source_rgba(f.0, f.1, f.2, 0.7);
                let t = "- - -  with SBX Bass";
                if let Ok(e) = cr.text_extents(t) {
                    cr.move_to(w - PAD_R - e.width(), PAD_T - 10.0);
                    let _ = cr.show_text(t);
                }
            }
            // The curve
            cr.new_path();
            cr.set_line_width(3.0);
            cr.set_source_rgba(PINK.0, PINK.1, PINK.2, alpha);
            smooth(cr, &pts);
            let _ = cr.stroke();
            // Points, with their value when hovered or dragged.
            let focus = active.get().or(hover.get());
            for (i, (x, y)) in pts.iter().enumerate() {
                let big = focus == Some(i);
                cr.new_path();
                cr.arc(*x, *y, if big { 8.5 } else { 6.0 }, 0.0, 2.0 * PI);
                cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
                let _ = cr.fill_preserve();
                cr.set_line_width(2.5);
                cr.set_source_rgba(RED.0, RED.1, RED.2, alpha);
                let _ = cr.stroke();
                if big || vals[i] != 0.0 {
                    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
                    cr.set_font_size(if big { 12.5 } else { 10.0 });
                    cr.set_source_rgba(f.0, f.1, f.2, if big { 1.0 } else { 0.6 * alpha });
                    let v = vals[i].round() as i32;
                    let t = if v == 0 { "0".to_string() } else { format!("{v:+}") };
                    let ty = if vals[i] >= 0.0 { y - 14.0 } else { y + 24.0 };
                    text_centered(cr, &t, *x, ty);
                    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
                }
            }
        });
    }
    let set_from = {
        let bands = bands.clone();
        move |i: usize, y: f64, h: f64| {
            let db = 12.0 - (y - PAD_T) / (h - PAD_T - PAD_B) * 24.0;
            bands[i].set_value(db.round().clamp(-12.0, 12.0));
        }
    };
    let nearest = |x: f64, w: f64| -> usize {
        let step = (w - PAD_L - PAD_R) / 9.0;
        (((x - PAD_L) / step).round().clamp(0.0, 9.0)) as usize
    };
    let drag = gtk::GestureDrag::new();
    let origin = Rc::new(Cell::new((0.0, 0.0)));
    {
        let (active, origin, set_from) = (active.clone(), origin.clone(), set_from.clone());
        drag.connect_drag_begin(move |g, x, y| {
            let Some(w) = g.widget() else { return };
            w.grab_focus();
            let i = nearest(x, w.width() as f64);
            active.set(Some(i));
            origin.set((x, y));
            set_from(i, y, w.height() as f64);
            w.queue_draw();
        });
    }
    {
        let (active, set_from) = (active.clone(), set_from.clone());
        drag.connect_drag_update(move |g, _, dy| {
            let (Some(w), Some(i)) = (g.widget(), active.get()) else { return };
            set_from(i, origin.get().1 + dy, w.height() as f64);
        });
    }
    {
        let active = active.clone();
        drag.connect_drag_end(move |g, _, _| {
            if let Some(w) = g.widget() {
                w.queue_draw();
            }
            let _ = &active;
        });
    }
    area.add_controller(drag);
    let motion = gtk::EventControllerMotion::new();
    {
        let hover = hover.clone();
        motion.connect_motion(move |m, x, _| {
            if let Some(w) = m.widget() {
                let i = Some(nearest(x, w.width() as f64));
                if hover.get() != i {
                    hover.set(i);
                    w.queue_draw();
                }
            }
        });
    }
    {
        let hover = hover.clone();
        motion.connect_leave(move |m| {
            hover.set(None);
            if let Some(w) = m.widget() {
                w.queue_draw();
            }
        });
    }
    area.add_controller(motion);
    let keys = gtk::EventControllerKey::new();
    {
        let (active, bands) = (active.clone(), bands.clone());
        keys.connect_key_pressed(move |k, key, _, _| {
            use gtk::gdk::Key;
            let i = active.get().unwrap_or(0);
            match key {
                Key::Left => active.set(Some(i.saturating_sub(1))),
                Key::Right => active.set(Some((i + 1).min(9))),
                Key::Up => bands[i].set_value((bands[i].value() + 1.0).min(12.0)),
                Key::Down => bands[i].set_value((bands[i].value() - 1.0).max(-12.0)),
                _ => return gtk::glib::Propagation::Proceed,
            }
            if active.get().is_none() {
                active.set(Some(i));
            }
            if let Some(w) = k.widget() {
                w.queue_draw();
            }
            gtk::glib::Propagation::Stop
        });
    }
    area.add_controller(keys);
    {
        let active = active.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(move |f| {
            active.set(None);
            if let Some(w) = f.widget() {
                w.queue_draw();
            }
        });
        area.add_controller(focus);
    }
    for b in &bands {
        let a = area.clone();
        b.connect_value_changed(move |_| a.queue_draw());
    }
    EqCurve { area, boost, live }
}

// ------------------------------------------------------------------ studio knobs (design N6/F)

/// A small round power button with an LED, for an effect's on/off.
pub fn power_button(tooltip: &str) -> gtk::ToggleButton {
    let led = gtk::Box::builder().css_classes(["led"]).halign(gtk::Align::Center).valign(gtk::Align::Center).build();
    gtk::ToggleButton::builder().child(&led).css_classes(["pwr"]).tooltip_text(tooltip)
        .valign(gtk::Align::Center).halign(gtk::Align::Center).build()
}

/// Draws a studio knob: an LED ring of dots for the level, a dark machined body
/// and a pointer.
pub fn draw_knob(cr: &Context, size: f64, frac: f64, on: bool) {
    let c = size / 2.0;
    let ro = c - 3.0;
    let rb = c * 0.6;
    let n = 25;
    let (a0, a1) = (0.75 * PI, 2.25 * PI);
    let lit = (frac.clamp(0.0, 1.0) * (n - 1) as f64).round() as i32;
    for i in 0..n {
        let a = a0 + (a1 - a0) * i as f64 / (n - 1) as f64;
        let l = i <= lit && frac > 0.001;
        cr.new_path();
        cr.arc(c + ro * a.cos(), c + ro * a.sin(), if size > 90.0 { 2.4 } else { 1.9 }, 0.0, 2.0 * PI);
        if l && on {
            let col = if i as f64 > n as f64 * 0.8 { (1.0, 0.70, 0.28) } else { PINK };
            cr.set_source_rgba(col.0, col.1, col.2, 1.0);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(col.0, col.1, col.2, 0.35);
            cr.set_line_width(2.5);
            let _ = cr.stroke();
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, if l { 0.4 } else { 0.09 });
            let _ = cr.fill();
        }
    }
    // Shadow, body, inner ring.
    cr.new_path();
    cr.arc(c, c + 2.0, rb + 1.0, 0.0, 2.0 * PI);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.fill();
    let g = gtk::cairo::RadialGradient::new(c - rb * 0.3, c - rb * 0.4, 0.0, c, c, rb * 1.3);
    g.add_color_stop_rgb(0.0, 0.29, 0.29, 0.32);
    g.add_color_stop_rgb(0.55, 0.145, 0.145, 0.165);
    g.add_color_stop_rgb(1.0, 0.07, 0.07, 0.08);
    cr.arc(c, c, rb, 0.0, 2.0 * PI);
    let _ = cr.set_source(&g);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.1);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    cr.new_path();
    cr.arc(c, c, rb * 0.72, 0.0, 2.0 * PI);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.05);
    let _ = cr.stroke();
    let a = a0 + (a1 - a0) * frac.clamp(0.0, 1.0);
    cr.new_path();
    cr.move_to(c + rb * 0.3 * a.cos(), c + rb * 0.3 * a.sin());
    cr.line_to(c + rb * 0.78 * a.cos(), c + rb * 0.78 * a.sin());
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_width(if size > 90.0 { 3.0 } else { 2.4 });
    cr.set_source_rgba(1.0, 1.0, 1.0, if on { 1.0 } else { 0.35 });
    let _ = cr.stroke();
}

/// A knob you turn: drag round it (clockwise raises), scroll, or use the
/// arrow keys. Turning it switches its effect on (`power`), like hardware.
pub fn knob(adj: &gtk::Adjustment, power: Option<&gtk::ToggleButton>, size: i32, tooltip: &str) -> gtk::DrawingArea {
    let da = gtk::DrawingArea::builder()
        .content_width(size).content_height(size)
        .halign(gtk::Align::Center)
        .focusable(true).can_focus(true)
        .tooltip_text(tooltip)
        .css_classes(["knob"])
        .build();
    da.set_cursor_from_name(Some("grab"));
    {
        let (adj, power) = (adj.clone(), power.cloned());
        da.set_draw_func(move |_, cr, w, _| {
            let span = adj.upper() - adj.lower();
            let frac = if span > 0.0 { (adj.value() - adj.lower()) / span } else { 0.0 };
            let on = power.as_ref().is_none_or(|p| p.is_active()) && power.as_ref().is_none_or(|p| p.is_sensitive());
            draw_knob(cr, w as f64, frac, on);
        });
    }
    let turn_on = {
        let power = power.cloned();
        move || {
            if let Some(p) = &power && !p.is_active() {
                p.set_active(true);
            }
        }
    };
    let set = {
        let adj = adj.clone();
        let turn_on = turn_on.clone();
        move |v: f64| {
            let step = adj.step_increment().max(1e-9);
            adj.set_value(((v / step).round() * step).clamp(adj.lower(), adj.upper()));
            turn_on();
        }
    };
    // Rotary drag: follow the pointer's angle round the knob's centre.
    let drag = gtk::GestureDrag::new();
    let state: Rc<Cell<(f64, f64, f64, f64)>> = Rc::new(Cell::new((0.0, 0.0, 0.0, 0.0))); // start x, y, last angle, value
    {
        let (adj, state) = (adj.clone(), state.clone());
        drag.connect_drag_begin(move |g, x, y| {
            let Some(w) = g.widget() else { return };
            w.grab_focus();
            w.set_cursor_from_name(Some("grabbing"));
            let (cx, cy) = (w.width() as f64 / 2.0, w.height() as f64 / 2.0);
            state.set((x, y, (y - cy).atan2(x - cx), adj.value()));
        });
    }
    {
        let (adj, state, set) = (adj.clone(), state.clone(), set.clone());
        drag.connect_drag_update(move |g, dx, dy| {
            let Some(w) = g.widget() else { return };
            let (x0, y0, last, v) = state.get();
            let (cx, cy) = (w.width() as f64 / 2.0, w.height() as f64 / 2.0);
            let a = (y0 + dy - cy).atan2(x0 + dx - cx);
            let mut d = a - last;
            if d > PI { d -= 2.0 * PI; }
            if d < -PI { d += 2.0 * PI; }
            let span = adj.upper() - adj.lower();
            let nv = (v + d / (1.5 * PI) * span).clamp(adj.lower(), adj.upper());
            state.set((x0, y0, a, nv));
            set(nv);
        });
    }
    drag.connect_drag_end(|g, _, _| {
        if let Some(w) = g.widget() {
            w.set_cursor_from_name(Some("grab"));
        }
    });
    da.add_controller(drag);
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    {
        let (adj, set) = (adj.clone(), set.clone());
        scroll.connect_scroll(move |_, _, dy| {
            let step = adj.step_increment().max((adj.upper() - adj.lower()) / 50.0);
            set(adj.value() - dy.signum() * step);
            gtk::glib::Propagation::Stop
        });
    }
    da.add_controller(scroll);
    let keys = gtk::EventControllerKey::new();
    {
        let (adj, set, power) = (adj.clone(), set.clone(), power.cloned());
        keys.connect_key_pressed(move |_, key, _, _| {
            use gtk::gdk::Key;
            let step = adj.step_increment().max((adj.upper() - adj.lower()) / 100.0);
            match key {
                Key::Up | Key::Right => set(adj.value() + step),
                Key::Down | Key::Left => set(adj.value() - step),
                Key::Page_Up => set(adj.value() + step * 10.0),
                Key::Page_Down => set(adj.value() - step * 10.0),
                Key::space | Key::Return => {
                    if let Some(p) = &power {
                        p.set_active(!p.is_active());
                    }
                }
                _ => return gtk::glib::Propagation::Proceed,
            }
            gtk::glib::Propagation::Stop
        });
    }
    da.add_controller(keys);
    {
        let d = da.clone();
        adj.connect_value_changed(move |_| d.queue_draw());
        if let Some(p) = power {
            let d = da.clone();
            p.connect_toggled(move |_| d.queue_draw());
        }
    }
    da
}

/// A horizontal LED meter (24 segments), under each of the mixer's sliders.
pub fn draw_hmeter(cr: &Context, w: f64, h: f64, level: f64) {
    let n = 24;
    let gap = 2.0;
    let sw = (w - gap * (n - 1) as f64) / n as f64;
    for i in 0..n {
        let x = i as f64 * (sw + gap);
        let on = level * n as f64 > i as f64 + 0.3;
        let col = if !on { (1.0, 1.0, 1.0, 0.07) } else if i >= 19 { (1.0, 0.70, 0.28, 1.0) }
                  else if i >= 13 { (PINK.0, PINK.1, PINK.2, 1.0) } else { (RED.0, RED.1, RED.2, 1.0) };
        cr.rectangle(x, 0.0, sw, h);
        cr.set_source_rgba(col.0, col.1, col.2, col.3);
        let _ = cr.fill();
    }
}

/// The four signal-path dots: white unchanged, amber changed, pink effects.
pub fn draw_path_dots(cr: &Context, h: f64, kinds: &[&str]) {
    let y = h / 2.0;
    for (i, k) in kinds.iter().enumerate() {
        let x = 5.0 + i as f64 * 18.0;
        if i + 1 < kinds.len() {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.25);
            cr.set_line_width(1.5);
            cr.move_to(x + 4.0, y);
            cr.line_to(x + 14.0, y);
            let _ = cr.stroke();
        }
        let (r, g, b) = match *k { "changed" => (1.0, 0.70, 0.28), "enhanced" => PINK, _ => (0.91, 0.91, 0.93) };
        cr.new_path();
        cr.arc(x, y, 6.0, 0.0, 2.0 * PI);
        cr.set_source_rgba(r, g, b, 0.25);
        let _ = cr.fill();
        cr.arc(x, y, 4.0, 0.0, 2.0 * PI);
        cr.set_source_rgb(r, g, b);
        let _ = cr.fill();
    }
}
