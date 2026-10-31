//! The page's logic: what kind of thing is playing, the words and pictures
//! for it, the four sound choices (and what each one changes), the room
//! drawing, the controls, and the Look and feel panel. The widgets are in
//! stage.rs; this fills them from the settings and what's playing.

use super::*;
use crate::art::{self, Look, Pixels};
use crate::meta::{self, Kind, Meta};
use crate::oys;
use crate::stage::{Corner, Field, Head, Layout};
use gtk::gdk;

/// Pictures prepared for the page (textures for widgets, surfaces for the room).
#[derive(Default)]
pub struct Tex {
    pub key: String,
    pub bg: Vec<gdk::Texture>,
    pub split: Option<gdk::Texture>,
    pub neon: Option<gtk::cairo::ImageSurface>,
    pub records: Vec<gtk::cairo::ImageSurface>,
    pub pics: Vec<gtk::cairo::ImageSurface>,
    pub logo_s: Option<gtk::cairo::ImageSurface>,
    pub cover_s: Option<gtk::cairo::ImageSurface>,
}

/// Prepared on a helper thread (plain pixels can cross threads).
pub struct Prepared {
    key: String,
    bg: Vec<Pixels>,
    cover: Option<Pixels>,
    logo: Option<Pixels>,
    neon: Option<Pixels>,
    split: Option<Pixels>,
    records: Vec<Pixels>,
    pics: Vec<Pixels>,
}

/// The game monitor: the logo first, then the trailer (muted, once), then pictures.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Mon {
    #[default]
    Logo,
    Trailer,
    Pictures,
}

#[derive(Default)]
pub struct PageState {
    /// The full-screen speaker test (speaker_test.rs): its overlay, its parts
    /// (top, room, foot, title, Walk round), where its speakers can be tapped,
    /// the walk's step and run, and whether music plays again when it closes.
    pub test_pg: RefCell<Option<gtk::Overlay>>,
    pub test_parts: RefCell<Option<(gtk::Box, gtk::DrawingArea, gtk::Box, gtk::Label, gtk::Button)>>,
    pub test_hits: RefCell<Vec<(&'static str, [f64; 4])>>,
    pub test_walk: Cell<Option<usize>>,
    pub test_gen: Cell<u32>,
    pub test_resume: Cell<bool>,
    /// Where the mini's speakers were last drawn (for tapping).
    pub mini_hits: RefCell<Vec<(&'static str, [f64; 4])>>,
    pub meta: RefCell<Option<(String, Meta)>>,
    pub asked: RefCell<String>,
    pub tex: RefCell<Tex>,
    pub smooth: RefCell<oys::Smooth>,
    pub layout: RefCell<oys::Layout>,
    pub last_frame: Cell<i64>,
    pub mon: Cell<Mon>,
    pub mon_since: Cell<Option<Instant>>,
    pub mon_i: Cell<usize>,
    pub mon_key: RefCell<String>,
    pub media: RefCell<Option<gtk::MediaFile>>,
    pub auto_compact: Cell<bool>,
    pub full: RefCell<Option<gtk::Overlay>>,
    /// The window was already full screen before the full-screen view opened.
    pub was_fullscreen: Cell<bool>,
    /// Where the pointer is over the room (for the monitor's buttons).
    pub pointer: Cell<Option<(f64, f64)>>,
    /// The monitor trailer's sound is on (this trailer only).
    pub mon_sound: Cell<bool>,
    /// The trailer has played on the monitor once for this game.
    pub trailer_done: Cell<bool>,
    /// The Music profile before a live recording picked the concert hall or
    /// the stadium by itself (put back for the next studio song).
    pub before_live: RefCell<Option<String>>,
    /// The compact view's small speaker plan: x, y, side (for tapping a speaker).
    pub mini: Cell<Option<(f64, f64, f64)>>,
}

/// The trailer's local file: a file path as is, a stream once it's saved.
pub fn trailer_file(url: &str) -> Option<std::path::PathBuf> {
    if !url.contains("://") {
        return Some(std::path::PathBuf::from(url)).filter(|p| p.exists());
    }
    let p = trailer_cache(url);
    p.exists().then_some(p)
}

fn trailer_cache(url: &str) -> std::path::PathBuf {
    let h = url.split('?').next().unwrap_or(url).bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    crate::system::cache_dir().join("trailers").join(format!("{h:016x}.mp4"))
}

/// Saves a trailer stream (HLS) to the cache with ffmpeg, up to 1080p. Slow:
/// call on a helper thread. Old trailers beyond the newest 12 are removed.
fn save_trailer(url: &str) -> Option<std::path::PathBuf> {
    let dest = trailer_cache(url);
    let dir = dest.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    let part = dest.with_extension("part.mp4");
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i", url, "-map", "0:v:0", "-map", "0:a:0?", "-c", "copy", "-movflags", "+faststart"])
        .arg(&part)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&part);
        return None;
    }
    std::fs::rename(&part, &dest).ok()?;
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut v: Vec<_> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
        v.sort();
        while v.len() > 12 {
            let _ = std::fs::remove_file(v.remove(0).1);
        }
    }
    Some(dest)
}

/// The monitor's buttons, worked out from its rectangle: (id, x, y, w, h).
fn mon_buttons(r: (f64, f64, f64, f64), trailer: bool, playing: bool) -> Vec<(&'static str, f64, f64, f64, f64)> {
    let (x, y, w, h) = r;
    let mut v = vec![("prev", x, y + h / 2.0 - 20.0, 26.0, 40.0), ("next", x + w - 26.0, y + h / 2.0 - 20.0, 26.0, 40.0),
                     ("full", x + w - 32.0, y + 6.0, 26.0, 26.0)];
    if trailer {
        v.push(("play", x + w / 2.0 - 23.0, y + h / 2.0 - 23.0, 46.0, 46.0));
    }
    if playing {
        v.push(("sound", x + 6.0, y + h - 32.0, 26.0, 26.0));
    }
    v
}

/// The two teams in a match title ("Harbour City v Northgate United",
/// "Arsenal vs. Chelsea | Premier League"), as three-letter short names for
/// the stadium's scoreboard.
fn teams(title: &str) -> Option<(String, String)> {
    let low = title.to_lowercase();
    let (i, sep) = [" vs. ", " vs ", " v "].iter().find_map(|sep| low.find(sep).map(|i| (i, sep.len())))?;
    let cut = |t: &str| -> String {
        let t = t.split(['|', '(', ':']).next().unwrap_or(t);
        let t = t.split(" - ").next().unwrap_or(t);
        t.split_whitespace().next().unwrap_or("").chars().filter(|c| c.is_alphanumeric()).take(3).collect::<String>().to_uppercase()
    };
    let home = cut(title[..i].rsplit(['|', ':']).next().unwrap_or(""));
    let away = cut(&title[i + sep..]);
    (!home.is_empty() && !away.is_empty()).then_some((home, away))
}

/// Pages whose room has a screen that shows the title, then pictures (and the
/// trailer: by itself for games, one press away for films and series).
/// The Remote's room shows the speakers in a row once it is over 3.2 times as wide as tall.
fn remote_row(w: f64, h: f64) -> bool {
    h > 0.0 && w / h > 3.2
}

fn has_monitor(k: Option<&Kind>) -> bool {
    matches!(k, Some(Kind::Game | Kind::Film | Kind::Tv))
}

/// The kind of page for a kind of content (for layouts and labels).
pub fn page_kind(k: &Kind) -> &'static str {
    meta::kind_id(k)
}

fn kind_word(k: &Kind) -> &'static str {
    match k {
        Kind::Studio => "Studio",
        Kind::LiveSmall | Kind::LiveStadium => "Live",
        Kind::Film => "Film",
        Kind::Tv => "TV",
        Kind::Game => "Game",
        Kind::Sport => "Sport",
        Kind::Video => "A video",
        Kind::Stream => "Live stream",
    }
}

/// The phrase for an SBX profile on a kind of page.
fn style_phrase(style: &str, k: &Kind) -> (String, String) {
    let p = |a: &str, b: &str| (a.to_string(), b.to_string());
    match (style, k) {
        ("Music", _) => p("close, like the studio", "SBX Music"),
        ("Concert", Kind::Studio) => p("like a live show", "SBX Concert"),
        ("Concert", _) => p("like the concert hall", "SBX Concert"),
        ("Stadium", Kind::Sport) => p("in the stands", "SBX Stadium"),
        ("Stadium", _) => p("like a stadium", "SBX Stadium"),
        ("Arena", _) => p("like a stadium", "SBX Arena"),
        ("Cinema", Kind::Sport) => p("like a big screen", "SBX Cinema"),
        ("Cinema", _) => p("like a cinema", "SBX Cinema"),
        ("Night", _) => p("quietly, for late at night", "SBX Night"),
        ("Gaming", _) => p("wide, to place every sound", "SBX Gaming"),
        ("FPS", _) => p("tight, to hear every footstep", "SBX FPS"),
        ("Racing", _) => p("fast, for racing", "SBX Racing"),
        ("RPG", _) => p("rich, for role-playing", "SBX RPG"),
        ("Strategy", _) => p("clear, for strategy", "SBX Strategy"),
        ("Adventure", _) => p("big, for adventures", "SBX Adventure"),
        (s, _) if model::is_game_profile(s) => (format!("made for {s}"), "Creative's game profile".into()),
        (s, _) => (s.to_string(), "your own profile".into()),
    }
}

const TONE_WORDS: [&str; 5] = ["much warmer", "a little warmer", "as mixed", "a little brighter", "much brighter"];

/// The first four-digit year (1900–2099) in some text.
fn year_in(t: &str) -> Option<String> {
    t.split(|c: char| !c.is_ascii_digit()).find(|w| w.len() == 4 && (w.starts_with("19") || w.starts_with("20"))).map(str::to_string)
}

