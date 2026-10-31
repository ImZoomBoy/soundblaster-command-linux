//! The full-screen speaker test (round 101's "Test your speakers"): the page's
//! photo printed in red behind a shade, your room drawn in perspective with the
//! speakers round your seat and their names on leader lines. Tap a speaker (or
//! its name) to hear it, or Walk round to hear each in turn, as noise or a
//! voice. Music pauses while it's open and plays again when it closes.

use super::*;
use gtk::cairo::Context;
use std::f64::consts::TAU;

/// The room's projection and drawing size (the prototype's `iso()` and its
/// 1020 x 640 view box).
const ISO_S: f64 = 62.0;
const ISO_OX: f64 = 470.0;
const ISO_OY: f64 = 390.0;
const VIEW: (f64, f64, f64, f64) = (-40.0, -20.0, 1020.0, 640.0);
/// Where you sit (z), and how high the walls are.
const SEAT_Z: f64 = 0.7;
const WALL: f64 = 2.5;
/// Walk round goes round the room from the front left.
const WALK: [&str; 6] = ["FL", "FC", "FR", "RR", "RL", "LFE"];

fn iso(x: f64, y: f64, z: f64) -> (f64, f64) {
    (ISO_OX + (x - z) * 0.866 * ISO_S, ISO_OY + (x + z) * 0.5 * ISO_S - y * ISO_S)
}

/// Where a speaker stands on the floor (x, z): round your seat at its angle;
/// the subwoofer on the right.
fn place(ch: &str) -> (f64, f64) {
    if ch == "LFE" {
        return (2.25, -2.2);
    }
    let a: f64 = match ch { "FL" => -30.0, "FR" => 30.0, "FC" => 0.0, "RL" => -110.0, _ => 110.0 };
    let a = a.to_radians();
    (a.sin() * 2.2, SEAT_Z - a.cos() * 2.2)
}

/// Where each name sits in the drawing (its baseline), and whether it ends there.
fn label_at(ch: &str) -> (f64, f64, bool) {
    match ch {
        "LFE" => (842.0, 330.0, false),
        "FL" => (300.0, 96.0, true),
        "FC" => (610.0, 40.0, false),
        "FR" => (822.0, 200.0, false),
        "RL" => (110.0, 350.0, true),
        _ => (700.0, 560.0, false),
    }
}

type Rgba = (f64, f64, f64, f64);

fn rgb(c: u32) -> Rgba {
    (((c >> 16) & 255) as f64 / 255.0, ((c >> 8) & 255) as f64 / 255.0, (c & 255) as f64 / 255.0, 1.0)
}

fn shape(cr: &Context, pts: &[(f64, f64)], fill: Rgba, stroke: Rgba, lw: f64) {
    cr.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        cr.line_to(p.0, p.1);
    }
    cr.close_path();
    cr.set_source_rgba(fill.0, fill.1, fill.2, fill.3);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(stroke.0, stroke.1, stroke.2, stroke.3);
    cr.set_line_width(lw);
    let _ = cr.stroke();
}

/// A box in the room: (x, z) its middle on the floor, w x d x h its size,
/// starting y0 above the floor; its top, right and front faces.
#[allow(clippy::too_many_arguments)]
fn iso_box(cr: &Context, x: f64, z: f64, w: f64, d: f64, h: f64, y0: f64, fill: [Rgba; 3], stroke: Rgba) {
    let (x0, x1, z0, z1, ya, yb) = (x - w / 2.0, x + w / 2.0, z - d / 2.0, z + d / 2.0, y0, y0 + h);
    shape(cr, &[iso(x0, ya, z1), iso(x1, ya, z1), iso(x1, yb, z1), iso(x0, yb, z1)], fill[1], stroke, 1.3);
    shape(cr, &[iso(x1, ya, z0), iso(x1, yb, z0), iso(x1, yb, z1), iso(x1, ya, z1)], fill[2], stroke, 1.3);
    shape(cr, &[iso(x0, yb, z0), iso(x1, yb, z0), iso(x1, yb, z1), iso(x0, yb, z1)], fill[0], stroke, 1.3);
}

