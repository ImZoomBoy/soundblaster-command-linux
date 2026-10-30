//! The compact views, picked in prototype rounds 103–112:
//!
//! - the **Remote**, a tall card for small windows: the artist photo printed
//!   in red (or the album art whole, an option), the title, the readout, the
//!   room with the speakers, the four sound choices as tiles (their options
//!   rise in a sheet from the bottom) and the controls;
//! - the **Strip**, one thin row for the edge of a screen (a second screen left
//!   open all day): the cover, the title and readout, the six cabinets with
//!   their meters, the four choices side by side and the controls, with a pin
//!   that keeps it above other windows.
//!
//! Both are widgets only; the page fills them (`Stage` forwards what it gets)
//! and their presses go back through `send` as the page's own actions:
//! "act:pp", "pick:room:Concert", and so on. The rooms are drawn by the page.

use crate::stage::{Field, Head};
use gtk::cairo::ImageSurface;
use gtk::prelude::*;
use gtk::gdk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub const CSS: &str = r#"
/* the Remote */
.remote { background: #0b0b0c; }
.remote .r-photo { background: #e3262f; }
.remote .r-fade { background: linear-gradient(to bottom, alpha(#0b0b0c,0.4) 0%, transparent 30%, transparent 50%, #0b0b0c 100%); }
.remote .r-blur { filter: blur(28px) brightness(0.5) saturate(1.25); }
.remote .r-kick { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.3px; color: white; text-shadow: 0 1px 8px black; }
.remote .r-title { font-family: "Anton"; font-size: 30px; line-height: 32px; color: white; }
.remote .r-title.long { font-size: 23px; line-height: 26px; }
.remote .r-artist { font-family: "Anton"; font-size: 17px; line-height: 21px; color: #ffd0d3; }
.remote .r-thumb { box-shadow: 0 10px 26px alpha(black,0.8), 0 0 0 1px alpha(white,0.25); }
.remote .r-cro, .strip .s-cro { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12.5px; letter-spacing: 1.2px; color: #ff4b53; padding: 4px 9px; border: 1px solid alpha(#e3262f,0.5); border-radius: 6px; background: #1a0507; }
.remote .r-tech { font-size: 11px; color: alpha(white,0.56); }
.remote .r-tile { background: #151517; border: 1px solid alpha(white,0.08); border-radius: 12px; padding: 8px 11px; min-height: 0; box-shadow: none; }
.remote .r-tile:hover { border-color: #e3262f; }
.remote .r-tile:disabled { opacity: 1; }
.remote .r-tile .t-l, .strip .c-l { font-family: "Archivo Narrow"; font-weight: 700; font-size: 9.5px; letter-spacing: 1.3px; color: #ff4b53; }
.remote .r-tile .t-v { font-family: "Anton"; font-weight: normal; font-size: 15.5px; color: white; }
.remote .r-tile:disabled .t-v { color: alpha(white,0.68); }
.remote .r-ctl button.round, .strip .s-ctl button.round, .strip .s-ctl menubutton.round > button, .remote .r-ctl menubutton.round > button { min-width: 34px; min-height: 34px; padding: 0; border-radius: 999px; border: 1px solid alpha(white,0.16); background: alpha(black,0.4); }
.remote .r-ctl button.round:checked, .strip .s-ctl button.round:checked { background: #e3262f; border-color: #e3262f; }
.remote .r-ctl button.pp, .strip .s-ctl button.pp { min-width: 42px; min-height: 42px; background: white; color: black; border: none; border-radius: 999px; padding: 0; }
.remote .r-ctl button.flat-t, .strip .s-ctl button.flat-t { min-width: 30px; min-height: 30px; padding: 0; background: none; border: none; }
.remote .r-ctl button.flat-t:disabled, .strip .s-ctl button.flat-t:disabled { opacity: 0.3; }
.remote scale trough, .strip scale trough { min-height: 3px; background: alpha(white,0.16); }
.remote scale highlight, .strip scale highlight { background: #e3262f; }
.remote scale slider, .strip scale slider { min-width: 13px; min-height: 13px; margin: -5px; background: white; border: none; box-shadow: none; }
.r-scrim { background: alpha(black,0.62); border: none; border-radius: 0; box-shadow: none; }
.r-sheet { background: #121214; border-radius: 16px 16px 0 0; box-shadow: 0 -1px 0 alpha(white,0.12), 0 -24px 60px alpha(black,0.8); }
.r-grab { min-width: 38px; min-height: 4px; border-radius: 2px; background: alpha(white,0.19); }
.r-sh { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.5px; color: #ff4b53; }
.r-done { border-radius: 999px; padding: 5px 14px; background: alpha(white,0.08); font-weight: 700; min-height: 0; }
.r-done:hover { background: #e3262f; }
.r-opt, popover.s-opts button { padding: 9px 12px; border-radius: 10px; background: none; box-shadow: none; }
.r-opt .o-t, popover.s-opts .o-t { font-family: "Anton"; font-weight: normal; font-size: 18px; color: alpha(white,0.72); }
.r-opt .o-s, popover.s-opts .o-s { font-family: "Archivo Narrow"; font-size: 11px; font-weight: 600; color: alpha(white,0.5); }
.r-opt:hover, popover.s-opts button:hover { background: alpha(white,0.06); }
.r-opt.cur, popover.s-opts button.cur { background: #e3262f; }
.r-opt.cur .o-t, .r-opt.cur .o-s, popover.s-opts button.cur .o-t, popover.s-opts button.cur .o-s { color: white; }
popover.s-opts .o-h { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 1.4px; color: #ff4b53; padding: 4px 12px 2px 12px; }
/* the ☰ menu's technical line and separators */
popover .amenu .mn { padding: 6px 12px 9px 12px; margin-bottom: 4px; border-bottom: 1px solid alpha(white,0.11); }
popover .amenu .mn-t { font-family: "Archivo Narrow"; font-weight: 700; font-size: 13px; letter-spacing: 1.3px; color: #ff4b53; }
popover .amenu .mn-s { font-size: 11.5px; color: alpha(white,0.6); }
popover .amenu .mn-sep { font-family: "Archivo Narrow"; font-weight: 700; font-size: 10.5px; letter-spacing: 1.3px; color: alpha(white,0.4); margin: 6px 12px 2px 12px; }
/* the window buttons, at the top right when the pointer comes near */
.wctl windowcontrols { border-spacing: 6px; }
.wctl windowcontrols button { min-width: 30px; min-height: 30px; padding: 0; border-radius: 999px; background: alpha(black,0.72); border: 1px solid alpha(white,0.16); color: white; box-shadow: none; }
.wctl windowcontrols button:hover { background: alpha(white,0.18); }
.wctl windowcontrols button.close:hover { background: #e3262f; border-color: #e3262f; }
/* Over your shoulder, mini */
.shoulder { background: #050505; }
.shoulder .sh-shade { background: linear-gradient(to bottom, alpha(black,0.85) 0%, alpha(black,0.45) 24%, alpha(black,0.2) 40%, alpha(black,0.72) 56%, #050505 68%); }
.shoulder .sh-kick { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11px; letter-spacing: 0.7px; color: #ff4b53; }
.shoulder .sh-title { font-family: "Anton"; font-size: 40px; line-height: 40px; color: white; text-shadow: 0 4px 30px alpha(black,0.7); }
.shoulder .sh-title.long { font-size: 27px; line-height: 30px; }
.shoulder .sh-artist { font-family: "Anton"; font-size: 20px; line-height: 24px; color: #ffd0d3; }
.shoulder .sh-big { font-family: "Anton"; font-size: 56px; color: white; text-shadow: 0 4px 30px alpha(black,0.7); }
.shoulder .sh-bigs { font-family: "Archivo Narrow"; font-weight: 700; font-size: 9.5px; letter-spacing: 1.1px; color: alpha(white,0.72); }
.shoulder .sh-live { background: #e3262f; font-family: "Archivo Narrow"; font-weight: 700; font-size: 10px; letter-spacing: 1.4px; padding: 4px 7px; border-radius: 2px; color: white; }
.shoulder .sh-vn { font-family: "Anton"; font-size: 15px; background: alpha(#050505,0.92); padding: 2px 7px; border-radius: 3px; color: white; }
.shoulder .sh-vd { font-family: "Archivo Narrow"; font-weight: 700; font-size: 9.5px; letter-spacing: 1px; color: alpha(white,0.66); }
.shoulder .sh-row { padding: 5px 0 6px 0; }
.shoulder .sh-row.first { border-top: 2px solid #e3262f; }
.shoulder .sh-l { font-family: "Archivo Narrow"; font-weight: 700; font-size: 10px; letter-spacing: 1.3px; color: #ff4b53; }
.shoulder .sh-v { border-style: solid; border-width: 0 2px; border-color: alpha(white,0.333); border-radius: 0; background: alpha(black,0.4); padding: 0 3px 0 5px; min-height: 0; box-shadow: none; }
.shoulder .sh-v:hover { border-color: #e3262f; }
.shoulder .sh-v.fixed { border-color: transparent; background: none; }
.shoulder .sh-v-l { font-family: "Anton"; font-weight: normal; font-size: 20px; color: white; }
.shoulder .sh-v.fixed .sh-v-l { color: alpha(white,0.82); }
.shoulder .sh-chip { border: 1px dashed alpha(white,0.4); border-radius: 999px; padding: 3px 11px; background: alpha(black,0.5); min-height: 0; box-shadow: none; }
.shoulder .sh-chip:hover { border-color: #e3262f; }
.shoulder .sh-chip-l { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12px; color: white; }
.shoulder .r-ctl button.round, .shoulder .r-ctl menubutton.round > button { min-width: 34px; min-height: 34px; padding: 0; border-radius: 999px; border: 1px solid alpha(white,0.16); background: alpha(black,0.4); }
.shoulder .r-ctl button.round:checked { background: #e3262f; border-color: #e3262f; }
.shoulder .r-ctl button.pp { min-width: 42px; min-height: 42px; background: white; color: black; border: none; border-radius: 999px; padding: 0; }
.shoulder .r-cro { font-family: "Archivo Narrow"; font-weight: 700; font-size: 12.5px; letter-spacing: 1.2px; color: #ff4b53; padding: 4px 9px; border: 1px solid alpha(#e3262f,0.5); border-radius: 6px; background: #1a0507; }
.shoulder scale trough { min-height: 3px; background: alpha(white,0.16); }
.shoulder scale highlight { background: #e3262f; }
.shoulder scale slider { min-width: 13px; min-height: 13px; margin: -5px; background: white; border: none; box-shadow: none; }
/* a side panel closes with Done; the window's buttons sit at its header's right end */
.side-done { border-radius: 999px; padding: 5px 14px; background: alpha(white,0.08); font-weight: 700; min-height: 0; box-shadow: none; }
.side-done:hover { background: #e3262f; }
.side-done.ghost { background: none; border: 1px solid alpha(white,0.18); font-weight: 600; }
.side-done.ghost:hover { background: alpha(white,0.08); border-color: alpha(white,0.4); }
.side-wc button { min-width: 28px; min-height: 28px; padding: 0; border-radius: 999px; background: alpha(white,0.06); border: none; box-shadow: none; }
.side-wc button:hover { background: alpha(white,0.16); }
.side-wc button.close:hover { background: #e3262f; }
/* the Strip */
.strip { background: linear-gradient(to bottom, #1a1a1d, #121214); }
.strip .s-sep { min-width: 1px; background: alpha(white,0.07); }
.strip .s-title { font-family: "Anton"; font-size: 19px; color: white; }
.strip .s-artist { font-family: "Anton"; font-size: 13.5px; color: #ffd0d3; }
/* the readout sits in the controls, as on the page */
.strip .s-ro { font-family: "Archivo Narrow"; font-weight: 700; font-size: 11.5px; letter-spacing: 1.2px; color: #ff4b53; padding: 5px 9px; border: 1px solid alpha(#e3262f,0.4); border-radius: 6px; background: alpha(#1a0507,0.8); }
/* the window's own buttons: a group of their own at the far right */
.strip .s-wc button { min-width: 24px; min-height: 24px; padding: 0; border-radius: 999px; background: alpha(white,0.06); border: none; box-shadow: none; }
.strip .s-wc button:hover { background: alpha(white,0.16); }
.strip .s-wc button.close:hover { background: #e3262f; }
.strip .s-choice button { padding: 3px 7px; border-radius: 6px; background: none; box-shadow: none; min-height: 0; border: none; }
.strip .s-choice button:hover { background: alpha(white,0.06); }
.strip .c-l { font-size: 8.5px; }
.strip .c-v { font-family: "Anton"; font-weight: normal; font-size: 15px; color: white; }
.strip .s-ctl button.round, .strip .s-ctl menubutton.round > button { min-width: 30px; min-height: 30px; }
.strip .s-ctl button.pp { min-width: 36px; min-height: 36px; }
.strip .s-ctl button.flat-t { min-width: 26px; min-height: 26px; }
.strip .s-ctl button.pin:checked { background: white; color: black; border-color: white; }
"#;

type Send = RefCell<Option<Rc<dyn Fn(&str)>>>;

fn lbl(text: &str, cls: &[&str]) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(cls.iter().map(|s| s.to_string()).collect::<Vec<_>>()).build()
}

fn icon_btn(icon: &str, tip: &str, cls: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).tooltip_text(tip).css_classes([cls]).valign(gtk::Align::Center).build()
}

/// A picture drawn at the size wanted: the whole picture ("contain"), or filling
/// the area ("cover"), from a cairo surface.
#[derive(Clone)]
pub struct Pic {
    pub area: gtk::DrawingArea,
    surf: Rc<RefCell<Option<ImageSurface>>>,
}

impl Pic {
    fn new(cover: bool) -> Pic {
        let area = gtk::DrawingArea::new();
        let surf: Rc<RefCell<Option<ImageSurface>>> = Rc::new(RefCell::new(None));
        let sf = surf.clone();
        area.set_draw_func(move |_, cr, w, h| {
            let Some(s) = sf.borrow().clone() else { return };
            let (iw, ih) = (s.width() as f64, s.height() as f64);
            if iw < 1.0 || ih < 1.0 { return; }
            let (w, h) = (w as f64, h as f64);
            let k = if cover { (w / iw).max(h / ih) } else { (w / iw).min(h / ih) };
            let (dw, dh) = (iw * k, ih * k);
            cr.save().ok();
            cr.rectangle(0.0, 0.0, w, h);
            cr.clip();
            cr.translate((w - dw) / 2.0, (h - dh) / 2.0);
            cr.scale(k, k);
            let _ = cr.set_source_surface(&s, 0.0, 0.0);
            cr.source().set_filter(gtk::cairo::Filter::Good);
            let _ = cr.paint();
            cr.restore().ok();
        });
        Pic { area, surf }
    }

    fn set(&self, s: Option<ImageSurface>) {
        *self.surf.borrow_mut() = s;
        self.area.queue_draw();
    }

    /// (width, height) of the picture, if there is one.
    fn size(&self) -> Option<(f64, f64)> {
        self.surf.borrow().as_ref().map(|s| (s.width() as f64, s.height() as f64))
    }
}

/// A texture as a cairo surface (GTK hands its pixels over in cairo's own format).
fn surface_of(tex: &gdk::Texture) -> Option<ImageSurface> {
    let mut surf = ImageSurface::create(gtk::cairo::Format::ARgb32, tex.width(), tex.height()).ok()?;
    let stride = surf.stride() as usize;
    {
        let mut data = surf.data().ok()?;
        tex.download(&mut data, stride);
    }
    surf.mark_dirty();
    Some(surf)
}

/// The four choices' small print: the phrase before the comma ("close" for
/// "close, like the studio"), for the Strip and the receiver-style spaces.
pub fn short(f: &Field) -> String {
    if f.chip { return f.value.clone(); }
    let v = f.value.split(", ").next().unwrap_or_default();
    // "like a cinema" → "cinema", "all six speakers" → "all six", "in front of you" → "in front", "a little warmer" → "warmer"
    let v = ["like a ", "like the ", "in the ", "a little ", "on "].iter().find_map(|p| v.strip_prefix(p)).unwrap_or(v);
    let v = v.strip_suffix(" speakers").or_else(|| v.strip_suffix(" of you")).or_else(|| v.strip_suffix(" you")).unwrap_or(v);
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_words_for_the_strip() {
        let f = |v: &str| Field { value: v.into(), ..Default::default() };
        for (v, want) in [("close, like the studio", "close"), ("like a cinema", "cinema"), ("all six speakers", "all six"),
                          ("in front of you", "in front"), ("all around you", "all around"), ("a little warmer", "warmer"),
                          ("as mixed", "as mixed"), ("like the concert hall", "concert hall"), ("on headphones", "headphones"),
                          ("in the stands", "stands"), ("quietly, for late at night", "quietly"), ("untouched", "untouched")] {
            assert_eq!(short(&f(v)), want, "{v}");
        }
    }
}

// ---------------------------------------------------------------------------
// The Remote
// ---------------------------------------------------------------------------

pub struct Remote {
    pub root: gtk::Overlay,
    photo: gtk::Overlay,
    red: Pic,
    album_bk: Pic,
    album: Pic,
    kick: gtk::Label,
    title: gtk::Label,
    artist: gtk::Label,
    thumb: Pic,
    cro: gtk::Label,
    tech: gtk::Label,
    pub room: gtk::DrawingArea,
    tiles: gtk::Grid,
    pub prev: gtk::Button,
    pub pp: gtk::Button,
    pub next: gtk::Button,
    pub hp: gtk::ToggleButton,
    pub menu: gtk::Button,
    sheet: Rc<Sheet>,
    fields: RefCell<Vec<Field>>,
    album_mode: Cell<bool>,
    has_cover: Cell<bool>,
    size: Cell<(i32, i32)>,
    send: Send,
}

impl Remote {
    pub fn new(vol: &gtk::Adjustment) -> Rc<Remote> {
        // The top: the photo printed in red, or the album art whole over a blurred copy of itself.
        // (Drawn at the size wanted: a GtkPicture would ask for its whole height.)
        let red = Pic::new(true);
        red.area.set_hexpand(true);
        let photo = gtk::Overlay::builder().css_classes(["r-photo"]).build();
        photo.set_child(Some(&red.area));
        let album_bk = Pic::new(true);
        album_bk.area.add_css_class("r-blur");
        album_bk.area.set_visible(false);
        photo.add_overlay(&album_bk.area);
        let album = Pic::new(false);
        album.area.set_margin_top(18);
        album.area.set_margin_bottom(56);
        album.area.set_halign(gtk::Align::Center);
        album.area.add_css_class("r-thumb");
        album.area.set_visible(false);
        photo.add_overlay(&album.area);
        let fade = gtk::Box::builder().css_classes(["r-fade"]).can_target(false).build();
        photo.add_overlay(&fade);
        let kick = lbl("", &["r-kick"]);
        kick.set_halign(gtk::Align::Start);
        kick.set_valign(gtk::Align::Start);
        kick.set_margin_start(16);
        kick.set_margin_top(14);
        photo.add_overlay(&kick);
        let title = lbl("", &["r-title"]);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let artist = lbl("", &["r-artist"]);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let tcol = gtk::Box::builder().orientation(gtk::Orientation::Vertical).hexpand(true).valign(gtk::Align::End).build();
        tcol.append(&title);
        tcol.append(&artist);
        let thumb = Pic::new(true);
        thumb.area.set_content_width(66);
        thumb.area.set_content_height(66);
        thumb.area.set_valign(gtk::Align::End);
        thumb.area.add_css_class("r-thumb");
        let trow = gtk::Box::builder().spacing(12).valign(gtk::Align::End).margin_start(16).margin_end(16).build();
        trow.append(&tcol);
        trow.append(&thumb.area);
        photo.add_overlay(&trow);

        let cro = lbl("", &["r-cro"]);
        let tech = lbl("", &["r-tech"]);
        tech.set_ellipsize(gtk::pango::EllipsizeMode::End);
        tech.set_hexpand(true);
        let rrow = gtk::Box::builder().spacing(9).margin_start(16).margin_end(16).margin_top(9).build();
        rrow.append(&cro);
        rrow.append(&tech);

        let room = gtk::DrawingArea::builder().vexpand(true).hexpand(true).build();
        room.set_tooltip_text(Some("Tap a speaker to test it"));
        let tiles = gtk::Grid::builder().column_homogeneous(true).row_spacing(7).column_spacing(7).margin_start(12).margin_end(12).build();

        let prev = icon_btn("media-skip-backward-symbolic", "Previous", "flat-t");
        let pp = icon_btn("media-playback-pause-symbolic", "Play / pause", "pp");
        let next = icon_btn("media-skip-forward-symbolic", "Next", "flat-t");
        let volume = gtk::Scale::builder().adjustment(vol).orientation(gtk::Orientation::Horizontal).draw_value(false)
            .hexpand(true).valign(gtk::Align::Center).tooltip_text("Volume").build();
        let hp = gtk::ToggleButton::builder().icon_name("audio-headphones-symbolic").tooltip_text("Headphones")
            .css_classes(["round"]).valign(gtk::Align::Center).build();
        // ☰ opens the control centre (not a menu).
        let menu = gtk::Button::builder().icon_name("open-menu-symbolic").tooltip_text("Sound, Mixer, Microphone, Speakers, Look and feel, Settings").css_classes(["round"])
            .valign(gtk::Align::Center).build();
        let ctl = gtk::Box::builder().spacing(6).css_classes(["r-ctl"]).margin_start(14).margin_end(14).margin_top(10).margin_bottom(14).build();
        for w in [prev.upcast_ref::<gtk::Widget>(), pp.upcast_ref(), next.upcast_ref(), volume.upcast_ref(), hp.upcast_ref(), menu.upcast_ref()] {
            ctl.append(w);
        }

        let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        col.append(&photo);
        col.append(&rrow);
        col.append(&room);
        col.append(&tiles);
        col.append(&ctl);
        let root = gtk::Overlay::builder().css_classes(["remote"]).hexpand(true).vexpand(true).build();
        root.set_child(Some(&col));
        root.set_overflow(gtk::Overflow::Hidden);

        let sheet = Sheet::new(&root);

        let r = Rc::new(Remote {
            root, photo, red, album_bk, album, kick, title, artist, thumb, cro, tech, room, tiles, prev, pp, next, hp, menu,
            sheet, fields: RefCell::new(vec![]), album_mode: Cell::new(false), has_cover: Cell::new(false),
            size: Cell::new((400, 720)), send: RefCell::new(None),
        });
        for (b, a) in [(&r.prev, "prev"), (&r.pp, "pp"), (&r.next, "next"), (&r.menu, "menu")] {
            let wk = Rc::downgrade(&r);
            b.connect_clicked(move |_| if let Some(r) = wk.upgrade() { r.emit(&format!("act:{a}")) });
        }
        {
            let wk = Rc::downgrade(&r);
            r.hp.connect_clicked(move |_| if let Some(r) = wk.upgrade() { r.emit("act:hp") });
        }
        r
    }

    pub fn set_send(&self, f: Rc<dyn Fn(&str)>) {
        self.sheet.set_send(f.clone());
        *self.send.borrow_mut() = Some(f);
    }

    fn emit(&self, a: &str) {
        let f = self.send.borrow().clone();
        if let Some(f) = f { f(a) }
    }

    /// The window's size: the top takes 31% of it (37% for the album art), and
    /// a wide window puts the four tiles on one line.
    pub fn resized(self: &Rc<Self>, w: i32, h: i32) {
        self.size.set((w, h));
        let k = if self.album_mode.get() { 0.37 } else { 0.31 };
        self.photo.set_size_request(-1, (h as f64 * k).round().max(110.0) as i32);
        if self.tiles.first_child().is_some() {
            let cols = if w >= 640 { 4 } else { 2 };
            if cols != self.cols() { self.lay_tiles() }
        }
        self.title.set_max_width_chars(((w as f64 - 120.0) / 14.0).max(8.0) as i32);
    }

    fn cols(&self) -> i32 {
        let mut n = 0;
        let mut c = self.tiles.first_child();
        while let Some(w) = c {
            let (col, row, _, _) = self.tiles.query_child(&w);
            if row == 0 { n = n.max(col + 1); }
            c = w.next_sibling();
        }
        n
    }

    /// The artist photo printed in red (the page's background), or the album
    /// art whole: `album` from Look and feel.
    pub fn set_top(self: &Rc<Self>, album: bool) {
        self.album_mode.set(album);
        let album = album && self.has_cover.get();
        self.red.area.set_visible(!album);
        self.album_bk.area.set_visible(album);
        self.album.area.set_visible(album);
        self.thumb.area.set_visible(!album && self.has_cover.get());
        let (w, h) = self.size.get();
        self.resized(w, h);
    }

    pub fn set_photo(&self, tex: Option<&gdk::Texture>) {
        self.red.set(tex.and_then(surface_of));
    }

    pub fn set_cover(self: &Rc<Self>, s: Option<ImageSurface>) {
        self.has_cover.set(s.is_some());
        if let Some((iw, ih)) = s.as_ref().map(|s| (s.width() as f64, s.height() as f64)) {
            // The thumbnail keeps the cover's shape (a poster or a game box is tall).
            self.thumb.area.set_content_width((66.0 * (iw / ih).min(1.0)).round() as i32);
        }
        self.album_bk.set(s.clone());
        self.album.set(s.clone());
        self.thumb.set(s);
        if let Some((iw, ih)) = self.album.size() {
            let (_, h) = self.size.get();
            let ah = (h as f64 * if self.album_mode.get() { 0.37 } else { 0.31 } - 74.0).max(60.0);
            self.album.area.set_content_width((ah * iw / ih).round() as i32);
        }
        self.set_top(self.album_mode.get());
    }

    pub fn set_head(&self, h: &Head) {
        self.kick.set_label(&h.kick.to_uppercase());
        self.title.set_label(&h.title);
        if h.title.chars().count() > 18 { self.title.add_css_class("long") } else { self.title.remove_css_class("long") }
        self.artist.set_label(&h.subtitle);
        self.artist.set_visible(!h.subtitle.is_empty());
    }

    pub fn set_readout(&self, read: &str, tech: &str) {
        self.cro.set_label(read);
        self.tech.set_label(tech);
    }

    pub fn set_fields(self: &Rc<Self>, fields: &[Field]) {
        if *self.fields.borrow() == fields { return; }
        *self.fields.borrow_mut() = fields.to_vec();
        self.lay_tiles();
        self.sheet.follow(fields);
    }

    fn lay_tiles(self: &Rc<Self>) {
        while let Some(c) = self.tiles.first_child() { self.tiles.remove(&c); }
        let cols = if self.size.get().0 >= 640 { 4 } else { 2 };
        for (i, f) in self.fields.borrow().iter().enumerate() {
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).build();
            col.append(&lbl(&f.label.to_uppercase(), &["t-l"]));
            let v = lbl(&f.value, &["t-v"]);
            v.set_ellipsize(gtk::pango::EllipsizeMode::End);
            col.append(&v);
            let b = gtk::Button::builder().child(&col).css_classes(["r-tile"]).tooltip_text(format!("{}: {}", f.label, f.value)).build();
            b.set_sensitive(!f.options.is_empty());
            let fc = f.clone();
            let wk = Rc::downgrade(self);
            b.connect_clicked(move |_| if let Some(r) = wk.upgrade() { r.sheet.open(&fc) });
            self.tiles.attach(&b, (i as i32) % cols, (i as i32) / cols, 1, 1);
        }
    }

    /// The labels on the four tiles, for the self-test.
    pub fn tile_values(&self) -> Vec<String> {
        self.fields.borrow().iter().map(|f| f.value.clone()).collect()
    }

    pub fn sheet_open(&self) -> bool {
        self.sheet.is_open()
    }

    /// Presses the tile for one choice (the self-test).
    pub fn press_tile(self: &Rc<Self>, key: &str) {
        if let Some(f) = self.fields.borrow().iter().find(|f| f.key == key).cloned() { self.sheet.open(&f) }
    }

    /// Picks an option in the open sheet, as a press would (the self-test).
    pub fn press_option(&self, id: &str) {
        self.sheet.press(id);
    }
}

// ---------------------------------------------------------------------------
// The sheet: a choice's options rising from the bottom (the Remote, the mini)
// ---------------------------------------------------------------------------

/// A choice's options rise in a sheet from the bottom of a compact view and stop
/// below the window buttons; Done or the dimmed window closes it.
pub struct Sheet {
    scrim: gtk::Button,
    rev: gtk::Revealer,
    title: gtk::Label,
    list: gtk::Box,
    field: RefCell<Option<Field>>,
    send: Send,
}

impl Sheet {
    fn new(root: &gtk::Overlay) -> Rc<Sheet> {
        let scrim = gtk::Button::builder().css_classes(["r-scrim"]).hexpand(true).vexpand(true).visible(false).build();
        root.add_overlay(&scrim);
        let title = lbl("", &["r-sh"]);
        title.set_hexpand(true);
        let done = gtk::Button::builder().label("Done").css_classes(["r-done"]).build();
        let head = gtk::Box::builder().spacing(10).margin_start(16).margin_end(12).margin_top(8).margin_bottom(6).build();
        head.append(&title);
        head.append(&done);
        let list = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(3).margin_start(8).margin_end(8).margin_bottom(12).build();
        let sw = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).propagate_natural_height(true).child(&list).build();
        let grab = gtk::Box::builder().css_classes(["r-grab"]).halign(gtk::Align::Center).margin_top(8).build();
        let sbox = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["r-sheet"]).build();
        sbox.append(&grab);
        sbox.append(&head);
        sbox.append(&sw);
        let rev = gtk::Revealer::builder().child(&sbox).valign(gtk::Align::End).transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(200).margin_top(52).build();
        root.add_overlay(&rev);
        let sh = Rc::new(Sheet { scrim, rev, title, list, field: RefCell::new(None), send: RefCell::new(None) });
        for b in [sh.scrim.clone(), done] {
            let wk = Rc::downgrade(&sh);
            b.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.close() });
        }
        sh
    }

    fn set_send(&self, f: Rc<dyn Fn(&str)>) {
        *self.send.borrow_mut() = Some(f);
    }

    fn emit(&self, a: &str) {
        let f = self.send.borrow().clone();
        if let Some(f) = f { f(a) }
    }

    fn open(self: &Rc<Self>, f: &Field) {
        self.fill(f);
        self.scrim.set_visible(true);
        self.rev.set_reveal_child(true);
    }

    fn fill(self: &Rc<Self>, f: &Field) {
        *self.field.borrow_mut() = Some(f.clone());
        self.title.set_label(&f.label.to_uppercase());
        while let Some(c) = self.list.first_child() { self.list.remove(&c); }
        for (id, phrase, sub) in &f.options {
            let bx = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(1).build();
            bx.append(&lbl(phrase, &["o-t"]));
            if !sub.is_empty() { bx.append(&lbl(sub, &["o-s"])); }
            let b = gtk::Button::builder().child(&bx).css_classes(["r-opt"]).build();
            if *id == f.current { b.add_css_class("cur"); }
            let id = id.clone();
            let wk = Rc::downgrade(self);
            b.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.press(&id) });
            self.list.append(&b);
        }
    }

    /// An open sheet follows a change (the current option moves).
    fn follow(self: &Rc<Self>, fields: &[Field]) {
        if !self.is_open() { return; }
        let key = self.field.borrow().as_ref().map(|f| f.key);
        if let Some(f) = key.and_then(|k| fields.iter().find(|f| f.key == k).cloned()) { self.fill(&f) }
    }

    fn close(&self) {
        self.rev.set_reveal_child(false);
        self.scrim.set_visible(false);
    }

    fn is_open(&self) -> bool {
        self.rev.reveals_child()
    }

    /// Picks an option: Extras switch on and off and stay open; the others close.
    fn press(&self, id: &str) {
        let f = self.field.borrow().clone();
        if let Some(f) = f {
            self.emit(&format!("pick:{}:{}", f.key, id));
            if !f.chip { self.close() }
        }
    }
}

// ---------------------------------------------------------------------------
// The Strip
// ---------------------------------------------------------------------------

pub struct Strip {
    pub root: gtk::Box,
    cover: Pic,
    title: gtk::Label,
    artist: gtk::Label,
    ro: gtk::Label,
    pub spk: gtk::DrawingArea,
    choices: gtk::Box,
    pub prev: gtk::Button,
    pub pp: gtk::Button,
    pub next: gtk::Button,
    pub hp: gtk::ToggleButton,
    pub pin: gtk::ToggleButton,
    pub menu: gtk::MenuButton,
    fields: RefCell<Vec<Field>>,
    send: Send,
}

impl Strip {
    pub fn new(vol: &gtk::Adjustment) -> Rc<Strip> {
        let sep = || gtk::Box::builder().css_classes(["s-sep"]).build();
        let cover = Pic::new(true);
        cover.area.set_content_width(76);
        cover.area.set_size_request(76, -1);
        let title = lbl("", &["s-title"]);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let artist = lbl("", &["s-artist"]);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let info = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).margin_start(14).margin_end(12)
            .width_request(156).build();
        info.append(&title);
        info.append(&artist);
        let ro = lbl("", &["s-ro"]);
        ro.set_valign(gtk::Align::Center);
        ro.set_tooltip_text(Some("What comes in → what plays"));
        let spk = gtk::DrawingArea::builder().hexpand(true).build();
        spk.set_tooltip_text(Some("Tap a speaker to test it"));
        spk.set_size_request(160, -1);
        let choices = gtk::Box::builder().spacing(2).margin_start(6).margin_end(6).valign(gtk::Align::Center).build();
        let prev = icon_btn("media-skip-backward-symbolic", "Previous", "flat-t");
        let pp = icon_btn("media-playback-pause-symbolic", "Play / pause", "pp");
        let next = icon_btn("media-skip-forward-symbolic", "Next", "flat-t");
        let volume = gtk::Scale::builder().adjustment(vol).orientation(gtk::Orientation::Horizontal).draw_value(false)
            .width_request(64).valign(gtk::Align::Center).tooltip_text("Volume").build();
        let hp = gtk::ToggleButton::builder().icon_name("audio-headphones-symbolic").tooltip_text("Headphones")
            .css_classes(["round"]).valign(gtk::Align::Center).build();
        let pin = gtk::ToggleButton::builder().icon_name("view-pin-symbolic").tooltip_text("Keep above other windows, on every desktop")
            .css_classes(["round", "pin"]).valign(gtk::Align::Center).build();
        let menu = gtk::MenuButton::builder().icon_name("open-menu-symbolic").tooltip_text("Menu").css_classes(["round"])
            .valign(gtk::Align::Center).build();
        // As on the page: play, the readout, headphones, the volume; then the pin and ☰.
        let ctl = gtk::Box::builder().spacing(6).css_classes(["s-ctl"]).margin_start(12).margin_end(12).valign(gtk::Align::Center).build();
        for w in [prev.upcast_ref::<gtk::Widget>(), pp.upcast_ref(), next.upcast_ref(), ro.upcast_ref(), hp.upcast_ref(), volume.upcast_ref(),
                  pin.upcast_ref(), menu.upcast_ref()] {
            ctl.append(w);
        }
        // The window's own buttons in a group of their own, apart from the rest.
        let wc = gtk::WindowControls::builder().side(gtk::PackType::End).decoration_layout(":minimize,close").css_classes(["s-wc"])
            .valign(gtk::Align::Center).margin_start(10).margin_end(10).build();
        let root = gtk::Box::builder().css_classes(["strip"]).hexpand(true).vexpand(true).build();
        for w in [cover.area.upcast_ref::<gtk::Widget>(), info.upcast_ref(), sep().upcast_ref(), spk.upcast_ref(), sep().upcast_ref(),
                  choices.upcast_ref(), sep().upcast_ref(), ctl.upcast_ref(), sep().upcast_ref(), wc.upcast_ref()] {
            root.append(w);
        }
        let s = Rc::new(Strip { root, cover, title, artist, ro, spk, choices, prev, pp, next, hp, pin, menu, fields: RefCell::new(vec![]), send: RefCell::new(None) });
        for (b, a) in [(&s.prev, "prev"), (&s.pp, "pp"), (&s.next, "next")] {
            let wk = Rc::downgrade(&s);
            b.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.emit(&format!("act:{a}")) });
        }
        {
            let wk = Rc::downgrade(&s);
            s.hp.connect_clicked(move |_| if let Some(s) = wk.upgrade() { s.emit("act:hp") });
            let wk = Rc::downgrade(&s);
            s.pin.connect_clicked(move |b| if let Some(s) = wk.upgrade() { s.emit(if b.is_active() { "act:pin-on" } else { "act:pin-off" }) });
        }
        s
    }

    pub fn set_send(&self, f: Rc<dyn Fn(&str)>) {
        *self.send.borrow_mut() = Some(f);
    }

    fn emit(&self, a: &str) {
        let f = self.send.borrow().clone();
        if let Some(f) = f { f(a) }
    }

    pub fn set_head(&self, h: &Head) {
        self.title.set_label(&h.title);
        self.artist.set_label(&h.subtitle);
        self.artist.set_visible(!h.subtitle.is_empty());
    }

    pub fn set_cover(&self, s: Option<ImageSurface>) {
        self.cover.area.set_visible(s.is_some());
        self.cover.set(s);
    }

    pub fn set_readout(&self, read: &str) {
        self.ro.set_label(read);
    }

    pub fn set_fields(self: &Rc<Self>, fields: &[Field]) {
        if *self.fields.borrow() == fields { return; }
        *self.fields.borrow_mut() = fields.to_vec();
        while let Some(c) = self.choices.first_child() { self.choices.remove(&c); }
        for f in fields {
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(60).build();
            col.append(&lbl(&f.label.to_uppercase(), &["c-l"]));
            let v = lbl(&short(f), &["c-v"]);
            v.set_ellipsize(gtk::pango::EllipsizeMode::End);
            v.set_max_width_chars(11);
            col.append(&v);
            let tip = format!("{}: {}", f.label, f.value);
            if f.options.is_empty() {
                let b = gtk::Box::builder().css_classes(["s-choice"]).tooltip_text(tip).build();
                let bt = gtk::Button::builder().child(&col).can_target(false).build();
                b.append(&bt);
                self.choices.append(&b);
                continue;
            }
            // The options drop out of the bar (a popover may reach past a thin window).
            let pop = gtk::Popover::builder().css_classes(["s-opts"]).has_arrow(false).build();
            let list = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_top(6).margin_bottom(6)
                .margin_start(6).margin_end(6).build();
            list.append(&lbl(&f.label.to_uppercase(), &["o-h"]));
            for (id, phrase, sub) in &f.options {
                let bx = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
                bx.append(&lbl(phrase, &["o-t"]));
                if !sub.is_empty() { bx.append(&lbl(sub, &["o-s"])); }
                let b = gtk::Button::builder().child(&bx).build();
                if *id == f.current { b.add_css_class("cur"); }
                let key = format!("pick:{}:{}", f.key, id);
                let wk = Rc::downgrade(self);
                let pw = pop.downgrade();
                b.connect_clicked(move |_| {
                    if let Some(p) = pw.upgrade() { p.popdown() }
                    if let Some(s) = wk.upgrade() { s.emit(&key) }
                });
                list.append(&b);
            }
            pop.set_child(Some(&list));
            let mb = gtk::MenuButton::builder().child(&col).popover(&pop).tooltip_text(tip).build();
            let b = gtk::Box::builder().css_classes(["s-choice"]).build();
            b.append(&mb);
            self.choices.append(&b);
        }
    }

    /// The short words in the four choices, for the self-test.
    pub fn choice_words(&self) -> Vec<String> {
        self.fields.borrow().iter().map(short).collect()
    }
}

/// Keeps the Strip above other windows and on every desktop (or lets it go), with
/// a KWin script: KDE only; elsewhere the window manager's own menu does it.
pub fn pin_window(on: bool, app_id: &str) {
    let app_id = app_id.to_string();
    std::thread::spawn(move || {
        let js = format!(
            "const ws = workspace.windowList ? workspace.windowList() : workspace.clientList();\n\
             for (const w of ws) {{ if (w.resourceClass == \"{app_id}\" || w.desktopFileName == \"{app_id}\") {{ w.keepAbove = {on}; w.onAllDesktops = {on}; }} }}\n");
        let path = crate::system::cache_dir().join("pin.js");
        if std::fs::write(&path, js).is_err() { return; }
        let call = |obj: &str, method: &str, args: &[&str]| -> Option<String> {
            let mut c = std::process::Command::new("gdbus");
            c.args(["call", "--session", "--dest", "org.kde.KWin", "--object-path", obj, "--method", method]);
            c.args(args);
            let out = c.output().ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
        };
        let _ = call("/Scripting", "org.kde.kwin.Scripting.unloadScript", &["sbcommand-pin"]);
        let Some(id) = call("/Scripting", "org.kde.kwin.Scripting.loadScript", &[&path.to_string_lossy(), "sbcommand-pin"]) else { return };
        let n: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
        if n.is_empty() { return; }
        // KWin 6 names the script object /Scripting/Script<N>; KWin 5 /<N>.
        if call(&format!("/Scripting/Script{n}"), "org.kde.kwin.Script.run", &[]).is_none() {
            let _ = call(&format!("/{n}"), "org.kde.kwin.Script.run", &[]);
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
        let _ = call("/Scripting", "org.kde.kwin.Scripting.unloadScript", &["sbcommand-pin"]);
    });
}

// ---------------------------------------------------------------------------
// Over your shoulder, mini: the main page made tall (rounds 103–108)
// ---------------------------------------------------------------------------

/// What's on the room's back wall in the mini.
pub enum Sign<'a> {
    /// Songs: the artist's name in neon (their logo, or the name in Anton).
    Neon { logo: Option<&'a ImageSurface>, text: &'a str },
    /// Films, series and games: an LED wall of dots, the logo, then a still.
    Led { logo: Option<&'a ImageSurface>, pic: Option<&'a ImageSurface> },
    /// Football: the teams on a dot-matrix scoreboard.
    Score { home: &'a str, away: &'a str, sub: &'a str },
    None,
}

/// Everything the mini's room shows (speakers in `room::SPEAKERS` order).
pub struct MiniScene<'a> {
    pub plays: [bool; 6],
    pub levels: [f32; 6],
    pub lit: Option<&'a str>,
    pub headphones: bool,
    /// How wide the sound is (0..1, from the SBX profile) and whether it surrounds you.
    pub spread: f64,
    pub around: bool,
    pub tone_rgb: (f64, f64, f64),
    pub sign: Sign<'a>,
    pub playing: bool,
    pub t: f64,
}

/// One speaker in dots, as on the main page: white dots, the woofer in red dots
/// (brighter with the level), the tweeter one red dot; grey when it's silent.
fn dot_box(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, cell: f64, on: bool, lv: f64, lit: bool) {
    let cols = (w / cell).round().max(3.0) as i32;
    let rows = (h / cell).round().max(4.0) as i32;
    let r = cell * 0.36;
    let cx = (cols - 1) as f64 / 2.0;
    let wy = rows as f64 * 0.62;
    let wr = (cols.min(rows)) as f64 * 0.3;
    for j in 0..rows {
        for i in 0..cols {
            let px = x - w / 2.0 + i as f64 * w / ((cols - 1).max(1)) as f64;
            let py = y - h / 2.0 + j as f64 * h / ((rows - 1).max(1)) as f64;
            let in_w = ((i as f64 - cx).powi(2) + ((j as f64 - wy) * 1.05).powi(2)).sqrt() < wr;
            let in_t = ((i as f64 - cx).powi(2) + (j as f64 - rows as f64 * 0.2).powi(2)).sqrt() < wr * 0.35;
            let (cr_, cg, cb, ca) = if lit { (1.0, 0.294, 0.325, 1.0) } else if !on { (1.0, 1.0, 1.0, 0.22) }
                else if in_w { if lv > 0.15 { (0.89, 0.149, 0.184, 0.55 + 0.45 * lv) } else { (0.541, 0.106, 0.129, 1.0) } }
                else if in_t { (0.89, 0.149, 0.184, 1.0) } else { (1.0, 1.0, 1.0, 1.0) };
            cr.set_source_rgba(cr_, cg, cb, ca);
            cr.new_path();
            cr.arc(px, py, r * if in_w && on { 1.0 + 0.3 * lv } else { 1.0 }, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    }
}

thread_local! {
    /// Pictures sampled for the LED wall (by surface, grid size and fit).
    static LED: RefCell<std::collections::HashMap<(usize, i32, i32, bool), Rc<Vec<f32>>>> = RefCell::new(Default::default());
}

/// A picture as a grid of brightness (0..1), for the LED wall: a logo whole
/// (`contain`, its shape lit) or a still filling the wall.
fn led_grid(src: &ImageSurface, cols: i32, rows: i32, contain: bool) -> Option<Rc<Vec<f32>>> {
    let key = (src.to_raw_none() as usize, cols, rows, contain);
    if let Some(g) = LED.with(|m| m.borrow().get(&key).cloned()) { return Some(g); }
    let mut small = ImageSurface::create(gtk::cairo::Format::ARgb32, cols, rows).ok()?;
    {
        let cr = gtk::cairo::Context::new(&small).ok()?;
        let (iw, ih) = (src.width() as f64, src.height() as f64);
        let k = if contain { (cols as f64 * 0.9 / iw).min(rows as f64 * 0.72 / ih) } else { (cols as f64 / iw).max(rows as f64 / ih) };
        cr.translate((cols as f64 - iw * k) / 2.0, (rows as f64 - ih * k) / 2.0);
        cr.scale(k, k);
        cr.set_source_surface(src, 0.0, 0.0).ok()?;
        cr.source().set_filter(gtk::cairo::Filter::Good);
        cr.paint().ok()?;
    }
    small.flush();
    let stride = small.stride() as usize;
    let data = small.data().ok()?;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for j in 0..rows as usize {
        for i in 0..cols as usize {
            let p = &data[j * stride + i * 4..j * stride + i * 4 + 4];
            let a = p[3] as f32 / 255.0;
            // Cairo's pixels are premultiplied BGRA (little-endian).
            let (b, g, r) = if a > 0.0 { (p[0] as f32 / 255.0 / a, p[1] as f32 / 255.0 / a, p[2] as f32 / 255.0 / a) } else { (0.0, 0.0, 0.0) };
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            out.push(if contain { (a * 2.8).min(1.0) * (0.75 + 0.25 * lum) } else { ((lum - 0.08) * 1.25).clamp(0.0, 1.0) * a });
        }
    }
    let g = Rc::new(out);
    LED.with(|m| m.borrow_mut().insert(key, g.clone()));
    Some(g)
}

/// The 5x7 dot-matrix letters for the scoreboard (rows top to bottom, 5 bits each).
fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [14,17,17,31,17,17,17], 'B' => [30,17,17,30,17,17,30], 'C' => [14,17,16,16,16,17,14], 'D' => [30,17,17,17,17,17,30],
        'E' => [31,16,16,30,16,16,31], 'F' => [31,16,16,30,16,16,16], 'G' => [14,17,16,23,17,17,15], 'H' => [17,17,17,31,17,17,17],
        'I' => [14,4,4,4,4,4,14], 'J' => [7,2,2,2,2,18,12], 'K' => [17,18,20,24,20,18,17], 'L' => [16,16,16,16,16,16,31],
        'M' => [17,27,21,21,17,17,17], 'N' => [17,17,25,21,19,17,17], 'O' => [14,17,17,17,17,17,14], 'P' => [30,17,17,30,16,16,16],
        'Q' => [14,17,17,17,21,18,13], 'R' => [30,17,17,30,20,18,17], 'S' => [15,16,16,14,1,1,30], 'T' => [31,4,4,4,4,4,4],
        'U' => [17,17,17,17,17,17,14], 'V' => [17,17,17,17,17,10,4], 'W' => [17,17,17,21,21,21,10], 'X' => [17,17,10,4,10,17,17],
        'Y' => [17,17,17,10,4,4,4], 'Z' => [31,1,2,4,8,16,31], '0' => [14,17,19,21,25,17,14], '1' => [4,12,4,4,4,4,14],
        '2' => [14,17,1,2,4,8,31], '3' => [31,2,4,2,1,17,14], '4' => [2,6,10,18,31,2,2], '5' => [31,16,30,1,1,17,14],
        '6' => [6,8,16,30,17,17,14], '7' => [31,1,2,4,8,8,8], '8' => [14,17,17,14,17,17,14], '9' => [14,17,17,15,1,2,12],
        '.' => [0,0,0,0,0,12,12], '-' => [0,0,0,31,0,0,0], '·' => [0,0,0,12,12,0,0], '\'' => [12,4,8,0,0,0,0], '&' => [12,18,20,8,21,18,13],
        _ => [0; 7],
    }
}

/// Text in 5x7 dots, `cell` px a dot, starting at (x, y); dim dots fill the rest of `cols`.
fn dot_text(cr: &gtk::cairo::Context, text: &str, x: f64, y: f64, cols: i32, cell: f64, lit: (f64, f64, f64, f64), dim: (f64, f64, f64, f64)) {
    let mut colbits: Vec<u8> = vec![];
    for ch in text.chars() {
        let g = glyph(ch);
        for c in (0..5).rev() {
            let mut b = 0u8;
            for (r, row) in g.iter().enumerate() { if row >> c & 1 == 1 { b |= 1 << r; } }
            colbits.push(b);
        }
        colbits.push(0);
    }
    let r = cell * 0.38;
    for pass in [false, true] {
        let (cr_, g, b, a) = if pass { lit } else { dim };
        cr.set_source_rgba(cr_, g, b, a);
        cr.new_path();
        for i in 0..cols {
            let bits = colbits.get(i as usize).copied().unwrap_or(0);
            for q in 0..7 {
                if (bits >> q & 1 == 1) == pass {
                    let (px, py) = (x + i as f64 * cell + cell / 2.0, y + q as f64 * cell + cell / 2.0);
                    cr.new_sub_path();
                    cr.arc(px, py, r, 0.0, std::f64::consts::TAU);
                }
            }
        }
        let _ = cr.fill();
    }
}

/// The LED wall (films, series, games): the title's logo in bright dots, then a
/// still in white and red halftone dots, taking turns every 6 seconds; football
/// gets a dot-matrix scoreboard. A faint red light falls to the floor.
fn led_wall(cr: &gtk::cairo::Context, rect: (f64, f64, f64, f64), sign: &Sign, t: f64, floor_y: f64) {
    let (x0, y0, w, h) = rect;
    let cell = (w / 86.0).max(2.7);
    let cols = (w / cell).floor() as i32;
    let rows = (h / cell).floor() as i32;
    let (ox, oy) = (x0 + (w - cols as f64 * cell) / 2.0, y0 + (h - rows as f64 * cell) / 2.0);
    let (pw, ph) = (cols as f64 * cell, rows as f64 * cell);
    // the light on the floor
    let g = gtk::cairo::LinearGradient::new(0.0, oy + ph, 0.0, floor_y);
    g.add_color_stop_rgba(0.0, 1.0, 0.353, 0.376, 0.10);
    g.add_color_stop_rgba(1.0, 1.0, 0.353, 0.376, 0.0);
    cr.move_to(ox + pw * 0.12, oy + ph);
    cr.line_to(ox + pw * 0.88, oy + ph);
    cr.line_to(ox + pw * 1.08, floor_y);
    cr.line_to(ox - pw * 0.08, floor_y);
    cr.close_path();
    let _ = cr.set_source(&g);
    let _ = cr.fill();
    // the panel
    let m = cell * 0.7;
    cr.new_sub_path();
    let rad = 3.0;
    let (bx, by, bw, bh) = (ox - m, oy - m, pw + 2.0 * m, ph + 2.0 * m);
    cr.arc(bx + bw - rad, by + rad, rad, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(bx + bw - rad, by + bh - rad, rad, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(bx + rad, by + bh - rad, rad, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(bx + rad, by + rad, rad, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.close_path();
    cr.set_source_rgb(0.02, 0.02, 0.02);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.16);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    match sign {
        Sign::Score { home, away, sub } => {
            let line1 = format!("{home} - {away}");
            let n1 = line1.chars().count() as f64 * 6.0;
            let cs = (pw / (n1 + 2.0)).min(ph * 0.5 / 7.0);
            let line2 = sub.to_uppercase();
            let n2 = line2.chars().count() as f64 * 6.0;
            let cs2 = (cs * 0.5).max(2.0).min(pw / (n2 + 2.0));
            dot_text(cr, &line1, ox + (pw - n1 * cs) / 2.0, oy + ph * 0.16, n1 as i32, cs, (1.0, 0.886, 0.894, 1.0), (1.0, 0.231, 0.267, 0.1));
            dot_text(cr, &line2, ox + (pw - n2 * cs2) / 2.0, oy + ph * 0.66, n2 as i32, cs2, (1.0, 0.231, 0.267, 1.0), (1.0, 0.231, 0.267, 0.1));
        }
        Sign::Led { logo, pic } => {
            let srcs: Vec<(&ImageSurface, bool)> = [logo.map(|l| (l, true)), pic.map(|p| (p, false))].into_iter().flatten().collect();
            if srcs.is_empty() { return; }
            let ph_ = (t / 6.0) % srcs.len() as f64;
            let i = ph_.floor() as usize;
            let f = ph_ - i as f64;
            let a = led_grid(srcs[i].0, cols, rows, srcs[i].1);
            let nx = srcs[(i + 1) % srcs.len()];
            let b = led_grid(nx.0, cols, rows, nx.1);
            let mix = if f > 0.82 { (f - 0.82) / 0.18 } else { 0.0 };
            let colors = [(1.0, 0.231, 0.267, 0.14), (0.639, 0.078, 0.11, 1.0), (1.0, 0.231, 0.267, 1.0), (1.0, 0.886, 0.894, 1.0)];
            for (bucket, col) in colors.iter().enumerate() {
                cr.set_source_rgba(col.0, col.1, col.2, col.3);
                cr.new_path();
                for j in 0..rows {
                    for q in 0..cols {
                        let id = (j * cols + q) as usize;
                        let mut l = a.as_ref().map(|g| g[id]).unwrap_or(0.0) as f64;
                        if mix > 0.0 { if let Some(g) = &b { l = l * (1.0 - mix) + g[id] as f64 * mix; } }
                        let bk = if l < 0.1 { 0 } else if l < 0.38 { 1 } else if l < 0.7 { 2 } else { 3 };
                        if bk != bucket { continue; }
                        let r = if bk == 0 { cell * 0.16 } else { cell * 0.44 * (0.3 + 0.7 * l) };
                        cr.new_sub_path();
                        cr.arc(ox + q as f64 * cell + cell / 2.0, oy + j as f64 * cell + cell / 2.0, r, 0.0, std::f64::consts::TAU);
                    }
                }
                let _ = cr.fill();
            }
        }
        _ => {}
    }
}

/// The mini's room: the sign at the back (neon, the LED wall or the scoreboard),
/// a soft red glow as wide as the sound, the floor ring in dots, the six dot
/// speakers on it at their 5.1 angles, and you in the middle. Returns each
/// speaker's box, for tapping.
pub fn mini_room(cr: &gtk::cairo::Context, w: f64, h: f64, sc: &MiniScene) -> Vec<(&'static str, [f64; 4])> {
    use std::f64::consts::TAU;
    let has_sign = !matches!(sc.sign, Sign::None);
    let neon = matches!(sc.sign, Sign::Neon { .. });
    let cx = w * 0.5;
    let (cy, ry) = if neon { (h * 0.7, h * 0.19) } else if has_sign { (h * 0.75, h * 0.17) } else { (h * 0.56, h * 0.22) };
    let rx = w * 0.39;
    match &sc.sign {
        Sign::Neon { logo, text } => crate::themes::neon_sign(cr, *logo, text, sc.playing, sc.t, cx, h * 0.135, w * 0.66, h * 0.25, true),
        Sign::Led { .. } | Sign::Score { .. } => led_wall(cr, (cx - w * 0.31, h * 0.03, w * 0.62, h * 0.34), &sc.sign, sc.t, cy - ry * 0.4),
        Sign::None => {}
    }
    // the sound: a soft glow in the tone's colour, wider for a hall or a stadium, forward when only the fronts play
    let r_ = rx * (0.5 + 0.65 * sc.spread) * if sc.headphones { 0.3 } else { 1.0 };
    let gy = cy - if !sc.headphones && !sc.around { ry * 0.45 } else { 0.0 };
    let (tr, tg, tb) = sc.tone_rgb;
    cr.save().ok();
    cr.translate(cx, gy);
    cr.scale(1.0, ry / rx * 1.25);
    let g = gtk::cairo::RadialGradient::new(0.0, 0.0, 0.0, 0.0, 0.0, r_);
    g.add_color_stop_rgba(0.0, tr, tg, tb, 0.42);
    g.add_color_stop_rgba(0.6, tr, tg, tb, 0.14);
    g.add_color_stop_rgba(1.0, tr, tg, tb, 0.0);
    cr.arc(0.0, 0.0, r_, 0.0, TAU);
    let _ = cr.set_source(&g);
    let _ = cr.fill();
    cr.restore().ok();
    // the floor ring, in dots
    cr.save().ok();
    cr.translate(cx, cy);
    cr.scale(rx, ry);
    cr.arc(0.0, 0.0, 1.0, 0.0, TAU);
    cr.restore().ok();
    cr.set_dash(&[0.0, 7.0], 0.0);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_width(2.4);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
    // you
    for (sx, sy, fill) in [(w * 0.075, h * 0.03, false), (w * 0.044, h * 0.013, true)] {
        cr.save().ok();
        cr.translate(cx, cy);
        cr.scale(sx, sy);
        cr.arc(0.0, 0.0, 1.0, 0.0, TAU);
        cr.restore().ok();
        if fill { cr.set_source_rgb(1.0, 1.0, 1.0); let _ = cr.fill(); } else { cr.set_source_rgb(0.89, 0.149, 0.184); cr.set_line_width(2.5); let _ = cr.stroke(); }
    }
    cr.select_font_face("Archivo Narrow", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    cr.set_font_size(10.0);
    let you = if sc.headphones { "YOU · HEADPHONES" } else { "YOU" };
    if let Ok(e) = cr.text_extents(you) {
        cr.move_to(cx - e.width() / 2.0 - e.x_bearing(), cy + h * 0.075);
        let _ = cr.show_text(you);
    }
    // the speakers on the ring, back to front
    let ang = |c: &str| match c { "FL" => -30.0, "FC" => 0.0, "FR" => 30.0, "LFE" => 58.0, "RL" => -115.0, _ => 115.0f64 };
    let mut order: Vec<usize> = (0..6).collect();
    order.sort_by(|a, b| (ang(crate::room::SPEAKERS[*b].0).to_radians().cos()).total_cmp(&ang(crate::room::SPEAKERS[*a].0).to_radians().cos()));
    let k1 = w / 440.0;
    let cell = (w * 0.0084).max(3.0);
    let mut hits = vec![];
    for i in order {
        let c = crate::room::SPEAKERS[i].0;
        let a = ang(c).to_radians();
        let rr = if c == "LFE" { 0.86 } else { 1.0 };
        let (x, y) = (cx + rx * rr * a.sin(), cy - ry * rr * a.cos());
        let q = (y - (cy - ry)) / (2.0 * ry);
        let s = 0.8 + 0.45 * q;
        let (bw, bh) = match c { "FC" => (50.0, 24.0), "LFE" => (40.0, 36.0), _ => (30.0, 44.0) };
        let (bw, bh) = (bw * s * k1, bh * s * k1);
        let yy = y - bh / 2.0;
        dot_box(cr, x, yy, bw, bh, cell * s, sc.plays[i], sc.levels[i] as f64, sc.lit == Some(c));
        hits.push((c, [x - bw / 2.0 - 6.0, yy - bh / 2.0 - 6.0, bw + 12.0, bh + 12.0]));
    }
    hits
}

pub struct Shoulder {
    pub root: gtk::Overlay,
    photo: Pic,
    kick: gtk::Label,
    title: gtk::Label,
    artist: gtk::Label,
    big: gtk::Box,
    big_n: gtk::Label,
    big_s: gtk::Label,
    venue: gtk::Box,
    venue_n: gtk::Label,
    venue_d: gtk::Label,
    pub room: gtk::DrawingArea,
    rows: gtk::Grid,
    cro: gtk::Label,
    pub pp: gtk::Button,
    pub hp: gtk::ToggleButton,
    pub menu: gtk::Button,
    sheet: Rc<Sheet>,
    fields: RefCell<Vec<Field>>,
    size: Cell<(i32, i32)>,
    send: Send,
}

impl Shoulder {
    pub fn new(vol: &gtk::Adjustment) -> Rc<Shoulder> {
        let root = gtk::Overlay::builder().css_classes(["shoulder"]).hexpand(true).vexpand(true).build();
        root.set_overflow(gtk::Overflow::Hidden);
        let photo = Pic::new(true);
        photo.area.set_valign(gtk::Align::Start);
        photo.area.add_css_class("sh-photo");
        let base = gtk::Box::builder().hexpand(true).vexpand(true).build();
        root.set_child(Some(&base));
        root.add_overlay(&photo.area);
        root.add_overlay(&gtk::Box::builder().css_classes(["sh-shade"]).can_target(false).build());
        let room = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        room.set_tooltip_text(Some("Tap a speaker to test it"));
        root.add_overlay(&room);
        // the title block, top left
        let kick = lbl("", &["sh-kick"]);
        // One line (a wrapped label asks for more height than it shows); long titles get smaller type.
        let title = lbl("", &["sh-title"]);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let artist = lbl("", &["sh-artist"]);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let head = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(3).halign(gtk::Align::Start).valign(gtk::Align::Start)
            .margin_start(20).margin_top(20).margin_end(120).build();
        head.append(&kick);
        head.append(&title);
        head.append(&artist);
        root.add_overlay(&head);
        // the corner: the big 5.1, or the live venue
        let big_n = lbl("", &["sh-big"]);
        let big_s = gtk::Label::builder().use_markup(true).css_classes(["sh-bigs"]).valign(gtk::Align::End).margin_bottom(4).build();
        let big = gtk::Box::builder().spacing(6).halign(gtk::Align::End).valign(gtk::Align::Start).margin_end(16).margin_top(16).visible(false).build();
        big.append(&big_n);
        big.append(&big_s);
        root.add_overlay(&big);
        let venue_n = lbl("", &["sh-vn"]);
        let venue_d = lbl("", &["sh-vd"]);
        venue_n.set_halign(gtk::Align::End);
        venue_d.set_halign(gtk::Align::End);
        let live = lbl("LIVE", &["sh-live"]);
        live.set_halign(gtk::Align::End);
        let venue = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).halign(gtk::Align::End).valign(gtk::Align::Start)
            .margin_end(14).margin_top(16).visible(false).build();
        venue.append(&live);
        venue.append(&venue_n);
        venue.append(&venue_d);
        root.add_overlay(&venue);
        // the four choices as the main page's rows, then the controls
        let rows = gtk::Grid::builder().column_spacing(22).halign(gtk::Align::Fill).valign(gtk::Align::End).margin_start(18).margin_end(18)
            .margin_bottom(66).build();
        root.add_overlay(&rows);
        let cro = lbl("", &["r-cro"]);
        cro.set_valign(gtk::Align::Center);
        let pp = icon_btn("media-playback-pause-symbolic", "Play / pause", "pp");
        let volume = gtk::Scale::builder().adjustment(vol).orientation(gtk::Orientation::Horizontal).draw_value(false)
            .hexpand(true).valign(gtk::Align::Center).tooltip_text("Volume").build();
        let hp = gtk::ToggleButton::builder().icon_name("audio-headphones-symbolic").tooltip_text("Headphones")
            .css_classes(["round"]).valign(gtk::Align::Center).build();
        // ☰ opens the control centre (not a menu).
        let menu = gtk::Button::builder().icon_name("open-menu-symbolic").tooltip_text("Sound, Mixer, Microphone, Speakers, Look and feel, Settings").css_classes(["round"])
            .valign(gtk::Align::Center).build();
        let dock = gtk::Box::builder().spacing(8).css_classes(["r-ctl"]).valign(gtk::Align::End).margin_start(14).margin_end(14).margin_bottom(14).build();
        for wd in [cro.upcast_ref::<gtk::Widget>(), pp.upcast_ref(), volume.upcast_ref(), hp.upcast_ref(), menu.upcast_ref()] {
            dock.append(wd);
        }
        root.add_overlay(&dock);
        let sheet = Sheet::new(&root);
        let m = Rc::new(Shoulder { root, photo, kick, title, artist, big, big_n, big_s, venue, venue_n, venue_d, room, rows, cro, pp, hp, menu,
            sheet, fields: RefCell::new(vec![]), size: Cell::new((440, 720)), send: RefCell::new(None) });
        {
            let wk = Rc::downgrade(&m);
            m.pp.connect_clicked(move |_| if let Some(m) = wk.upgrade() { m.emit("act:pp") });
            let wk = Rc::downgrade(&m);
            m.hp.connect_clicked(move |_| if let Some(m) = wk.upgrade() { m.emit("act:hp") });
            let wk = Rc::downgrade(&m);
            m.menu.connect_clicked(move |_| if let Some(m) = wk.upgrade() { m.emit("act:menu") });
        }
        m
    }

    pub fn set_send(&self, f: Rc<dyn Fn(&str)>) {
        self.sheet.set_send(f.clone());
        *self.send.borrow_mut() = Some(f);
    }

    fn emit(&self, a: &str) {
        let f = self.send.borrow().clone();
        if let Some(f) = f { f(a) }
    }

    /// The window's size: the photo takes the top two thirds (it fades out),
    /// the room sits between the title block and the rows; a wide window puts
    /// the rows two to a line.
    pub fn resized(self: &Rc<Self>, w: i32, h: i32) {
        let old = self.size.replace((w, h));
        self.photo.area.set_size_request(-1, (h as f64 * 0.66) as i32);
        let wide = w >= 640;
        let rows_h = if wide { 80 } else { 150 };
        self.room.set_margin_top(if h < 640 { 104 } else { 132 });
        self.room.set_margin_bottom(66 + rows_h);
        if (old.0 >= 640) != wide { self.lay_rows() }
    }

    pub fn set_photo(&self, tex: Option<&gdk::Texture>) {
        self.photo.set(tex.and_then(surface_of));
    }

    pub fn set_head(&self, h: &Head) {
        self.kick.set_label(&h.kick.to_uppercase());
        self.title.set_label(&h.title);
        if h.title.chars().count() > 15 { self.title.add_css_class("long") } else { self.title.remove_css_class("long") }
        self.artist.set_label(&h.subtitle);
        self.artist.set_visible(!h.subtitle.is_empty());
    }

    /// The corner: the big speaker count (films, games, sport) or the live venue.
    pub fn set_corner(&self, c: &crate::stage::Corner) {
        use crate::stage::Corner;
        self.big.set_visible(matches!(c, Corner::Big(..)));
        self.venue.set_visible(matches!(c, Corner::Venue(..)));
        match c {
            Corner::Big(out, on, all) => {
                self.big_n.set_label(if out == "HEADPHONES" { "HP" } else { out });
                self.big_s.set_markup(&format!("<span font_family='Anton' size='16pt' foreground='#ff4b53'>{on}</span>\n/ {all}\nSPEAKERS"));
            }
            Corner::Venue(n, d) => {
                self.venue_n.set_label(n);
                self.venue_d.set_label(&d.to_uppercase());
                self.venue_d.set_visible(!d.is_empty());
            }
            Corner::None => {}
        }
    }

    pub fn set_readout(&self, read: &str) {
        self.cro.set_label(read);
    }

    pub fn set_fields(self: &Rc<Self>, fields: &[Field]) {
        if *self.fields.borrow() == fields { return; }
        *self.fields.borrow_mut() = fields.to_vec();
        self.lay_rows();
        self.sheet.follow(fields);
    }

    fn lay_rows(self: &Rc<Self>) {
        while let Some(c) = self.rows.first_child() { self.rows.remove(&c); }
        let wide = self.size.get().0 >= 640;
        for (i, f) in self.fields.borrow().iter().enumerate() {
            let row = gtk::Box::builder().spacing(12).css_classes(if i == 0 || (wide && i == 1) { vec!["sh-row", "first"] } else { vec!["sh-row"] })
                .hexpand(true).build();
            let l = lbl(&f.label.to_uppercase(), &["sh-l"]);
            l.set_width_chars(10);
            l.set_valign(gtk::Align::Center);
            row.append(&l);
            let fixed = f.options.is_empty();
            let v = gtk::Label::builder().label(&f.value).css_classes(if f.chip { vec!["sh-chip-l"] } else { vec!["sh-v-l"] }).build();
            v.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let inner = gtk::Box::builder().spacing(2).build();
            inner.append(&v);
            if !fixed && !f.chip { inner.append(&crate::stage::chevron(&v)); }
            let b = gtk::Button::builder().child(&inner).css_classes(if f.chip { vec!["sh-chip"] } else if fixed { vec!["sh-v", "fixed"] } else { vec!["sh-v"] })
                .halign(gtk::Align::Start).valign(gtk::Align::Center).tooltip_text(format!("{}: {}", f.label, f.value)).build();
            b.set_can_target(!fixed);
            let fc = f.clone();
            let wk = Rc::downgrade(self);
            b.connect_clicked(move |_| if let Some(m) = wk.upgrade() { m.sheet.open(&fc) });
            row.append(&b);
            let (col, r) = if wide { ((i % 2) as i32, (i / 2) as i32) } else { (0, i as i32) };
            self.rows.attach(&row, col, r, 1, 1);
        }
    }

    /// The four rows' values, for the self-test.
    pub fn row_values(&self) -> Vec<String> {
        self.fields.borrow().iter().map(|f| f.value.clone()).collect()
    }

    pub fn sheet_open(&self) -> bool {
        self.sheet.is_open()
    }

    pub fn press_row(self: &Rc<Self>, key: &str) {
        if let Some(f) = self.fields.borrow().iter().find(|f| f.key == key).cloned() { self.sheet.open(&f) }
    }

    pub fn press_option(&self, id: &str) {
        self.sheet.press(id);
    }
}