fn mmss(s: f64) -> String {
    let s = s.max(0.0) as i64;
    if s >= 3600 { format!("{} h {} min", s / 3600, (s % 3600) / 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

impl Ui {
    /// What's playing as a kind of content (meta when known, else a guess).
    pub fn kind_now(&self) -> Option<Kind> {
        let np = self.np.borrow().clone()?;
        if let Some((k, m)) = self.page.meta.borrow().as_ref()
            && *k == np.key() && let Some(kind) = &m.kind {
                return Some(kind.clone());
            }
        Some(meta::classify(&np))
    }

    /// The layout this page uses, and whether the window shows the compact view.
    pub(crate) fn layout_now(&self) -> (Layout, bool) {
        let s = self.s.borrow();
        let kind = self.kind_now().map(|k| page_kind(&k)).unwrap_or("music");
        let compact = s.compact_always || self.page.auto_compact.get();
        (Layout::from_id(&s.layout_for(kind)), compact)
    }

    pub fn connect_page(self: &Rc<Self>) {
        let st = self.stage.clone();
        {
            let wk = Rc::downgrade(self);
            *st.on_pick.borrow_mut() = Some(Rc::new(move |a: &str| if let Some(u) = wk.upgrade() { u.page_pick(a) }));
        }
        {
            let wk = Rc::downgrade(self);
            *st.on_action.borrow_mut() = Some(Rc::new(move |a: &str| if let Some(u) = wk.upgrade() { u.page_action(a) }));
        }
        // The ☰ menus: the page's, the Remote's and the Strip's.
        // ☰ opens the control centre; the Strip, too thin for it, has a short menu of its own.
        st.strip.menu.set_popover(Some(&self.menu_popover(true)));
        self.connect_minis();
        // The signal path card moves from the old capsule to the readout's popover.
        {
            if let Some(old) = self.path_box.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>() {
                old.set_child(None::<&gtk::Widget>);
            }
            let pop = gtk::Popover::builder().child(&self.path_box).position(gtk::PositionType::Top).build();
            st.readout_btn.set_popover(Some(&pop));
        }
        // The room: drawn by the theme, animated while sound plays or things fade.
        {
            let wk = Rc::downgrade(self);
            st.room.set_draw_func(move |_, cr, w, h| {
                if let Some(u) = wk.upgrade() {
                    u.draw_room(cr, w as f64, h as f64);
                }
            });
            let wk = Rc::downgrade(self);
            st.room.add_tick_callback(move |da, clock| {
                let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
                let t = clock.frame_time();
                let busy = u.page.layout.borrow().animating || u.levels.get().iter().any(|l| *l > 0.02)
                    || u.np.borrow().as_ref().is_some_and(|n| n.playing);
                if busy && t - u.page.last_frame.get() > 33_000 {
                    u.page.last_frame.set(t);
                    da.queue_draw();
                }
                glib::ControlFlow::Continue
            });
            let motion = gtk::EventControllerMotion::new();
            let wk = Rc::downgrade(self);
            motion.connect_motion(move |_, x, y| if let Some(u) = wk.upgrade() {
                let was_in = u.over_monitor(u.page.pointer.get());
                u.page.pointer.set(Some((x, y)));
                if was_in != u.over_monitor(Some((x, y))) { u.stage.room.queue_draw() }
            });
            let wk = Rc::downgrade(self);
            motion.connect_leave(move |_| if let Some(u) = wk.upgrade() {
                u.page.pointer.set(None);
                u.stage.room.queue_draw();
            });
            st.room.add_controller(motion);
            let wk = Rc::downgrade(self);
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, x, y| {
                let Some(u) = wk.upgrade() else { return };
                if let Some(id) = u.monitor_button_at(x, y) {
                    u.monitor_press(id);
                    return;
                }
                // The compact view's small plan.
                if u.layout_now().1 && let Some((mx, my, side)) = u.page.mini.get() {
                    let mut st = u.room_state();
                    st.top = 6.0;
                    st.bottom = 24.0;
                    if let Some(ch) = room::hit(side, side, &st, x - mx, y - my) {
                        u.test_speaker(ch);
                    }
                    return;
                }
                let lay = u.page.layout.borrow().clone();
                if let Some((ch, _)) = lay.speakers.iter().find(|(_, (sx, sy, sw, sh))| x >= *sx && x <= sx + sw && y >= *sy && y <= sy + sh) {
                    if let Some(i) = oys::ORDER.iter().position(|o| o == ch) {
                        u.page.smooth.borrow_mut().ping(i, u.t0.elapsed().as_secs_f64(), 1.6);
                    }
                    u.test_speaker(ch);
                    return;
                }
                if let Some((sx, sy, sw, sh)) = lay.screen_rect
                    && x >= sx && x <= sx + sw && y >= sy && y <= sy + sh
                    && has_monitor(u.kind_now().as_ref()) {
                        u.open_full(false);
                    }
            });
            st.room.add_controller(click);
        }
        // Monitor pictures change every 10 seconds.
        {
            let wk = Rc::downgrade(self);
            glib::timeout_add_local(Duration::from_secs(10), move || {
                let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
                if u.page.mon.get() == Mon::Pictures {
                    u.page.mon_i.set(u.page.mon_i.get() + 1);
                    u.stage.room.queue_draw();
                }
                glib::ControlFlow::Continue
            });
        }
    }

    // ---- filling the page

    pub fn update_stage(&self) {
        let np = self.np.borrow().clone();
        let s = self.s.borrow().clone();
        if let Some(n) = &np {
            self.ask_meta(n);
        }
        let kind = self.kind_now().unwrap_or(match s.activity.as_str() { "game" => Kind::Game, "movie" => Kind::Film, _ => Kind::Studio });
        let m = self.page.meta.borrow().as_ref().filter(|(k, _)| np.as_ref().is_some_and(|n| n.key() == *k)).map(|x| x.1.clone());
        let (layout, compact) = self.layout_now();
        self.stage.set_layout(layout, compact);
        let tex = self.page.tex.borrow();
        let fresh = np.as_ref().is_some_and(|n| n.key() == tex.key);

        // Title block
        let mut head = Head::default();
        let stream_ch = np.as_ref().and_then(|n| n.stream.as_ref()).map(|f| f.channels).unwrap_or(2);
        let fmt = if stream_ch >= 6 { "5.1" } else { "stereo" };
        match &np {
            Some(n) => {
                let app = m.as_ref().map(|m| m.kicker_app.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| n.app.clone());
                let mut kick = vec![app, kind_word(&kind).to_string()];
                if let Some(g) = m.as_ref().and_then(|m| m.credits.iter().find(|c| c.0 == "Genre").map(|c| c.1.clone())) {
                    kick.push(g.split(',').next().unwrap_or(&g).trim().to_string());
                } else {
                    kick.push(fmt.to_string());
                }
                head.kick = kick.join(" · ");
                head.title = m.as_ref().map(|m| m.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| n.title.clone());
                head.subtitle = m.as_ref().map(|m| m.subtitle.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| n.artist.clone());
                head.credits = m.as_ref().map(|m| m.credits.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| {
                    let mut c = vec![];
                    if !n.extra.is_empty() { c.push(("Track".into(), n.extra.trim_start_matches("Track ").to_string())); }
                    if !n.album.is_empty() { c.push(("Album".into(), n.album.clone())); }
                    if let Some(l) = n.length { c.push(("Length".into(), mmss(l))); }
                    c
                });
                if matches!(kind, Kind::Game | Kind::Sport | Kind::Video | Kind::Stream) {
                    head.credits.retain(|c| c.0 != "Length");
                }
                // Films, series, games and sport show "year · app" under the name
                // instead of the credits (round 101's pages).
                if !kind.is_music() {
                    let year = head.credits.iter().map(|c| c.1.as_str())
                        .chain(m.iter().flat_map(|m| m.facts.iter().map(|f| f.as_str())))
                        .find_map(year_in);
                    let app = m.as_ref().map(|m| m.kicker_app.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| n.app.clone());
                    head.line = [year.unwrap_or_default(), app].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                }
                head.bio = m.as_ref().map(|m| m.bio.clone()).unwrap_or_default();
                // The design shows the facts line on films, series and games, not on songs.
                head.facts = if kind.is_music() { String::new() } else { m.as_ref().map(|m| m.facts.join(" · ")).unwrap_or_default() };
                let has_pics = m.as_ref().is_some_and(|m| !m.pictures.is_empty() || m.trailer.is_some());
                head.button = match kind {
                    Kind::Film | Kind::Tv if m.as_ref().is_some_and(|m| m.trailer.is_some()) => Some(("Watch the trailer".into(), "full screen, with sound".into())),
                    Kind::Game | Kind::Film | Kind::Tv if has_pics && (s.theme != "oys" || compact) => Some(("Full screen".into(),
                        if m.as_ref().is_some_and(|m| m.trailer.is_some()) { "pictures, and the trailer with sound".into() } else { "pictures".into() })),
                    _ => None,
                };
            }
            None => {
                head.kick = "Nothing playing".into();
                head.title = model::activity_label(&s.activity).into();
                head.subtitle = if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "SBX off".into() };
            }
        }
        let logo = if fresh && kind == Kind::Game { tex.logo_s.clone() } else { None };
        self.stage.set_head(&head, logo.as_ref());
        // The title block's height on the page, as in the design (top: 38% for
        // songs, films and series, 33% for games, 37% for sport).
        self.stage.set_anchor(match kind { Kind::Game => 0.33, Kind::Sport => 0.37, Kind::Video | Kind::Stream => 0.36, _ => 0.38 });

        // Corner: the big number for games and films, the venue for live songs.
        let (src, out, n_on, n_all) = self.readout_parts(stream_ch);
        let read = if src == out || src.ends_with(&format!(" {out}")) { src.clone() } else { format!("{src} → {out}") };
        self.stage.readout.set_label(&read);
        let corner = match &kind {
            Kind::LiveSmall | Kind::LiveStadium => {
                let venue = m.as_ref().map(|m| m.venue.clone()).unwrap_or_default();
                if venue.is_empty() { Corner::None } else { Corner::Venue(venue, m.as_ref().map(|m| m.date.clone()).unwrap_or_default()) }
            }
            Kind::Game | Kind::Film | Kind::Tv if np.is_some() => Corner::Big(out.clone(), n_on, n_all),
            _ => Corner::None,
        };
        // Films and series: a ticket band across the top instead (not in the compact view).
        let ticket = match (&kind, np.as_ref()) {
            (Kind::Film | Kind::Tv, Some(n)) if !compact => {
                let ep: Vec<String> = head.credits.iter().filter(|c| c.0 == "Season" || c.0 == "Episode")
                    .map(|c| format!("{} {}", c.0, c.1)).collect();
                let screen = if ep.is_empty() { format!("Screen {out}") } else { ep.join(" · ") };
                let app = m.as_ref().map(|m| m.kicker_app.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| n.app.clone());
                Some(stage::Ticket {
                    left: if app.is_empty() { screen } else { format!("{app} · {screen}") },
                    source: format!("{src} sound"),
                    number: if out == "HEADPHONES" { "HP".into() } else { out.clone() },
                    seed: n.title.clone(),
                })
            }
            _ => None,
        };
        let corner = if ticket.is_some() { Corner::None } else { corner };
        self.stage.set_ticket(ticket);
        self.stage.set_corner(&corner);
        if fresh || np.is_none() {
            self.stage.set_cover(tex.cover_s.clone(), matches!(kind, Kind::Film | Kind::Tv | Kind::Game));
            self.stage.set_split(tex.split.as_ref());
        }
        drop(tex);

        // Sound choices
        let fields = self.sound_fields(&kind, stream_ch);
        let tech = self.tech_line();
        self.stage.set_fields(fields, &tech);
        self.stage.hp.set_active(s.headphones());
        let mpris = np.as_ref().is_some_and(|n| n.activity != "game" && n.length.is_some());
        self.stage.prev.set_sensitive(mpris);
        self.stage.next.set_sensitive(mpris);
        self.stage.pp.set_sensitive(np.as_ref().is_some_and(|n| n.activity != "game"));
        self.stage.pp.set_icon_name(if np.as_ref().is_some_and(|n| n.playing) { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
        self.stage.hint.set_visible(!compact && !self.stage.is_half());
        self.stage.room.queue_draw();
        self.stage.mirror();
        if self.panel_open() { self.fill_panel_head() }
        self.stage.remote.set_top(s.compact_top == "album");
        self.stage.strip.pin.set_active(s.strip_pin);
        self.stage.remote.room.queue_draw();
        self.stage.strip.spk.queue_draw();
    }

    /// (what comes in, what plays, speakers playing, speakers there).
    fn readout_parts(&self, ch: usize) -> (String, String, usize, usize) {
        let s = self.s.borrow();
        let all = model::channels(&s.speaker_config).len();
        let src = if ch >= 6 { "5.1".to_string() } else { "STEREO".to_string() };
        if s.headphones() {
            return (src, "HEADPHONES".into(), 0, all);
        }
        let has_rear = model::channels(&s.speaker_config).contains(&"RL");
        let has_sub = s.has_sub();
        let out = if ch >= 6 { s.speaker_config.clone() } else if has_rear && s.spreads() {
            if has_sub { "4.1".into() } else { "4.0".into() }
        } else if has_sub { "2.1".into() } else { "2.0".into() };
        let n_on = if ch >= 6 { all } else { out.split('.').map(|p| p.parse::<usize>().unwrap_or(0)).sum() };
        (src, out, n_on.min(all), all)
    }

    pub(crate) fn tech_line(&self) -> String {
        let s = self.s.borrow();
        let mut v = vec![if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "SBX off".into() }];
        if s.eq_enabled {
            v.push(format!("{} EQ{}", s.cur_eq(), match s.tone { t if t < 0 => " warmer", t if t > 0 => " brighter", _ => "" }));
        }
        if s.scout && s.sbx_active() { v.push("Scout".into()); }
        v.join(" · ")
    }

    /// Where / Room / Tone / Extras, worded for the kind of page.
    pub fn sound_fields(&self, kind: &Kind, ch: usize) -> Vec<Field> {
        let s = self.s.borrow();
        let (lw, lr, lt, lx) = match kind {
            Kind::Film | Kind::Tv => ("Seat", "Venue", "Mix", "Extras"),
            Kind::Game => ("Speakers", "Mode", "Tone", "Extras"),
            Kind::Sport => ("Crowd", "Atmosphere", "Tone", "Commentary"),
            Kind::LiveSmall | Kind::LiveStadium => ("Where", "Venue", "Tone", "Extras"),
            _ => ("Where", "Room", "Tone", "Extras"),
        };
        let mut out = vec![];
        // Where
        let has_rear = model::channels(&s.speaker_config).contains(&"RL");
        let spk_opt = ("hp".to_string(), "on headphones".to_string(), "switch the sound to the headphone socket".to_string());
        let mut f = Field { key: "where", label: lw.into(), ..Default::default() };
        if s.headphones() {
            f.value = "on headphones".into();
            f.current = "hp".into();
            f.options = vec![("hp".into(), "on headphones".into(), String::new()),
                             ("speakers".into(), "on the speakers".into(), "switch back to your speakers".into())];
        } else if ch >= 6 {
            let n = model::channels(&s.speaker_config).len();
            f.value = if n >= 6 { "all six speakers".into() } else { format!("all {n} speakers") };
            // Not a choice in the design: 5.1 plays on every speaker (headphones are the dock's button).
            f.current = "all".into();
            let _ = spk_opt;
        } else if has_rear {
            let around = s.spreads();
            f.current = if around { "rear" } else { "front" }.into();
            f.value = if around { "all around you" } else { "in front of you" }.into();
            f.options = vec![("front".into(), "in front of you".into(), "stereo on the front speakers, as mixed".into()),
                             ("rear".into(), "all around you".into(), "front and back speakers".into()), spk_opt];
        } else {
            f.value = "in front of you".into();
            f.current = "front".into();
            f.options = vec![("front".into(), "in front of you".into(), String::new()), spk_opt];
        }
        out.push(f);
        // Room / Venue / Mode: the SBX profile
        let act = match kind { Kind::Game => "game", Kind::Studio | Kind::LiveSmall | Kind::LiveStadium => "music", _ => "movie" };
        let mut styles: Vec<String> = match kind {
            // (a studio song keeps the studio room whatever the sound; Arena just makes it bigger)
            Kind::Studio => vec!["Music".into(), "Concert".into(), "Arena".into()],
            Kind::LiveSmall => vec!["Concert".into(), "Arena".into(), "Music".into()],
            Kind::LiveStadium => vec!["Arena".into(), "Concert".into(), "Music".into()],
            Kind::Sport => vec!["Stadium".into(), "Cinema".into(), "Night".into()],
            _ => model::styles_for(act).iter().map(|x| x.to_string()).collect(),
        };
        let cur = s.cur_profile();
        if !styles.contains(&cur) && s.sbx_active() {
            styles.insert(0, cur.clone());
        }
        for own in s.fx_profiles.keys() {
            if !styles.contains(own) { styles.push(own.clone()); }
        }
        let mut f = Field { key: "room", label: lr.into(), ..Default::default() };
        for st in &styles {
            let (p, sub) = style_phrase(st, kind);
            f.options.push((st.clone(), p, sub));
        }
        f.options.push(("off".into(), "untouched".into(), "SBX off: as it was made".into()));
        f.current = if s.sbx_active() { cur.clone() } else { "off".into() };
        f.value = if s.sbx_active() { style_phrase(&cur, kind).0 } else { "untouched".into() };
        out.push(f);
        // Tone
        let t = s.tone.clamp(-2, 2);
        let mut f = Field { key: "tone", label: lt.into(), current: t.to_string(), value: TONE_WORDS[(t + 2) as usize].into(), ..Default::default() };
        for v in -2i32..=2 {
            let sub = match v { 0 => format!("the {} EQ", s.cur_eq()), x if x < 0 => "more low end, softer top".into(), _ => "crisper top, less low end".into() };
            f.options.push((v.to_string(), TONE_WORDS[(v + 2) as usize].into(), sub));
        }
        out.push(f);
        // Extras
        let fx = s.fx_of(&cur);
        let dlg = s.sbx_active() && fx.dialog.0;
        let bass = s.sbx_active() && fx.bass.0;
        let scout = s.scout && s.sbx_active();
        let voice = if *kind == Kind::Sport { "clearer commentary" } else { "clearer voices" };
        let mut on = vec![];
        if dlg { on.push(voice); }
        if bass { on.push("more bass"); }
        if scout { on.push("footsteps"); }
        let mut f = Field { key: "extra", label: lx.into(), chip: true, ..Default::default() };
        f.value = if on.is_empty() { if *kind == Kind::Sport { "+ commentary or bass".into() } else { "+ voices or bass".into() } } else { on.join(" · ") };
        let tick = |b: bool| if b { "on: tap to turn off" } else { "off: tap to turn on" };
        f.options.push(("dialog".into(), voice.into(), format!("Dialog+ {}", tick(dlg))));
        f.options.push(("bass".into(), "more bass".into(), format!("SBX Bass {}", tick(bass))));
        if *kind == Kind::Game {
            f.options.push(("scout".into(), "hear footsteps sooner".into(), format!("Scout Mode {}", tick(scout))));
        }
        out.push(f);
        out
    }

    pub(crate) fn page_pick(&self, a: &str) {
        let (key, v) = a.split_once(':').unwrap_or((a, ""));
        match key {
            "where" => match v {
                "front" | "rear" => self.set_stereo_on(v),
                "hp" if !self.s.borrow().headphones() => self.ask_switch_output(),
                "speakers" if self.s.borrow().headphones() => self.ask_switch_output(),
                _ => {}
            },
            "room" => {
                if v == "off" {
                    self.set_sbx(false);
                } else {
                    if !self.s.borrow().sbx_enabled {
                        self.set_sbx(true);
                    }
                    self.set_style(v);
                }
            }
            "tone" => {
                let t = v.parse::<i32>().unwrap_or(0).clamp(-2, 2);
                {
                    let mut s = self.s.borrow_mut();
                    s.tone = t;
                    s.remember_kind();
                }
                self.changed(part::EQ);
                self.refresh();
            }
            "extra" => match v {
                "scout" => { let on = !self.s.borrow().game_footsteps; self.set_scout(on) }
                key => {
                    if !self.s.borrow().sbx_enabled {
                        self.set_sbx(true);
                    }
                    if let Some(row) = self.fx.iter().find(|r| r.key == key) {
                        row.on.set_active(!row.on.is_active());
                    }
                }
            },
            _ => {}
        }
        self.update_stage();
    }

    pub(crate) fn page_action(self: &Rc<Self>, a: &str) {
        match a {
            "prev" | "pp" | "next" => {
                let title = self.np.borrow().as_ref().map(|n| n.title.clone()).unwrap_or_default();
                let method = match a { "prev" => "Previous", "next" => "Next", _ => "PlayPause" };
                std::thread::spawn(move || mpris(method, &title));
            }
            "hp" => {
                let want = !self.s.borrow().headphones();
                self.stage.hp.set_active(!want);
                self.ask_switch_output();
            }
            "test" => self.open_speaker_test(),
            // ☰: the control centre, on the tab last shown (or closes it).
            "menu" => if self.panel_open() { self.close_panel() } else {
                let last = self.side_stack.visible_child_name().map(|n| n.to_string()).unwrap_or_else(|| "sound".into());
                self.show_page(&last);
            },
            // The Strip (☰), back to the full window, its pin, the window itself.
            "strip" => self.set_strip(true),
            "full" => self.set_strip(false),
            "pin-on" | "pin-off" => {
                let on = a == "pin-on";
                self.s.borrow_mut().strip_pin = on;
                self.save_now();
                if self.strip_on.get() || self.views.visible_child_name().as_deref() == Some("strip") {
                    crate::mini::pin_window(on, crate::APP_ID);
                }
            }
            "min" => self.win.minimize(),
            "quit" => self.win.close(),
            "fs" => self.open_full(self.kind_now() != Some(Kind::Game)),
            "resized" => {
                self.page.auto_compact.set(self.stage.small.get());
                self.update_stage();
            }
            _ => {}
        }
    }

    // ---- pictures and details

    fn ask_meta(&self, np: &NowPlaying) {
        let key = np.key();
        if *self.page.asked.borrow() == key {
            return;
        }
        *self.page.asked.borrow_mut() = key.clone();
        let np = np.clone();
        let opts = {
            let s = self.s.borrow();
            meta::Options { online: s.pictures_online, tmdb_key: s.tmdb_key.clone(), playnite: s.playnite_dir.clone() }
        };
        std::thread::spawn(move || {
            let live = std::env::var_os("SBCOMMAND_META_LIVE").is_some();
            let m = if crate::card::dry_run() && !live { demo_meta(&np) } else { meta::lookup_with(&np, &opts) };
            let prep = prepare(&key, &m, &np);
            glib::MainContext::default().invoke(move || {
                if let Some(u) = UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade())) {
                    u.apply_meta(key, m, prep);
                }
            });
        });
    }

    fn apply_meta(&self, key: String, m: Meta, p: Prepared) {
        if self.np.borrow().as_ref().map(NowPlaying::key) != Some(key.clone()) {
            return;
        }
        let tex = Tex {
            key: p.key,
            bg: p.bg.iter().map(Pixels::texture).collect(),
            split: p.split.as_ref().map(Pixels::texture),
            neon: p.neon.as_ref().and_then(Pixels::surface),
            records: p.records.iter().filter_map(Pixels::surface).collect(),
            pics: p.pics.iter().filter_map(Pixels::surface).collect(),
            logo_s: p.logo.as_ref().and_then(Pixels::surface),
            cover_s: p.cover.as_ref().and_then(Pixels::surface),
        };
        self.stage.set_backgrounds(tex.bg.clone());
        *self.page.tex.borrow_mut() = tex;
        self.follow_recording(m.kind.as_ref());
        let screen = has_monitor(m.kind.as_ref());
        *self.page.meta.borrow_mut() = Some((key.clone(), m));
        if screen && *self.page.mon_key.borrow() != key {
            *self.page.mon_key.borrow_mut() = key;
            self.start_monitor();
        } else if !screen {
            self.stop_video();
        }
        self.update_stage();
    }

    /// A live recording starts on the concert hall (a stadium gig on the
    /// stadium), like the prototype; the next studio song gets the Music
    /// profile back. Only when switching is automatic and SBX is on, and never
    /// over a profile you picked yourself.
    fn follow_recording(&self, kind: Option<&Kind>) {
        let (auto, music, sbx, cur) = {
            let s = self.s.borrow();
            (s.auto_mode == "auto", s.activity == "music", s.sbx_enabled, s.cur_profile())
        };
        if !auto || !music || !sbx {
            return;
        }
        match kind {
            Some(Kind::LiveSmall) | Some(Kind::LiveStadium) => {
                let want = if kind == Some(&Kind::LiveStadium) { "Arena" } else { "Concert" };
                let was = self.page.before_live.borrow().clone();
                if cur != want && (cur == "Music" || was.is_some()) {
                    if was.is_none() {
                        *self.page.before_live.borrow_mut() = Some(cur.clone());
                    }
                    self.set_style(want);
                }
            }
            Some(_) => {
                if let Some(prev) = self.page.before_live.borrow_mut().take()
                    && matches!(cur.as_str(), "Concert" | "Stadium" | "Arena") {
                        self.set_style(&prev);
                    }
            }
            None => {}
        }
    }

    // ---- the screen in the room (the game monitor, the cinema screen, the TV)

    /// The logo (or title) for 3 seconds; then a game's trailer plays muted,
    /// once, while films and series go straight to their stills (their
    /// trailer waits for the play button).
    fn start_monitor(&self) {
        self.stop_video();
        self.page.trailer_done.set(false);
        self.page.mon.set(Mon::Logo);
        self.page.mon_since.set(Some(Instant::now()));
        self.page.mon_i.set(0);
        let wk = UI.with(|c| c.borrow().clone());
        glib::timeout_add_local_once(Duration::from_secs(3), move || {
            if let Some(u) = wk.and_then(|w| w.upgrade()) && u.page.mon.get() == Mon::Logo {
                if u.kind_now() == Some(Kind::Game) {
                    u.play_trailer_on_monitor();
                } else {
                    u.page.mon.set(Mon::Pictures);
                    u.stage.room.queue_draw();
                }
            }
        });
    }

    fn play_trailer_on_monitor(&self) {
        let url = self.page.meta.borrow().as_ref().and_then(|(_, m)| m.trailer.clone());
        // In a dry run only with real pictures asked for (SBCOMMAND_META_LIVE); it's muted either way.
        let ok = !crate::card::dry_run() || std::env::var_os("SBCOMMAND_META_LIVE").is_some();
        let Some(url) = url.filter(|_| ok) else {
            self.page.mon.set(Mon::Pictures);
            self.stage.room.queue_draw();
            return;
        };
        // Streams (Steam's HLS) are saved once to the cache and played from there.
        let path = match trailer_file(&url) {
            Some(p) => p,
            None => {
                self.page.mon.set(Mon::Pictures);
                self.stage.room.queue_draw();
                let key = self.page.mon_key.borrow().clone();
                std::thread::spawn(move || {
                    let got = save_trailer(&url).is_some();
                    glib::MainContext::default().invoke(move || {
                        if let Some(u) = UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade()))
                            && got && *u.page.mon_key.borrow() == key && u.page.mon.get() == Mon::Pictures
                            && !u.page.trailer_done.get() && u.page.full.borrow().is_none() {
                            u.play_trailer_on_monitor();
                        }
                    });
                });
                return;
            }
        };
        self.page.trailer_done.set(true);
        let file = gio::File::for_path(&path);
        let mf = gtk::MediaFile::for_file(&file);
        // The monitor is silent unless you turn its sound on (this trailer only).
        self.page.mon_sound.set(false);
        mf.set_muted(true);
        mf.set_volume(0.0);
        let wk = UI.with(|c| c.borrow().clone());
        mf.connect_ended_notify(move |m| {
            if m.is_ended() && let Some(u) = wk.as_ref().and_then(|w| w.upgrade()) && u.page.full.borrow().is_none() {
                u.stop_video();
                u.page.mon.set(Mon::Pictures);
                u.stage.room.queue_draw();
            }
        });
        mf.play();
        self.stage.video.set_paintable(Some(&mf));
        *self.page.media.borrow_mut() = Some(mf);
        self.page.mon.set(Mon::Trailer);
        self.stage.room.queue_draw();
    }

    fn over_monitor(&self, p: Option<(f64, f64)>) -> bool {
        let (Some((x, y)), Some((sx, sy, sw, sh))) = (p, self.page.layout.borrow().screen_rect) else { return false };
        has_monitor(self.kind_now().as_ref()) && x >= sx && x <= sx + sw && y >= sy && y <= sy + sh
    }

    fn monitor_button_at(&self, x: f64, y: f64) -> Option<&'static str> {
        if !self.over_monitor(Some((x, y))) {
            return None;
        }
        let r = self.page.layout.borrow().screen_rect?;
        let trailer = self.page.meta.borrow().as_ref().is_some_and(|(_, m)| m.trailer.is_some());
        mon_buttons(r, trailer, self.page.mon.get() == Mon::Trailer).into_iter()
            .find(|(_, bx, by, bw, bh)| x >= *bx && x <= bx + bw && y >= *by && y <= by + bh).map(|b| b.0)
    }

    fn monitor_press(self: &Rc<Self>, id: &str) {
        match id {
            "play" => {
                if self.page.mon.get() == Mon::Trailer {
                    self.stop_video();
                    self.page.mon.set(Mon::Pictures);
                } else {
                    self.page.mon_sound.set(false);
                    self.play_trailer_on_monitor();
                }
            }
            "prev" | "next" => {
                if self.page.mon.get() == Mon::Trailer {
                    self.stop_video();
                }
                self.page.mon.set(Mon::Pictures);
                let i = self.page.mon_i.get() as i64 + if id == "next" { 1 } else { -1 };
                let n = self.page.tex.borrow().pics.len().max(1) as i64;
                self.page.mon_i.set(((i % n + n) % n) as usize);
            }
            "sound" => {
                let on = !self.page.mon_sound.get();
                self.page.mon_sound.set(on);
                if let Some(m) = self.page.media.borrow().as_ref() && !crate::card::dry_run() {
                    m.set_muted(!on);
                    m.set_volume(if on { 1.0 } else { 0.0 });
                }
            }
            "full" => self.open_full(false),
            _ => {}
        }
        self.stage.room.queue_draw();
    }

    /// Draws the monitor's buttons while the pointer is over it.
    /// Half a screen: Blueprint joins each note to the plan with a short dashed
    /// line; Callouts joins each choice to what it changes with a red line
    /// (Where to the nearer back speaker, Room to the back wall, Tone to where
    /// you sit, Extras to the sub, or the centre speaker for voices).
    fn draw_half_leaders(&self, cr: &gtk::cairo::Context, theme: &str, lay: &oys::Layout, g: crate::stage::HalfGeo) {
        let tiles = self.stage.tile_rects(&self.stage.room);
        let keys = self.stage.field_keys();
        // Extras points at the centre speaker when it's only voices (or commentary) on, else at the sub.
        let voices = self.stage.field_value("extra").is_some_and(|v| (v.contains("voice") || v.contains("commentary")) && !v.contains("bass"));
        let red = (1.0, 0.165, 0.208);
        cr.save().ok();
        cr.set_source_rgb(red.0, red.1, red.2);
        match theme {
            "blueprint" => {
                let (rx, _, rw, rh) = g.room;
                let k = (rh * 0.94 / 440.0).min((rw - 20.0) / 540.0);
                let (pl, pr) = (rx + rw / 2.0 - 260.0 * k, rx + rw / 2.0 + 260.0 * k);
                cr.set_line_width(1.5);
                cr.set_dash(&[6.0, 5.0], 0.0);
                for (t, _) in &tiles {
                    let y = t.1 + t.3 / 2.0;
                    let (a, b) = if t.0 + t.2 / 2.0 < rx + rw / 2.0 { (t.0 + t.2, pl) } else { (t.0, pr) };
                    if (b - a).abs() > 4.0 {
                        cr.move_to(a, y);
                        cr.line_to(b, y);
                    }
                }
                let _ = cr.stroke();
            }
            "callouts" => {
                let at = |id: &str| lay.speakers.iter().find(|(s, _)| *s == id).map(|(_, b)| (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0));
                cr.set_line_width(2.5);
                let mid = g.room.0 + g.room.2 / 2.0;
                for (i, (_, word)) in tiles.iter().enumerate() {
                    // A callout: a rule under the word, then a line to what it changes.
                    let left = word.0 + word.2 / 2.0 < mid;
                    let base = word.1 + word.3 + 3.0;
                    let from = (if left { word.0 + word.2 + 12.0 } else { word.0 - 12.0 }, base);
                    let near = |a: &str, b: &str| match (at(a), at(b)) {
                        (Some(p), Some(q)) => Some(if (p.0 - from.0).hypot(p.1 - from.1) <= (q.0 - from.0).hypot(q.1 - from.1) { p } else { q }),
                        (p, q) => p.or(q),
                    };
                    let to = match keys.get(i).map(String::as_str) {
                        Some("where") => near("RL", "RR"),
                        Some("tone") => Some(lay.you),
                        Some("extra") => if voices { at("FC") } else { at("LFE") },
                        Some("room") => lay.screen_rect.map(|r| (r.0 + r.2 / 2.0, r.1 + r.3 * 0.2))
                            .or_else(|| at("FL").zip(at("FR")).map(|(a, b)| ((a.0 + b.0) / 2.0, a.1.min(b.1) - g.room.3 * 0.1))),
                        _ => None,
                    };
                    let Some(to) = to else { continue };
                    cr.move_to(if left { word.0 } else { word.0 + word.2 }, base);
                    cr.line_to(from.0, from.1);
                    cr.line_to(to.0, to.1);
                    let _ = cr.stroke();
                    cr.arc(to.0, to.1, 5.0, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
            }
            _ => {}
        }
        cr.restore().ok();
    }

    fn draw_monitor_buttons(&self, cr: &gtk::cairo::Context) {
        if !self.over_monitor(self.page.pointer.get()) {
            return;
        }
        let Some(r) = self.page.layout.borrow().screen_rect else { return };
        let trailer = self.page.meta.borrow().as_ref().is_some_and(|(_, m)| m.trailer.is_some());
        let playing = self.page.mon.get() == Mon::Trailer;
        let hover = self.page.pointer.get();
        for (id, x, y, w, h) in mon_buttons(r, trailer, playing) {
            let hot = hover.is_some_and(|(px, py)| px >= x && px <= x + w && py >= y && py <= y + h);
            if hot { cr.set_source_rgb(art::RED.0, art::RED.1, art::RED.2) } else { cr.set_source_rgba(0.0, 0.0, 0.0, 0.7) }
            if id == "play" {
                cr.arc(x + w / 2.0, y + h / 2.0, w / 2.0, 0.0, 6.283);
                let _ = cr.fill_preserve();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(2.0);
                let _ = cr.stroke();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                if playing {
                    cr.rectangle(cx - 7.0, cy - 7.0, 14.0, 14.0);
                } else {
                    cr.move_to(cx - 6.0, cy - 9.0);
                    cr.line_to(cx + 10.0, cy);
                    cr.line_to(cx - 6.0, cy + 9.0);
                    cr.close_path();
                }
                let _ = cr.fill();
                continue;
            }
            cr.rectangle(x, y, w, h);
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(2.2);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            match id {
                "prev" => { cr.move_to(cx + 4.0, cy - 7.0); cr.line_to(cx - 3.0, cy); cr.line_to(cx + 4.0, cy + 7.0); }
                "next" => { cr.move_to(cx - 4.0, cy - 7.0); cr.line_to(cx + 3.0, cy); cr.line_to(cx - 4.0, cy + 7.0); }
                "full" => {
                    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        cr.move_to(cx + sx * 7.0, cy + sy * 3.0);
                        cr.line_to(cx + sx * 7.0, cy + sy * 7.0);
                        cr.line_to(cx + sx * 3.0, cy + sy * 7.0);
                    }
                }
                _ => {
                    cr.move_to(cx - 7.0, cy - 3.0); cr.line_to(cx - 4.0, cy - 3.0); cr.line_to(cx, cy - 7.0);
                    cr.line_to(cx, cy + 7.0); cr.line_to(cx - 4.0, cy + 3.0); cr.line_to(cx - 7.0, cy + 3.0); cr.close_path();
                    if self.page.mon_sound.get() {
                        let _ = cr.stroke();
                        cr.arc(cx + 1.0, cy, 5.0, -0.8, 0.8);
                    } else {
                        let _ = cr.stroke();
                        cr.move_to(cx + 3.0, cy - 3.0); cr.line_to(cx + 8.0, cy + 3.0);
                        cr.move_to(cx + 8.0, cy - 3.0); cr.line_to(cx + 3.0, cy + 3.0);
                    }
                }
            }
            let _ = cr.stroke();
        }
    }

    pub fn stop_video(&self) {
        if let Some(m) = self.page.media.borrow_mut().take() {
            m.pause();
            m.set_muted(true);
            // Clearing a stream that is still opening trips an assertion in
            // GTK's GStreamer backend; paused and dropped is enough then.
            if m.is_prepared() {
                m.clear();
            }
        }
        self.stage.video.set_paintable(None::<&gdk::Paintable>);
        self.stage.video.set_visible(false);
    }

    // ---- the room

    pub(crate) fn draw_room(&self, cr: &gtk::cairo::Context, w: f64, h: f64) {
        let (_, compact) = self.layout_now();
        if compact {
            // The compact view: the small speaker plan at the top right, if there's room.
            // (Ending above the sound choices.)
            let side = (w * 0.42).min(h * 0.5).min(460.0).min(self.stage.lower_top() - 54.0 - 12.0);
            if w > 560.0 && side >= 220.0 {
                cr.save().ok();
                cr.translate(w - side - 12.0, 54.0);
                cr.rectangle(-12.0, 0.0, side + 24.0, side);
                cr.clip();
                // The plan's own margins (the old room card's are for a taller box).
                let mut st = self.room_state();
                st.top = 6.0;
                st.bottom = 24.0;
                room::draw(cr, side, side, &st);
                self.page.mini.set(Some((w - side - 12.0, 54.0, side)));
                cr.restore().ok();
            } else {
                self.page.mini.set(None);
            }
            *self.page.layout.borrow_mut() = oys::Layout::default();
            self.stage.place_video(None);
            return;
        }
        let s = self.s.borrow().clone();
        let np = self.np.borrow().clone();
        let kind = self.kind_now().unwrap_or(Kind::Studio);
        let stream_ch = np.as_ref().and_then(|n| n.stream.as_ref()).map(|f| f.channels).unwrap_or(2);
        let chans = model::channels(&s.speaker_config);
        let lv = self.levels.get();
        // meters::ORDER is FL,FR,FC,LFE,RL,RR; the room's ORDER is FL,FC,FR,LFE,RL,RR.
        let lvl = |ch: &str| crate::meters::ORDER.iter().position(|c| *c == ch).map(|i| lv[i] as f64).unwrap_or(0.0);
        let active: [bool; 6] = std::array::from_fn(|i| {
            let ch = oys::ORDER[i];
            let ch = if ch == "FC" { "FC" } else { ch };
            !s.headphones() && chans.contains(&ch) && (stream_ch >= 6 || matches!(ch, "FL" | "FR" | "LFE") || (s.spreads() && matches!(ch, "RL" | "RR")))
        });
        let levels: [f64; 6] = std::array::from_fn(|i| lvl(oys::ORDER[i]));
        let prof = s.cur_profile();
        let place = if !s.sbx_active() { oys::Place::Plain } else {
            match (prof.as_str(), &kind) {
                ("Music" | "Concert" | "Stadium" | "Arena", Kind::Studio) => oys::Place::Studio,
                ("Music", _) => oys::Place::Studio,
                ("Concert", _) => oys::Place::Concert,
                ("Stadium" | "Arena", _) => oys::Place::Stadium,
                ("Cinema" | "Night", _) => oys::Place::Cinema,
                _ if s.activity == "game" || kind == Kind::Game => oys::Place::Gaming,
                _ => oys::Place::Cinema,
            }
        };
        let studio_grow = if kind == Kind::Studio && s.sbx_active() { match prof.as_str() { "Stadium" | "Arena" => 1.0, "Concert" => 0.6, _ => 0.0 } } else { 0.0 };
        let size = match prof.as_str() { "Concert" | "Cinema" | "Gaming" => 1.0, "Stadium" | "Arena" => 2.0, _ => 0.0 };
        let fx = s.fx_of(&prof);
        let spread = if s.sbx_active() && fx.surround.0 { fx.surround.1 as f64 / 100.0 } else { 0.3 };
        let tone_rgb = match s.tone { t if t <= -2 => (1.0, 0.416, 0.122), -1 => (1.0, 0.333, 0.212), 0 => (1.0, 0.231, 0.267), 1 => (1.0, 0.561, 0.596), _ => (1.0, 0.824, 0.839) };
        let tex = self.page.tex.borrow();
        let fresh = np.as_ref().is_some_and(|n| n.key() == tex.key);
        let artist = np.as_ref().map(|n| n.artist.to_uppercase()).unwrap_or_default();
        let title_up = np.as_ref().map(|n| n.title.to_uppercase()).unwrap_or_default();
        let score = np.as_ref().and_then(|n| teams(&n.title));
        let screen = match kind {
            Kind::Game | Kind::Film | Kind::Tv => match self.page.mon.get() {
                Mon::Logo => tex.logo_s.as_ref().filter(|_| fresh).map(oys::Screen::Logo).unwrap_or(oys::Screen::Title(&title_up)),
                Mon::Trailer if self.page.media.borrow().is_some() => oys::Screen::Video,
                _ => {
                    let n = tex.pics.len();
                    if fresh && n > 0 { oys::Screen::Picture(&tex.pics[self.page.mon_i.get() % n]) } else { oys::Screen::Title(&title_up) }
                }
            },
            Kind::Sport => match &score {
                Some((home, away)) => oys::Screen::Score { home, away, home_badge: None, away_badge: None },
                None => oys::Screen::Title(&title_up),
            },
            _ => oys::Screen::None,
        };
        let neon_text = if artist.is_empty() { title_up.as_str() } else { artist.as_str() };
        let half = self.stage.half_geo();
        let half_on = half.is_some();
        let sc = oys::Scene {
            place, studio_grow, active, levels, headphones: s.headphones(), sbx_on: s.sbx_active(), tone_rgb, size, spread,
            around: !s.headphones() && (stream_ch >= 6 || s.spreads()), playing: np.as_ref().is_some_and(|n| n.playing),
            t: self.t0.elapsed().as_secs_f64(), neon_text, neon_logo: tex.neon.as_ref().filter(|_| fresh),
            records: if fresh && kind == Kind::Studio { &tex.records } else { &[] }, screen,
            left_clip: if !half_on && (self.stage.layout() == Layout::Split || self.stage.layout() == Layout::SplitSlim) { w * 0.44 + 84.0 } else { 0.0 },
            corner: if half_on { None } else { self.stage.corner_rect() },
            fill: half_on,
        };
        let now = self.stage.room.frame_clock().map(|c| c.frame_time()).unwrap_or(0);
        let prev = self.page.last_frame.get();
        let dt = if prev > 0 && now > prev { ((now - prev) as f64 / 1e6).min(0.1) } else { 0.03 };
        let draw = |cr: &gtk::cairo::Context, w: f64, h: f64| match s.theme.as_str() {
            "blueprint" | "screenprint" | "callouts" => crate::themes::draw(&s.theme, cr, w, h, dt, &sc, &mut self.page.smooth.borrow_mut()),
            _ => oys::draw(cr, w, h, dt, &sc, &mut self.page.smooth.borrow_mut()),
        };
        let lay = match half {
            // Half a screen: the room in its own box, its edges fading into the page.
            Some(g) => {
                let (rx, ry, rw, rh) = g.room;
                cr.push_group();
                cr.translate(rx, ry);
                let mut lay = draw(cr, rw, rh);
                let _ = cr.pop_group_to_source();
                let (top, bottom): (f64, f64) = match s.theme.as_str() { "blueprint" => (0.0, 0.0), "screenprint" => (0.0, 0.06), "callouts" => (0.06, 0.1), _ => (0.1, 0.2) };
                let m = gtk::cairo::LinearGradient::new(0.0, ry, 0.0, ry + rh);
                m.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, if top > 0.0 { 0.0 } else { 1.0 });
                m.add_color_stop_rgba(top.max(0.001), 0.0, 0.0, 0.0, 1.0);
                m.add_color_stop_rgba(1.0 - bottom.max(0.001), 0.0, 0.0, 0.0, 1.0);
                m.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, if bottom > 0.0 { 0.0 } else { 1.0 });
                let _ = cr.mask(&m);
                for (_, b) in lay.speakers.iter_mut() {
                    b.0 += rx;
                    b.1 += ry;
                }
                if let Some(r) = lay.screen_rect.as_mut() {
                    r.0 += rx;
                    r.1 += ry;
                }
                lay.you = (lay.you.0 + rx, lay.you.1 + ry);
                self.draw_half_leaders(cr, &s.theme, &lay, g);
                lay
            }
            None => draw(cr, w, h),
        };
        drop(tex);
        *self.page.layout.borrow_mut() = lay.clone();
        if has_monitor(Some(&kind)) {
            self.draw_monitor_buttons(cr);
        }
        let video_rect = if matches!(self.page.mon.get(), Mon::Trailer) && has_monitor(Some(&kind)) { lay.screen_rect } else { None };
        self.stage.place_video(video_rect);
        *self.page.layout.borrow_mut() = lay;
    }

    // ---- full screen: the pictures, and the trailer with sound

    pub fn open_full(self: &Rc<Self>, trailer_first: bool) {
        if self.page.full.borrow().is_some() {
            return;
        }
        let m = self.page.meta.borrow().as_ref().map(|x| x.1.clone()).unwrap_or_default();
        let pics: Vec<std::path::PathBuf> = if m.pictures.is_empty() { m.backgrounds.clone() } else { m.pictures.clone() };
        let ov = gtk::Overlay::builder().css_classes(["fullview"]).hexpand(true).vexpand(true).focusable(true).build();
        let pic = gtk::Picture::builder().content_fit(gtk::ContentFit::Contain).can_shrink(true).hexpand(true).vexpand(true).build();
        ov.set_child(Some(&pic));
        let bar = gtk::Box::builder().spacing(12).css_classes(["fbar"]).valign(gtk::Align::Start).build();
        let kind_l = gtk::Label::builder().label("PICTURES").css_classes(["fk"]).build();
        let title = gtk::Label::builder().label(m.title.to_uppercase()).css_classes(["ft"]).build();
        let play = gtk::Button::builder().label("▶  Play the trailer  ·  with sound").css_classes(["fplay"]).visible(m.trailer.is_some()).build();
        let back = gtk::Button::builder().label("⤡  Back to the page  ·  Esc").css_classes(["fback"]).hexpand(true).halign(gtk::Align::End).build();
        bar.append(&kind_l);
        bar.append(&title);
        bar.append(&play);
        bar.append(&back);
        ov.add_overlay(&bar);
        let prev = gtk::Button::builder().icon_name("go-previous-symbolic").css_classes(["farrow"]).halign(gtk::Align::Start).valign(gtk::Align::Center).margin_start(24).build();
        let next = gtk::Button::builder().icon_name("go-next-symbolic").css_classes(["farrow"]).halign(gtk::Align::End).valign(gtk::Align::Center).margin_end(24).build();
        ov.add_overlay(&prev);
        ov.add_overlay(&next);
        let ctl = gtk::Box::builder().spacing(10).css_classes(["fctl"]).valign(gtk::Align::End).margin_start(24).margin_end(24).margin_bottom(20).visible(false).build();
        let fpp = gtk::Button::builder().icon_name("media-playback-pause-symbolic").css_classes(["flat"]).build();
        let seek = gtk::Scale::builder().orientation(gtk::Orientation::Horizontal).hexpand(true).draw_value(false).build();
        seek.set_range(0.0, 1.0);
        let mute = gtk::ToggleButton::builder().icon_name("audio-volume-high-symbolic").css_classes(["flat"]).tooltip_text("Mute").build();
        ctl.append(&fpp);
        ctl.append(&seek);
        ctl.append(&mute);
        ov.add_overlay(&ctl);
        let idx = Rc::new(Cell::new(self.page.mon_i.get()));
        let show = {
            let (pic, pics, idx) = (pic.clone(), pics.clone(), idx.clone());
            Rc::new(move |d: i64| {
                if pics.is_empty() { return; }
                let n = pics.len() as i64;
                let i = ((idx.get() as i64 + d) % n + n) % n;
                idx.set(i as usize);
                if let Ok(t) = gdk::Texture::from_filename(&pics[i as usize]) {
                    pic.set_paintable(Some(&t));
                }
            })
        };
        show(0);
        {
            let s = show.clone();
            prev.connect_clicked(move |_| s(-1));
            let s = show.clone();
            next.connect_clicked(move |_| s(1));
        }
        // Play the trailer: the monitor's own stream moves here, with sound.
        let start_trailer = {
            let (pic, kind_l, prev, next, ctl, play, fpp, seek, mute) = (pic.clone(), kind_l.clone(), prev.clone(), next.clone(), ctl.clone(), play.clone(), fpp.clone(), seek.clone(), mute.clone());
            let wk = Rc::downgrade(self);
            let url = m.trailer.clone();
            Rc::new(move || {
                let Some(u) = wk.upgrade() else { return };
                let mf = u.page.media.borrow().clone().or_else(|| {
                    let p = trailer_file(&url.clone()?)?;
                    Some(gtk::MediaFile::for_filename(&p))
                });
                let Some(mf) = mf else {
                    u.toast("The trailer is still being saved: it plays in a moment");
                    return;
                };
                u.stage.video.set_paintable(None::<&gdk::Paintable>);
                u.stage.video.set_visible(false);
                *u.page.media.borrow_mut() = Some(mf.clone());
                u.page.mon.set(Mon::Trailer);
                if !crate::card::dry_run() {
                    mf.set_muted(false);
                    mf.set_volume(1.0);
                    mf.play();
                }
                pic.set_paintable(Some(&mf));
                kind_l.set_label("TRAILER");
                prev.set_visible(false);
                next.set_visible(false);
                play.set_visible(false);
                ctl.set_visible(true);
                let (fpp, seek, mute) = (fpp.clone(), seek.clone(), mute.clone());
                mf.connect_timestamp_notify(move |m| {
                    let d = m.duration();
                    if d > 0 { seek.set_value(m.timestamp() as f64 / d as f64); }
                    fpp.set_icon_name(if m.is_playing() { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
                    mute.set_icon_name(if m.is_muted() { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
                });
            })
        };
        {
            let s = start_trailer.clone();
            play.connect_clicked(move |_| s());
            let wk = Rc::downgrade(self);
            fpp.connect_clicked(move |_| {
                if let Some(u) = wk.upgrade() && let Some(m) = u.page.media.borrow().as_ref() {
                    if m.is_playing() { m.pause() } else { m.play() }
                }
            });
            let wk = Rc::downgrade(self);
            mute.connect_toggled(move |b| {
                if let Some(u) = wk.upgrade() && let Some(m) = u.page.media.borrow().as_ref() {
                    m.set_muted(b.is_active());
                }
            });
            let wk = Rc::downgrade(self);
            seek.connect_change_value(move |_, _, v| {
                if let Some(u) = wk.upgrade() && let Some(m) = u.page.media.borrow().as_ref() {
                    m.seek((v.clamp(0.0, 1.0) * m.duration() as f64) as i64);
                }
                glib::Propagation::Proceed
            });
        }
        let close = {
            let wk = Rc::downgrade(self);
            let w2 = ov.downgrade();
            Rc::new(move || {
                let Some(u) = wk.upgrade() else { return };
                *u.page.full.borrow_mut() = None;
                if let Some(w) = w2.upgrade() { u.root_ov.remove_overlay(&w) }
                if !u.page.was_fullscreen.get() { u.win.unfullscreen() }
                // A trailer still playing carries on, silently, on the monitor.
                let media = u.page.media.borrow().clone();
                if let Some(m) = media {
                    if m.is_playing() && has_monitor(u.kind_now().as_ref()) {
                        m.set_muted(true);
                        m.set_volume(0.0);
                        u.stage.video.set_paintable(Some(&m));
                        u.page.mon.set(Mon::Trailer);
                    } else {
                        u.stop_video();
                        u.page.mon.set(Mon::Pictures);
                    }
                }
                u.page.mon_i.set(idx.get());
                u.stage.room.queue_draw();
            })
        };
        {
            let c = close.clone();
            back.connect_clicked(move |_| c());
            let c = close.clone();
            let keys = gtk::EventControllerKey::new();
            let s = show.clone();
            keys.connect_key_pressed(move |_, k, _, _| {
                match k {
                    gdk::Key::Escape => c(),
                    gdk::Key::Left => s(-1),
                    gdk::Key::Right => s(1),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            });
            ov.add_controller(keys);
        }
        *self.page.full.borrow_mut() = Some(ov.clone());
        self.root_ov.add_overlay(&ov);
        self.page.was_fullscreen.set(self.win.is_fullscreen());
        self.win.fullscreen();
        ov.grab_focus();
        let carry = self.page.mon.get() == Mon::Trailer && self.page.media.borrow().is_some();
        if (trailer_first || carry) && (m.trailer.is_some() || carry) {
            start_trailer();
        }
    }

    // ---- Look and feel

    /// Builds (or rebuilds) the Look and feel panel for the page showing now.
    /// It fits the control centre without scrolling: small pictures in rows,
    /// your defaults as one sentence, the compact views as three short lines.
    pub fn build_look_page(self: &Rc<Self>) {
        let p = &self.look_page;
        while let Some(c) = p.first_child() {
            p.remove(&c);
        }
        let s = self.s.borrow().clone();
        let kind = self.kind_now().map(|k| page_kind(&k)).unwrap_or("music");
        let kname = |k: &str| match k { "music" => "music", "film" => "films", "tv" => "TV", "game" => "games", "sport" => "sport", "video" => "online video", _ => "streams" };
        // A section's heading, with room for buttons on its right.
        let head = |t: &str| {
            let h = gtk::Box::builder().spacing(6).build();
            h.append(&gtk::Label::builder().label(t).xalign(0.0).hexpand(true).valign(gtk::Align::Center).css_classes(["lf-h"]).build());
            h
        };
        // A small picture with its name under it.
        let tile = |name: &str, tip: &str, on: bool, draw: Box<dyn Fn(&gtk::cairo::Context, f64, f64)>| {
            let bx = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).build();
            let da = gtk::DrawingArea::builder().content_width(40).content_height(30).hexpand(true).build();
            da.set_draw_func(move |_, cr, w, h| draw(cr, w as f64, h as f64));
            bx.append(&da);
            bx.append(&gtk::Label::builder().label(name).xalign(0.0).wrap(true).lines(2).css_classes(["lf-n"]).build());
            let b = gtk::Button::builder().child(&bx).tooltip_text(tip).css_classes(["lf-card"]).build();
            if on { b.add_css_class("on"); }
            b
        };
        // Theme: five pictures in a row.
        p.append(&head("THEME"));
        let tf = gtk::Box::builder().spacing(3).homogeneous(true).build();
        for (id, name) in model::THEMES {
            let b = tile(name, name, s.theme == id, Box::new(move |cr, w, h| draw_theme_preview(cr, w, h, id)));
            let wk = Rc::downgrade(self);
            b.connect_clicked(move |_| if let Some(u) = wk.upgrade() {
                u.s.borrow_mut().theme = id.into();
                u.save_now();
                u.update_stage();
                u.build_look_page();
            });
            tf.append(&b);
        }
        p.append(&tf);
        // Layout: four to a row; Save and Back sit by the heading once one is picked.
        let own = s.layout_defaults.get(kind).cloned()
            .unwrap_or_else(|| model::LAYOUT_DEFAULTS.iter().find(|x| x.0 == kind).map_or("slim", |x| x.1).to_string());
        let mut ids: Vec<(String, String)> = vec![("default".into(), "Default".into())];
        for (id, name, _) in model::LAYOUTS {
            if model::fit_layout(kind, id, &own) == id {
                ids.push((id.into(), name.into()));
            }
        }
        let chosen = if s.layout.is_empty() { "default".to_string() } else { s.layout.clone() };
        let lh = head("LAYOUT");
        if chosen != "default" {
            let save = gtk::Button::builder().label(format!("Save for {}", kname(kind))).css_classes(["lf-save"])
                .tooltip_text(format!("Pages of {} open with this layout from now on", kname(kind))).build();
            let back = gtk::Button::builder().label("↺ Default").css_classes(["lf-back"]).tooltip_text("Back to the default view").build();
            let wk = Rc::downgrade(self);
            let k2 = kind.to_string();
            save.connect_clicked(move |_| if let Some(u) = wk.upgrade() {
                let pick = u.s.borrow().layout.clone();
                let own = u.s.borrow().layout_for(&k2);
                let fit = model::fit_layout(&k2, &pick, &own);
                {
                    let mut s = u.s.borrow_mut();
                    s.layout_defaults.insert(k2.clone(), fit.clone());
                    s.layout = "default".into();
                }
                u.save_now();
                u.toast(&format!("Saved: {} now open with “{}”", kname(&k2), layout_name(&fit)));
                u.update_stage();
                u.build_look_page();
            });
            let wk = Rc::downgrade(self);
            back.connect_clicked(move |_| if let Some(u) = wk.upgrade() {
                u.s.borrow_mut().layout = "default".into();
                u.save_now();
                u.update_stage();
                u.build_look_page();
            });
            lh.append(&save);
            lh.append(&back);
        }
        p.append(&lh);
        let lf = gtk::Grid::builder().column_spacing(4).row_spacing(4).column_homogeneous(true).build();
        for (i, (id, name)) in ids.into_iter().enumerate() {
            let tip = if id == "default" { format!("Each kind of page uses its own layout ({} for {})", layout_name(&own), kname(kind)) }
                      else { model::LAYOUTS.iter().find(|x| x.0 == id).map_or(String::new(), |x| x.2.to_string()) };
            let lid = id.clone();
            let b = tile(&name, &tip, chosen == id, Box::new(move |cr, w, h| draw_layout_preview(cr, w, h, &lid)));
            let wk = Rc::downgrade(self);
            b.connect_clicked(move |_| if let Some(u) = wk.upgrade() {
                u.s.borrow_mut().layout = id.clone();
                u.save_now();
                u.update_stage();
                u.build_look_page();
            });
            lf.attach(&b, (i % 4) as i32, (i / 4) as i32, 1, 1);
        }
        p.append(&lf);
        // Your defaults, as one sentence: which kinds open with which layout.
        let dh = head("YOUR DEFAULTS");
        if !s.layout_defaults.is_empty() {
            let reset = gtk::Button::builder().label("Reset").css_classes(["lf-link"]).tooltip_text("Back to the original defaults").build();
            let wk = Rc::downgrade(self);
            reset.connect_clicked(move |_| if let Some(u) = wk.upgrade() {
                {
                    let mut s = u.s.borrow_mut();
                    s.layout_defaults.clear();
                    s.layout = "default".into();
                }
                u.save_now();
                u.toast("Back to the original defaults");
                u.update_stage();
                u.build_look_page();
            });
            dh.append(&reset);
        }
        p.append(&dh);
        let mut groups: Vec<(String, Vec<&str>)> = vec![];
        for (k, d) in model::LAYOUT_DEFAULTS {
            let n = layout_name(&s.layout_defaults.get(k).cloned().unwrap_or_else(|| d.to_string()));
            match groups.iter_mut().find(|g| g.0 == n) {
                Some(g) => g.1.push(k),
                None => groups.push((n, vec![k])),
            }
        }
        let sentence = groups.iter().map(|(n, ks)| {
            let mut first = true;
            let names: Vec<String> = ks.iter().map(|k| {
                let mut t = kname(k).to_string();
                if std::mem::take(&mut first) {
                    let mut c = t.chars();
                    t = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
                }
                if *k == kind { format!("<b>{t}</b>") } else { t }
            }).collect();
            let list = match names.split_last() {
                Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
                _ => names.concat(),
            };
            format!("{list} open with {}.", glib::markup_escape_text(n))
        }).collect::<Vec<_>>().join(" ");
        p.append(&gtk::Label::builder().label(&sentence).use_markup(true).xalign(0.0).wrap(true).width_chars(30).max_width_chars(30)
            .css_classes(["lf-hint"]).build());
        // The compact views: what small windows show, the Remote's top, the Strip.
        p.append(&head("THE COMPACT VIEWS"));
        let choice = |text: &str, tip: &str, items: [(&'static str, &'static str, &'static str); 2], cur: &str, act: fn(&Rc<Ui>, &str)| {
            let row = gtk::Box::builder().spacing(10).build();
            row.append(&gtk::Label::builder().label(text).tooltip_text(tip).xalign(0.0).hexpand(true).css_classes(["lf-row"]).build());
            let (seg, btns) = segmented(&items.map(|x| x.1), false);
            for (b, (id, _, about)) in btns.iter().zip(items) {
                b.set_active(cur == id);
                b.set_tooltip_text(Some(about));
                let wk = Rc::downgrade(self);
                b.connect_toggled(move |b| if b.is_active() && let Some(u) = wk.upgrade() { act(&u, id) });
            }
            row.append(&seg);
            row
        };
        p.append(&choice("Small windows show", "Windows under 900 × 600", [
            ("remote", "Remote", "The Remote: a tall card with the photo, the speakers and four tiles"),
            ("mini", "Mini", "Over your shoulder, mini: the main page made tall, with the neon sign and dot speakers")],
            &s.compact_view, |u, id| {
                u.s.borrow_mut().compact_view = id.into();
                u.save_now();
                let (w, h) = (u.sizer.width(), u.sizer.height());
                u.on_window_size(w, h);
                u.build_look_page();
            }));
        // The compact view in big windows too (it was the "Compact" theme).
        let arow = gtk::Box::builder().spacing(10).build();
        arow.append(&gtk::Label::builder().label("In big windows too").xalign(0.0).hexpand(true).css_classes(["lf-row"]).build());
        let always = gtk::Switch::builder().active(s.compact_always).valign(gtk::Align::Center)
            .tooltip_text("Show the compact view whatever the window's size").build();
        let wk = Rc::downgrade(self);
        always.connect_active_notify(move |b| if let Some(u) = wk.upgrade() {
            u.s.borrow_mut().compact_always = b.is_active();
            u.save_now();
            u.update_stage();
            let (w, h) = (u.sizer.width(), u.sizer.height());
            u.on_window_size(w, h);
        });
        arow.append(&always);
        p.append(&arow);
        // (the Remote's top only matters when the Remote is the view picked)
        if s.compact_view == "remote" {
            p.append(&choice("The Remote’s top", "What fills the top of the Remote", [
                ("photo", "Photo", "The artist photo printed in red, the cover by the title"),
                ("album", "Album", "The album art, whole and in colour")],
                &s.compact_top, |u, id| {
                    u.s.borrow_mut().compact_top = id.into();
                    u.save_now();
                    u.update_stage();
                    u.build_look_page();
                }));
        }
    }
}

fn layout_name(id: &str) -> String {
    model::LAYOUTS.iter().find(|x| x.0 == id).map_or("Slim dock".into(), |x| x.1.to_string())
}

/// Sends a play / pause / next / previous to the player showing this title.
pub(super) fn mpris(method: &str, title: &str) {
    let players = crate::detect::all_players();
    let p = players.iter().find(|p| !title.is_empty() && p.title == title)
        .or_else(|| players.iter().find(|p| p.status == "Playing"))
        .or_else(|| players.first());
    if let Some(p) = p {
        let _ = crate::system::run("gdbus", &["call", "--session", "--dest", &p.bus, "--object-path", "/org/mpris/MediaPlayer2",
            "--method", &format!("org.mpris.MediaPlayer2.Player.{method}")]);
    }
}

/// For the self-test and screenshots: details from the demo track, no network.
fn demo_meta(np: &NowPlaying) -> Meta {
    // The made-up details for the kind of thing playing (a match title gets the match).
    let mut m = meta::demo(meta::classify(np).id());
    if m.title.is_empty() {
        m = Meta { kind: Some(meta::classify(np)), title: np.title.clone(), subtitle: np.artist.clone(), kicker_app: np.app.clone(),
                   ..Default::default() };
    }
    if m.cover.is_none() { m.cover = np.art.clone(); }
    if m.backgrounds.is_empty() { m.backgrounds = np.art.iter().cloned().collect(); }
    if m.pictures.is_empty() && np.activity == "game" { m.pictures = np.art.iter().cloned().collect(); }
    m
}

/// Prepares the pictures (helper thread).
fn prepare(key: &str, m: &Meta, np: &NowPlaying) -> Prepared {
    let game = m.kind == Some(Kind::Game);
    let bg_look = if game { Look::RedLevelled } else { Look::Red };
    let cover_path = m.cover.clone().or_else(|| np.art.clone());
    Prepared {
        key: key.into(),
        bg: m.backgrounds.iter().take(6).filter_map(|p| art::load(p, bg_look, 1600)).collect(),
        cover: cover_path.as_ref().and_then(|p| art::load(p, Look::Plain, 600)),
        logo: m.logo.as_ref().and_then(|p| art::load(p, Look::Plain, 760)),
        neon: if game { None } else { m.logo.as_ref().and_then(|p| art::ink(p, 600)) },
        split: cover_path.as_ref().and_then(|p| art::load(p, Look::Dim, 1200)),
        records: m.records.iter().take(5).filter_map(|p| art::load(p, Look::Plain, 160)).collect(),
        pics: m.pictures.iter().take(14).filter_map(|p| art::load(p, Look::Plain, 960)).collect(),
    }
}

/// The small theme pictures in Look and feel.
fn draw_theme_preview(cr: &gtk::cairo::Context, w: f64, h: f64, id: &str) {
    let (r, g, b) = art::RED;
    cr.set_source_rgb(0.08, 0.03, 0.035);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();
    cr.set_line_width(1.5);
    match id {
        "blueprint" => {
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.rectangle(w * 0.15, h * 0.1, w * 0.7, h * 0.8);
            let _ = cr.stroke();
            cr.set_source_rgb(r, g, b);
            cr.set_dash(&[2.0, 4.0], 0.0);
            cr.arc(w * 0.5, h * 0.6, h * 0.24, 0.0, 6.283);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }
        "screenprint" => {
            cr.set_source_rgb(0.3, 0.05, 0.06);
            cr.rectangle(0.0, 0.0, w, h);
            let _ = cr.fill();
            cr.set_source_rgb(r, g, b);
            cr.rectangle(w * 0.3, 0.0, w * 0.4, h * 0.42);
            let _ = cr.fill();
            cr.move_to(0.0, h);
            cr.line_to(w * 0.3, h * 0.42);
            cr.line_to(w * 0.7, h * 0.42);
            cr.line_to(w, h);
            cr.close_path();
            cr.set_source_rgb(0.45, 0.06, 0.08);
            let _ = cr.fill();
        }
        "callouts" => {
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(w * 0.5, h * 0.15);
            cr.line_to(w * 0.85, h * 0.42);
            cr.line_to(w * 0.5, h * 0.7);
            cr.line_to(w * 0.15, h * 0.42);
            cr.close_path();
            let _ = cr.stroke();
            cr.set_source_rgb(r, g, b);
            cr.move_to(0.0, h * 0.85);
            cr.line_to(w * 0.4, h * 0.55);
            let _ = cr.stroke();
        }
        _ => {
            cr.set_source_rgba(r, g, b, 0.9);
            for i in 0..9 { for j in 0..4 {
                let x = w * 0.1 + i as f64 * w * 0.1;
                let y = h * 0.6 + j as f64 * h * 0.1;
                cr.arc(x, y, 0.6 + j as f64 * 0.7, 0.0, 6.283);
                let _ = cr.fill();
            } }
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(w * 0.2, h * 0.5);
            cr.line_to(w * 0.35, h * 0.2);
            cr.line_to(w * 0.65, h * 0.2);
            cr.line_to(w * 0.8, h * 0.5);
            let _ = cr.stroke();
            cr.arc(w * 0.5, h * 0.85, 5.0, 0.0, 6.283);
            let _ = cr.fill();
        }
    }
}

/// The small layout pictures in Look and feel.
fn draw_layout_preview(cr: &gtk::cairo::Context, w: f64, h: f64, id: &str) {
    let (r, g, b) = art::RED;
    let sx = w / 96.0;
    let sy = h / 56.0;
    let rect = |cr: &gtk::cairo::Context, x: f64, y: f64, ww: f64, hh: f64| { cr.rectangle(x * sx, y * sy, ww * sx, hh * sy); };
    cr.set_source_rgb(0.086, 0.02, 0.024);
    rect(cr, 0.0, 0.0, 96.0, 56.0);
    let _ = cr.fill();
    if id == "default" {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
        cr.set_font_size(11.0 * sx);
        if let Ok(e) = cr.text_extents("Default") {
            cr.move_to(w / 2.0 - e.width() / 2.0, h * 0.55);
            let _ = cr.show_text("Default");
        }
        return;
    }
    let split = id.starts_with("split");
    if split {
        cr.set_source_rgba(1.0, 0.23, 0.27, 0.33);
        rect(cr, 0.0, 0.0, 44.0, 56.0);
        let _ = cr.fill();
    }
    cr.set_source_rgba(r, g, b, 0.25);
    cr.move_to(60.0 * sx, 16.0 * sy);
    cr.line_to(92.0 * sx, 10.0 * sy);
    cr.line_to(92.0 * sx, 52.0 * sy);
    cr.line_to(60.0 * sx, 46.0 * sy);
    cr.close_path();
    let _ = cr.fill();
    let title_x = if matches!(id, "slim" | "rowstitle" | "bar") { 21.0 } else { 8.0 };
    if matches!(id, "slim" | "rowstitle" | "bar") {
        cr.set_source_rgb(1.0, 0.23, 0.27);
        rect(cr, 8.0, 8.0, 10.0, 11.0);
        let _ = cr.fill();
    }
    cr.set_source_rgb(1.0, 1.0, 1.0);
    rect(cr, title_x, 9.0, 24.0, 5.0);
    let _ = cr.fill();
    cr.set_source_rgb(1.0, 0.54, 0.56);
    rect(cr, title_x, 16.0, 15.0, 3.0);
    let _ = cr.fill();
    match id {
        "slimbig" => { cr.set_source_rgb(1.0, 0.23, 0.27); rect(cr, 7.0, 25.0, 17.0, 18.0); let _ = cr.fill(); }
        "rows" | "rowstitle" | "split" => {
            let x0 = if id == "rows" { 23.0 } else { 8.0 };
            if id == "rows" { cr.set_source_rgb(1.0, 0.23, 0.27); rect(cr, 7.0, 28.0, 13.0, 16.0); let _ = cr.fill(); }
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
            for y in [29.0, 33.0, 37.0, 41.0] { rect(cr, x0, y, 22.0, 2.0); }
            let _ = cr.fill();
        }
        "bar" => {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.8);
            rect(cr, 4.0, 38.0, 88.0, 8.0);
            let _ = cr.fill();
            cr.set_source_rgb(r, g, b);
            rect(cr, 4.0, 38.0, 13.0, 8.0);
            let _ = cr.fill();
        }
        _ => {}
    }
    if matches!(id, "slim" | "slimbig" | "splitslim") {
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.7);
        rect(cr, 6.0, 48.0, 50.0, 3.0);
        let _ = cr.fill();
        cr.set_source_rgb(r, g, b);
        rect(cr, 5.0, 47.0, 6.0, 5.0);
        let _ = cr.fill();
    }
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
    rect(cr, 66.0, 47.0, 24.0, 5.0);
    let _ = cr.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_titles_give_the_scoreboard_teams() {
        let t = |s: &str| teams(s).map(|(a, b)| format!("{a} v {b}"));
        assert_eq!(t("Harbour City v Northgate United").as_deref(), Some("HAR v NOR"));
        assert_eq!(t("Premier League: Arsenal vs. Chelsea | Highlights").as_deref(), Some("ARS v CHE"));
        assert_eq!(t("Arsenal vs Chelsea - Full match").as_deref(), Some("ARS v CHE"));
        assert_eq!(t("Starfall").as_deref(), None);
        assert_eq!(t("Grand Prix v").as_deref(), None);
    }
}

// ---------------------------------------------------------------------------
// The compact views (mini.rs) and the window around them
// ---------------------------------------------------------------------------

impl Ui {
    /// The ☰ menu: at the top the technical line (what plays, SBX, the EQ),
    /// then every panel, then the Strip. The Strip's own menu leads with the way
    /// back to the full window and ends with minimise and quit.
    fn menu_popover(self: &Rc<Self>, strip: bool) -> gtk::Popover {
        let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(2).margin_top(6).margin_bottom(6)
            .margin_start(6).margin_end(6).css_classes(["amenu"]).build();
        let pop = gtk::Popover::builder().child(&col).has_arrow(false).position(gtk::PositionType::Top).build();
        col.set_size_request(300, -1);
        let now_t = gtk::Label::builder().xalign(0.0).css_classes(["mn-t"]).build();
        let now_s = gtk::Label::builder().xalign(0.0).css_classes(["mn-s"]).wrap(true).max_width_chars(34).build();
        let now = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(1).css_classes(["mn"]).build();
        now.append(&now_t);
        now.append(&now_s);
        col.append(&now);
        {
            let wk = Rc::downgrade(self);
            pop.connect_show(move |_| if let Some(u) = wk.upgrade() {
                now_t.set_label(&u.stage.readout.label());
                now_s.set_label(&u.tech_line());
            });
        }
        let sep = |t: &str| gtk::Label::builder().label(t).xalign(0.0).css_classes(["mn-sep"]).build();
        let mut items: Vec<(&str, &str, &str)> = vec![];
        if strip { items.push(("full", "Back to the full window", "The page, with the room and the choices")); }
        items.extend([("sound", "Sound…", "SBX, effects and the equaliser"),
                      ("mixer", "Mixer…", "Levels for each speaker"),
                      ("crystalvoice", "Microphone…", "CrystalVoice and What U Hear"),
                      ("setup", "Speakers…", "Speaker setup, the subwoofer, a test"),
                      ("|", "THE PAGE", ""),
                      ("look", "Look and feel…", "Theme, layout and the compact views"),
                      ("test", "Speaker test", "Hear each speaker on its own"),
                      ("settings", "Settings…", "Automatic switching, pictures, the card")]);
        if strip { items.extend([("|", "THE WINDOW", ""), ("min", "Minimise", ""), ("quit", "Quit", "The tray keeps switching the sound")]); }
        else { items.extend([("|", "", ""), ("strip", "The Strip", "One thin row for the edge of a screen")]); }
        for (id, name, sub) in items {
            if id == "|" {
                col.append(&sep(name));
                continue;
            }
            let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(1).build();
            b.append(&gtk::Label::builder().label(name).xalign(0.0).css_classes(["mi-t"]).build());
            if !sub.is_empty() { b.append(&gtk::Label::builder().label(sub).xalign(0.0).css_classes(["mi-s"]).build()); }
            let btn = gtk::Button::builder().child(&b).css_classes(["flat"]).build();
            let wk = Rc::downgrade(self);
            let pw = pop.downgrade();
            btn.connect_clicked(move |_| {
                if let Some(p) = pw.upgrade() { p.popdown() }
                let Some(u) = wk.upgrade() else { return };
                match id {
                    "full" | "strip" | "min" | "quit" | "test" => u.page_action(id),
                    // A panel from the Strip: back to the full window first (a panel needs room).
                    _ => { if u.strip_on.get() { u.set_strip(false) } u.show_page(id) }
                }
            });
            col.append(&btn);
        }
        pop
    }

    /// Over your shoulder, mini's room: the artist's neon (songs), the LED wall
    /// (films, series, games) or the scoreboard (a match), the speakers round the ring.
    fn draw_shoulder(&self, cr: &gtk::cairo::Context, w: f64, h: f64) -> Vec<(&'static str, [f64; 4])> {
        use crate::mini::{MiniScene, Sign};
        let s = self.s.borrow().clone();
        let np = self.np.borrow().clone();
        let kind = self.kind_now().unwrap_or(Kind::Studio);
        let stream_ch = np.as_ref().and_then(|n| n.stream.as_ref()).map(|f| f.channels).unwrap_or(2);
        let st = self.mini_room_state();
        let fx = s.fx_of(&s.cur_profile());
        let spread = if s.sbx_active() && fx.surround.0 { fx.surround.1 as f64 / 100.0 } else { 0.3 };
        let tone_rgb = match s.tone { t if t <= -2 => (1.0, 0.416, 0.122), -1 => (1.0, 0.333, 0.212), 0 => (1.0, 0.231, 0.267), 1 => (1.0, 0.561, 0.596), _ => (1.0, 0.824, 0.839) };
        let tex = self.page.tex.borrow();
        let fresh = np.as_ref().is_some_and(|n| n.key() == tex.key);
        let artist = np.as_ref().map(|n| n.artist.clone()).unwrap_or_default();
        let title = np.as_ref().map(|n| n.title.clone()).unwrap_or_default();
        let sub = self.page.meta.borrow().as_ref().map(|m| m.1.subtitle.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| artist.clone());
        let score = teams(&title);
        let pic = if fresh && !tex.pics.is_empty() { Some(&tex.pics[self.page.mon_i.get() % tex.pics.len()]) } else { None };
        let sign = match kind {
            Kind::Studio | Kind::LiveSmall | Kind::LiveStadium =>
                Sign::Neon { logo: tex.neon.as_ref().filter(|_| fresh), text: if artist.is_empty() { &title } else { &artist } },
            Kind::Film | Kind::Tv | Kind::Game => Sign::Led { logo: tex.logo_s.as_ref().filter(|_| fresh), pic },
            Kind::Sport => match &score {
                Some((home, away)) => Sign::Score { home, away, sub: &sub },
                None => Sign::Led { logo: None, pic },
            },
            _ => Sign::None,
        };
        let sc = MiniScene {
            plays: st.plays, levels: st.levels, lit: st.lit.as_deref(), headphones: s.headphones(), spread,
            around: !s.headphones() && (stream_ch >= 6 || s.spreads()), tone_rgb, sign,
            playing: np.as_ref().is_some_and(|n| n.playing), t: self.t0.elapsed().as_secs_f64(),
        };
        crate::mini::mini_room(cr, w, h, &sc)
    }

    /// The room as the compact views draw it: smaller speakers, no margins.
    pub(crate) fn mini_room_state(&self) -> room::RoomState {
        let mut st = self.room_state();
        st.top = 4.0;
        st.bottom = 8.0;
        st.kmin = 0.36;
        st.kmax = 0.66;
        st.min_avail = 60.0;
        st
    }

    fn connect_minis(self: &Rc<Self>) {
        let st = self.stage.clone();
        // The Remote's room and the Strip's cabinets, drawn and animated like the page's room.
        {
            let wk = Rc::downgrade(self);
            // A wide, short room (the Remote in a wide window under 600 px tall)
            // shows the six speakers front on, as the Strip does: the plan from
            // above would shrink to a thin band.
            st.remote.room.set_draw_func(move |_, cr, w, h| if let Some(u) = wk.upgrade() {
                if remote_row(w as f64, h as f64) {
                    room::row(Some(cr), w as f64, h as f64, &u.mini_room_state());
                } else {
                    room::draw(cr, w as f64, h as f64, &u.mini_room_state());
                }
            });
            let wk = Rc::downgrade(self);
            st.strip.spk.set_draw_func(move |_, cr, w, h| if let Some(u) = wk.upgrade() {
                room::row(Some(cr), w as f64, h as f64, &u.mini_room_state());
            });
            for da in [st.remote.room.clone(), st.strip.spk.clone()] {
                let wk = Rc::downgrade(self);
                let last = Cell::new(0i64);
                da.add_tick_callback(move |da, clock| {
                    let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
                    let t = clock.frame_time();
                    let busy = u.levels.get().iter().any(|l| *l > 0.02) || u.room_lit.borrow().is_some();
                    if busy && da.is_mapped() && t - last.get() > 40_000 {
                        last.set(t);
                        da.queue_draw();
                    }
                    glib::ControlFlow::Continue
                });
            }
        }
        // Over your shoulder, mini: its room is drawn here (the sign at the back,
        // the speakers round the ring), animated while sound plays.
        {
            let wk = Rc::downgrade(self);
            st.shoulder.room.set_draw_func(move |_, cr, w, h| if let Some(u) = wk.upgrade() {
                let hits = u.draw_shoulder(cr, w as f64, h as f64);
                *u.page.mini_hits.borrow_mut() = hits;
            });
            let wk = Rc::downgrade(self);
            let last = Cell::new(0i64);
            st.shoulder.room.add_tick_callback(move |da, clock| {
                let Some(u) = wk.upgrade() else { return glib::ControlFlow::Break };
                let t = clock.frame_time();
                let busy = u.levels.get().iter().any(|l| *l > 0.02) || u.room_lit.borrow().is_some()
                    || u.np.borrow().as_ref().is_some_and(|n| n.playing);
                // The neon hums and the LED wall changes every few seconds: slowly when quiet.
                if da.is_mapped() && t - last.get() > if busy { 40_000 } else { 500_000 } {
                    last.set(t);
                    da.queue_draw();
                }
                glib::ControlFlow::Continue
            });
            let wk = Rc::downgrade(self);
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, x, y| if let Some(u) = wk.upgrade() {
                let hit = u.page.mini_hits.borrow().iter().find(|(_, b)| x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]).map(|h| h.0);
                if let Some(ch) = hit { u.test_speaker(ch) }
            });
            st.shoulder.room.add_controller(click);
        }
        // Tap a speaker to test it.
        {
            let wk = Rc::downgrade(self);
            let click = gtk::GestureClick::new();
            click.connect_released(move |g, _, x, y| if let Some(u) = wk.upgrade() {
                let da = g.widget().unwrap();
                let (w, h) = (da.width() as f64, da.height() as f64);
                let st = u.mini_room_state();
                let hit = if remote_row(w, h) {
                    room::row(None, w, h, &st).into_iter().find(|(_, b)| x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]).map(|(c, _)| c)
                } else {
                    room::hit(w, h, &st, x, y)
                };
                if let Some(ch) = hit { u.test_speaker(ch) }
            });
            st.remote.room.add_controller(click);
            let wk = Rc::downgrade(self);
            let click = gtk::GestureClick::new();
            click.connect_released(move |g, _, x, y| if let Some(u) = wk.upgrade() {
                let da = g.widget().unwrap();
                let boxes = room::row(None, da.width() as f64, da.height() as f64, &u.mini_room_state());
                if let Some((ch, _)) = boxes.iter().find(|(_, b)| x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3]) { u.test_speaker(ch) }
            });
            st.strip.spk.add_controller(click);
        }
        // The window's size picks what it shows.
        {
            let wk = Rc::downgrade(self);
            self.sizer.connect_resize(move |_, w, h| if let Some(u) = wk.upgrade() { u.on_window_size(w, h) });
        }
    }

    /// The page for a full-size window, the Remote for a small one (under
    /// 900 x 600, or the Compact theme), the Strip for a thin one (or when asked).
    pub fn on_window_size(self: &Rc<Self>, w: i32, h: i32) {
        self.size_speaker_test(w, h);
        let compact_theme = self.s.borrow().compact_always;
        let small = w < 900 || h < 600;
        let mini = self.s.borrow().compact_view == "mini";
        // Under 300 px tall the window is the Strip (the Remote can't be that
        // short, so dragging a window down ends there and can carry on to 76 px).
        let view = if self.strip_on.get() || h < 300 { "strip" } else if compact_theme || small { if mini { "mini" } else { "remote" } } else { "page" };
        if self.views.visible_child_name().as_deref() != Some(view) {
            self.views.set_visible_child_name(view);
        }
        // The window buttons are always there at the top right (hidden until
        // the pointer came near, they were hard to reach), except in the
        // Strip, which has its own.
        self.wctl.set_reveal_child(view != "strip");
        if view != "strip" { self.full_size.set((w, h)) }
        // Half a screen (Meta+Left on a 1920 screen, or any window 900–1200 wide):
        // the theme's own layout.
        let theme = self.s.borrow().theme.clone();
        let half = view == "page" && w < 1200 && !matches!(self.stage.layout(), Layout::Split | Layout::SplitSlim);
        if self.stage.set_half(half.then_some(theme.as_str())) {
            self.update_stage();
            self.stage.room.queue_draw();
        }
        self.side_for(view == "strip");
        self.stage.remote.resized(w, h);
        self.stage.shoulder.resized(w, h);
        if view == "strip" { self.stage.strip.resized(w, h) }
        self.size_panel(w, h, view);
        if self.page.auto_compact.get() != small {
            self.page.auto_compact.set(small);
            self.update_stage();
        }
    }

    /// The Strip has no room for the control centre: it closes (☰ there goes
    /// back to the full window first).
    fn side_for(&self, strip: bool) {
        if strip { self.close_panel() }
    }

    /// A narrow control centre (a small window, or the default 1240 px one)
    /// fits its width: the tab names shorten and the pages' side margins slim down.
    fn narrow_panel(&self, narrow: bool) {
        if self.panel_narrow.replace(narrow) == narrow { return; }
        if narrow { self.side_box.add_css_class("narrow") } else { self.side_box.remove_css_class("narrow") }
        for (id, b) in &self.cc_tabs {
            b.set_label(match (*id, narrow) {
                ("sound", _) => "Sound", ("mixer", _) => "Mixer", ("crystalvoice", true) => "Mic", ("crystalvoice", false) => "Microphone",
                ("setup", _) => "Speakers", ("look", true) => "Look", ("look", false) => "Look and feel", _ => "Settings",
            });
        }
        let m = if narrow { 12 } else { 18 };
        let mut c = self.side_stack.first_child();
        while let Some(sw) = c {
            if let Some(inner) = sw.downcast_ref::<gtk::ScrolledWindow>().and_then(|s| s.child()).and_then(|v| v.first_child()) {
                inner.set_margin_start(m);
                inner.set_margin_end(m);
            }
            c = sw.next_sibling();
        }
        if let Some(tb) = self.cc_tabs.first().and_then(|(_, b)| b.parent()) {
            tb.set_margin_start(if narrow { 8 } else { 14 });
            tb.set_margin_end(if narrow { 8 } else { 14 });
        }
        if let Some(u) = crate::ui::UI.with(|c| c.borrow().as_ref().and_then(|w| w.upgrade())) && self.current_page() == "look" {
            u.build_look_page();
        }
    }

    /// The control centre's size comes from the window only (never from what's
    /// in it): above the dock at the bottom right, or most of a small window,
    /// always below the window buttons.
    fn size_panel(&self, w: i32, h: i32, view: &str) {
        let page = view == "page";
        let (pw, ph, me, mb) = if page {
            (((w as f64 * 0.46).clamp(520.0, 660.0) as i32).min(w - 48), (h - 84 - 56).min(820), 24, 84)
        } else {
            (w - 24, h - 52 - 12, 12, 12)
        };
        self.side_box.set_size_request(pw.max(320), ph.max(300));
        // The full layout needs about 600 px (Sound's five knobs in a row).
        self.narrow_panel(pw < 600);
        self.cc_rev.set_margin_end(me);
        self.cc_rev.set_margin_bottom(mb);
    }

    /// The Strip: one thin row (1240 x 76) for the edge of a screen, kept above
    /// other windows if its pin is on; and back to the window it was.
    pub fn set_strip(self: &Rc<Self>, on: bool) {
        // A window dragged short is the Strip without asking: "Full window" makes it big again.
        let dragged = !on && self.views.visible_child_name().as_deref() == Some("strip");
        if on == self.strip_on.get() && !dragged { return; }
        let was_pinned = self.strip_on.get();
        self.strip_on.set(on);
        let pin = self.s.borrow().strip_pin;
        if on {
            self.views.set_visible_child_name("strip");
            self.side_for(true);
            self.wctl.set_reveal_child(false);
            self.win.unmaximize();
            self.win.set_default_size(1240, 76);
            if pin { crate::mini::pin_window(true, crate::APP_ID) }
        } else {
            let (w, h) = self.full_size.get();
            self.win.set_default_size(w.max(900), h.max(600));
            if pin && was_pinned { crate::mini::pin_window(false, crate::APP_ID) }
            self.on_window_size(w.max(900), h.max(600));
        }
    }
}