/// Draws the room with these speakers, `lit` playing (pulsing rings, `t` in
/// seconds). Returns where each speaker and its name can be tapped, in pixels.
pub fn draw_room(cr: &Context, w: f64, h: f64, chans: &[&'static str], lit: Option<&str>, t: f64) -> Vec<(&'static str, [f64; 4])> {
    let (vx, vy, vw, vh) = VIEW;
    let k = (w / vw).min(h / vh);
    let (ox, oy) = ((w - vw * k) / 2.0 - vx * k, (h - vh * k) / 2.0 - vy * k);
    let to_px = |r: [f64; 4]| [ox + r[0] * k, oy + r[1] * k, r[2] * k, r[3] * k];
    let _ = cr.save();
    cr.translate(ox, oy);
    cr.scale(k, k);
    let white = (1.0, 1.0, 1.0, 1.0);
    // The floor, the left wall and the back wall.
    shape(cr, &[iso(-3.0, 0.0, -3.0), iso(3.0, 0.0, -3.0), iso(3.0, 0.0, 3.0), iso(-3.0, 0.0, 3.0)], (0.0, 0.0, 0.0, 0.75), white, 1.6);
    shape(cr, &[iso(-3.0, 0.0, -3.0), iso(-3.0, 0.0, 3.0), iso(-3.0, WALL, 3.0), iso(-3.0, WALL, -3.0)], (0.04, 0.04, 0.045, 0.85), white, 1.6);
    shape(cr, &[iso(-3.0, 0.0, -3.0), iso(3.0, 0.0, -3.0), iso(3.0, WALL, -3.0), iso(-3.0, WALL, -3.0)], (0.08, 0.016, 0.02, 0.85), white, 1.6);
    // Your seat and you.
    let seat = [rgb(0x2a2a2e), rgb(0x141416), rgb(0x1c1c1f)];
    let edge = (1.0, 1.0, 1.0, 0.69);
    iso_box(cr, 0.0, SEAT_Z + 0.2, 1.2, 0.9, 0.45, 0.0, seat, edge);
    iso_box(cr, 0.0, SEAT_Z + 0.62, 1.2, 0.18, 0.95, 0.0, seat, edge);
    let hd = iso(0.0, 1.05, SEAT_Z + 0.25);
    cr.arc(hd.0, hd.1, 9.0, 0.0, TAU);
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.fill();
    // The speakers, back to front, on stands (the sub on the floor).
    let mut order: Vec<&'static str> = chans.to_vec();
    order.sort_by(|a, b| {
        let (pa, pb) = (place(a), place(b));
        (pa.0 + pa.1).total_cmp(&(pb.0 + pb.1))
    });
    let mut hits = vec![];
    let mut tips = vec![];
    for ch in &order {
        let (x, z) = place(ch);
        let sub = *ch == "LFE";
        let on = lit == Some(*ch);
        let fill = if on { [rgb(0xff5a62), rgb(0xe3262f), rgb(0xb51c24)] } else { [rgb(0xffffff), rgb(0xf1d6d8), rgb(0xd9b3b6)] };
        if !sub {
            let (a, b) = (iso(x, 0.0, z), iso(x, 0.7, z));
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(1.4);
            cr.move_to(a.0, a.1);
            cr.line_to(b.0, b.1);
            let _ = cr.stroke();
        }
        let (bw, bd, bh, y0) = if sub { (0.6, 0.6, 0.6, 0.0) } else { (0.38, 0.3, 0.62, 0.7) };
        iso_box(cr, x, z, bw, bd, bh, y0, fill, white);
        let c = iso(x, if sub { 0.3 } else { 1.0 }, z + if sub { 0.3 } else { 0.15 });
        if on {
            // Two rings spreading out, half a beat apart.
            for d in [0.0, 0.6] {
                let p = ((t + d) % 1.2) / 1.2;
                cr.arc(c.0, c.1, 18.0 * (0.7 + 1.8 * p), 0.0, TAU);
                cr.set_source_rgba(1.0, 0.294, 0.325, 1.0 - p);
                cr.set_line_width(2.5);
                let _ = cr.stroke();
            }
        }
        // The box on screen, roughly, for tapping.
        let (l, r) = (iso(x - bw / 2.0, y0 + bh, z + bd / 2.0).0, iso(x + bw / 2.0, y0 + bh, z - bd / 2.0).0);
        let (top, bottom) = (iso(x - bw / 2.0, y0 + bh, z - bd / 2.0).1, iso(x + bw / 2.0, y0, z + bd / 2.0).1);
        hits.push((*ch, to_px([l.min(r) - 6.0, top - 6.0, (r - l).abs() + 12.0, bottom - top + 12.0])));
        tips.push((*ch, c));
    }
    // Their names, on leader lines.
    cr.select_font_face("Anton", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
    cr.set_font_size(32.0);
    for (ch, c) in tips {
        let (lx, ly, ends) = label_at(ch);
        let on = lit == Some(ch);
        let (red, dim) = ((1.0, 0.294, 0.325, 1.0), (1.0, 1.0, 1.0, 0.44));
        let line = if on { red } else { dim };
        cr.set_source_rgba(line.0, line.1, line.2, line.3);
        cr.set_line_width(if on { 2.5 } else { 1.5 });
        cr.move_to(c.0, c.1);
        cr.line_to(lx + if ends { 8.0 } else { -8.0 }, ly - 11.0);
        let _ = cr.stroke();
        cr.arc(c.0, c.1, 4.0, 0.0, TAU);
        if on { cr.set_source_rgba(red.0, red.1, red.2, 1.0) } else { cr.set_source_rgb(1.0, 1.0, 1.0) }
        let _ = cr.fill();
        let name = model::channel_name(ch).to_uppercase();
        let Ok(e) = cr.text_extents(&name) else { continue };
        let tx = if ends { lx - e.x_advance() } else { lx };
        if on { cr.set_source_rgba(red.0, red.1, red.2, 1.0) } else { cr.set_source_rgb(1.0, 1.0, 1.0) }
        cr.move_to(tx, ly);
        let _ = cr.show_text(&name);
        hits.push((ch, to_px([tx - 4.0, ly + e.y_bearing() - 4.0, e.x_advance() + 8.0, e.height() + 8.0])));
    }
    let _ = cr.restore();
    hits
}

impl Ui {
    /// Opens the full-screen speaker test (the dock's speaker button).
    pub fn open_speaker_test(self: &Rc<Self>) {
        if self.page.test_pg.borrow().is_some() {
            return;
        }
        self.close_panel();
        let s = self.s.borrow().clone();
        let hp = s.headphones();
        let chans: Vec<&'static str> = Self::channels_now(&s).to_vec();
        let ov = gtk::Overlay::builder().css_classes(["spktest"]).hexpand(true).vexpand(true).focusable(true).build();
        // The page's photo, printed in red, behind a shade.
        let photo = gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).can_shrink(true).hexpand(true).vexpand(true).build();
        photo.set_paintable(self.stage.bg_paintable().as_ref());
        ov.set_child(Some(&photo));
        ov.add_overlay(&gtk::Box::builder().css_classes(["shade"]).hexpand(true).vexpand(true).can_target(false).build());
        // At the top: what it tests and its title; × goes back.
        let kick = label(&if hp { "HEADPHONES".to_string() } else { format!("SPEAKERS · {}", s.speaker_config) }, &["kick"]);
        let title = label(if hp { "Test your headphones" } else { "Test your speakers" }, &["ttl"]);
        let words = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).hexpand(true).build();
        words.append(&kick);
        words.append(&title);
        let close = gtk::Button::builder().label("×").css_classes(["x"]).tooltip_text("Back (Esc)").valign(gtk::Align::Start).build();
        let top = gtk::Box::builder().valign(gtk::Align::Start).build();
        top.append(&words);
        top.append(&close);
        ov.add_overlay(&top);
        // The room.
        let room = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        {
            let wk = Rc::downgrade(self);
            let chans = chans.clone();
            room.set_draw_func(move |_, cr, w, h| {
                let Some(u) = wk.upgrade() else { return };
                let lit = u.room_lit.borrow().clone();
                let hits = draw_room(cr, w as f64, h as f64, &chans, lit.as_deref(), u.t0.elapsed().as_secs_f64());
                *u.page.test_hits.borrow_mut() = hits;
            });
        }
        // (the rings move while a speaker plays)
        room.add_tick_callback(|da, _| {
            da.queue_draw();
            glib::ControlFlow::Continue
        });
        ov.add_overlay(&room);
        // At the bottom: Walk round, Noise or Voice, and a note.
        let walk = gtk::Button::builder().label("▶  WALK ROUND").css_classes(["walk"]).build();
        let (seg, sbtns) = segmented(&["NOISE", "VOICE"], false);
        seg.add_css_class("tseg");
        seg.set_valign(gtk::Align::Center);
        sbtns[usize::from(s.test_sound == "voice")].set_active(true);
        let playing = self.np.borrow().as_ref().is_some_and(|n| n.playing);
        let note = label(if playing { "Music is paused while you test" } else { "" }, &["note"]);
        note.set_hexpand(true);
        note.set_xalign(1.0);
        let foot = gtk::Box::builder().spacing(14).valign(gtk::Align::End).build();
        foot.append(&walk);
        foot.append(&seg);
        foot.append(&note);
        ov.add_overlay(&foot);
        // Tapping a speaker or its name plays it (and stops a walk).
        {
            let wk = Rc::downgrade(self);
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, x, y| {
                let Some(u) = wk.upgrade() else { return };
                let hit = u.page.test_hits.borrow().iter()
                    .find(|(_, b)| x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]).map(|h| h.0);
                if let Some(ch) = hit {
                    u.stop_walk();
                    u.test_speaker(ch);
                }
            });
            room.add_controller(click);
        }
        {
            let wk = Rc::downgrade(self);
            walk.connect_clicked(move |b| if let Some(u) = wk.upgrade() {
                if u.page.test_walk.get().is_some() {
                    u.stop_walk();
                } else {
                    b.set_label("■  STOP");
                    b.add_css_class("on");
                    let chans: Vec<&'static str> = WALK.iter().copied().filter(|c| Self::channels_now(&u.s.borrow()).contains(c)).collect();
                    u.walk_step(chans, 0);
                }
            });
        }
        for (b, id) in sbtns.iter().zip(["noise", "voice"]) {
            let wk = Rc::downgrade(self);
            b.connect_toggled(move |b| if b.is_active() && let Some(u) = wk.upgrade() {
                u.s.borrow_mut().test_sound = id.into();
                u.save_now();
            });
        }
        {
            let wk = Rc::downgrade(self);
            close.connect_clicked(move |_| if let Some(u) = wk.upgrade() { u.close_speaker_test() });
        }
        self.root_ov.add_overlay(&ov);
        *self.page.test_pg.borrow_mut() = Some(ov.clone());
        *self.page.test_parts.borrow_mut() = Some((top, room, foot, title, walk));
        self.size_speaker_test(self.sizer.width(), self.sizer.height());
        // Music pauses while you test.
        self.page.test_resume.set(playing);
        if playing {
            let t = self.np.borrow().as_ref().map(|n| n.title.clone()).unwrap_or_default();
            page::mpris("Pause", &t);
        }
        ov.grab_focus();
    }

    /// Closes the speaker test; music that was playing plays again.
    pub fn close_speaker_test(&self) {
        let Some(ov) = self.page.test_pg.borrow_mut().take() else { return };
        self.stop_walk();
        self.root_ov.remove_overlay(&ov);
        *self.page.test_parts.borrow_mut() = None;
        if self.page.test_resume.replace(false) {
            let t = self.np.borrow().as_ref().map(|n| n.title.clone()).unwrap_or_default();
            page::mpris("Play", &t);
        }
    }

    /// The test page's spacing and title size follow the window (the design's
    /// 46 px sides, title clamp(40px, 4.4vw, 76px)).
    pub(crate) fn size_speaker_test(&self, w: i32, h: i32) {
        let parts = self.page.test_parts.borrow();
        let Some((top, room, foot, title, _)) = parts.as_ref() else { return };
        let side = ((w as f64 * 0.029) as i32).clamp(18, 46);
        let px = (w as f64 * 0.044).clamp(40.0, 76.0);
        let attrs = gtk::pango::AttrList::new();
        attrs.insert(gtk::pango::AttrSize::new_size_absolute((px * gtk::pango::SCALE as f64) as i32));
        title.set_attributes(Some(&attrs));
        for b in [top, foot] {
            b.set_margin_start(side);
            b.set_margin_end(side);
        }
        top.set_margin_top(((h as f64) * 0.032) as i32 + 12);
        foot.set_margin_bottom(((h as f64) * 0.03) as i32 + 8);
        room.set_margin_start(side);
        room.set_margin_end(side);
        room.set_margin_top((px * 0.95) as i32 + 70);
        room.set_margin_bottom(96);
    }

    /// Walk round: each speaker in turn, a little under two seconds apart.
    fn walk_step(self: &Rc<Self>, chans: Vec<&'static str>, i: usize) {
        let run = self.page.test_gen.get().wrapping_add(1);
        self.page.test_gen.set(run);
        let Some(ch) = chans.get(i).copied() else {
            self.stop_walk();
            return;
        };
        self.page.test_walk.set(Some(i));
        self.test_speaker(ch);
        let wk = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(1800), move || {
            if let Some(u) = wk.upgrade() && u.page.test_gen.get() == run && u.page.test_walk.get().is_some() {
                u.walk_step(chans, i + 1);
            }
        });
    }

    fn stop_walk(&self) {
        self.page.test_gen.set(self.page.test_gen.get().wrapping_add(1));
        self.page.test_walk.set(None);
        if let Some((.., walk)) = self.page.test_parts.borrow().as_ref() {
            walk.set_label("▶  WALK ROUND");
            walk.remove_css_class("on");
        }
    }
}
