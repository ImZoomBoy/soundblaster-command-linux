//! The tray icon, run by the background service so it's there even with the
//! window closed: switch the kind of content's sound, SBX profile, SBX/Pure, where stereo plays and the EQ
//! preset, or open the window. No keyboard shortcuts are registered.

use crate::model::{self, Settings};
use crate::worker::{apply, part};
use ksni::blocking::TrayMethods;
use ksni::menu::*;

pub struct Tray {
    pub s: Settings,
}

/// Saves, sends to the card and tells an open window to reload.
fn commit(s: &Settings, parts: u32, listen: bool) {
    let _ = s.save();
    let _ = apply(s, parts);
    if listen {
        crate::system::apply_listen(s);
    }
    tell_window_to_reload();
}

pub fn tell_window_to_reload() {
    let _ = crate::system::run("busctl", &["--user", "call", "com.local.sbcommand", "/com/local/sbcommand",
        "org.gtk.Actions", "Activate", "sava{sv}", "reload", "0", "0"]);
}

fn open_window() {
    // Waited for on its own thread, or each closed window stays behind as a
    // dead entry under the background service.
    if let Ok(exe) = std::env::current_exe() && let Ok(mut child) = std::process::Command::new(exe).spawn() {
        std::thread::spawn(move || { let _ = child.wait(); });
    }
}

impl Tray {
    fn summary(&self) -> String {
        let s = &self.s;
        let sound = if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "Pure".into() };
        let spk = if s.headphones() { "Headphones".to_string() } else { s.speaker_config.clone() };
        let kind = if s.kind.is_empty() { model::activity_label(&s.activity) } else { model::kind_name(&s.kind) };
        format!("{kind} · {sound} · {spk}")
    }

    /// A change you make: kept as the current kind of content's own sound too.
    fn edit(&mut self, parts: u32, listen: bool, f: impl FnOnce(&mut Settings)) {
        let mut s = Settings::load();
        f(&mut s);
        s.remember_kind();
        commit(&s, parts, listen);
        self.s = s;
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "sbcommand".into()
    }
    fn title(&self) -> String {
        "Sound Blaster Command".into()
    }
    fn icon_name(&self) -> String {
        crate::ICON.into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Sound Blaster Command".into(), description: self.summary(), ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        open_window();
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let s = &self.s;
        // Each kind of content's own sound (as in the window's Sound tab).
        let kind = if s.kind.is_empty() { match s.activity.as_str() { "game" => "game", "movie" => "film", _ => "music" } } else { s.kind.as_str() };
        let styles: Vec<String> = model::styles_for(&s.activity).iter().map(|x| x.to_string()).collect();
        let eqs = s.eq_list();
        let cur_style = s.cur_profile();
        let cur_eq = s.cur_eq();
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem { label: self.summary(), enabled: false, ..Default::default() }.into(),
            MenuItem::Separator,
            RadioGroup {
                selected: model::KINDS.iter().position(|k| k.0 == kind).unwrap_or(0),
                select: Box::new(|t: &mut Self, i| {
                    let k = model::KINDS[i].0;
                    let mut s = Settings::load();
                    s.set_kind(k);
                    commit(&s, part::FX | part::EQ | part::SPEAKER, true);
                    t.s = s;
                }),
                options: model::KINDS.iter().map(|k| RadioItem { label: k.1.into(), ..Default::default() }).collect(),
            }.into(),
            MenuItem::Separator,
            RadioGroup {
                selected: usize::from(!s.sbx_enabled),
                select: Box::new(|t: &mut Self, i| t.edit(part::FX | part::EQ, false, |s| s.sbx_enabled = i == 0)),
                options: vec![RadioItem { label: "SBX".into(), ..Default::default() },
                              RadioItem { label: "Pure (no SBX effects)".into(), ..Default::default() }],
            }.into(),
            SubMenu {
                label: format!("SBX profile: {cur_style}"),
                enabled: s.sbx_active(),
                submenu: vec![RadioGroup {
                    selected: styles.iter().position(|x| *x == cur_style).unwrap_or(usize::MAX),
                    select: Box::new(move |t: &mut Self, i| {
                        let name = model::styles_for(&t.s.activity)[i].to_string();
                        t.edit(part::FX | part::EQ, false, |s| s.set_style(&name));
                    }),
                    options: styles.iter().map(|n| RadioItem { label: n.clone(), ..Default::default() }).collect(),
                }.into()],
                ..Default::default()
            }.into(),
            SubMenu {
                label: format!("Equaliser: {cur_eq}"),
                enabled: s.sbx_active(),
                submenu: vec![RadioGroup {
                    selected: eqs.iter().position(|x| *x == cur_eq).unwrap_or(usize::MAX),
                    select: Box::new(|t: &mut Self, i| {
                        let name = t.s.eq_list().get(i).cloned();
                        if let Some(n) = name {
                            t.edit(part::EQ, false, |s| s.set_eq(&n));
                        }
                    }),
                    options: eqs.iter().map(|n| RadioItem { label: n.clone(), ..Default::default() }).collect(),
                }.into()],
                ..Default::default()
            }.into(),
        ];
        if !s.headphones() {
            items.push(MenuItem::Separator);
            if model::channels(&s.speaker_config).contains(&"RL") {
                items.push(SubMenu {
                    label: format!("Stereo plays on: {}", model::STEREO_ON.iter().find(|x| x.0 == s.stereo_on).map_or("Fronts", |x| x.1)),
                    submenu: vec![RadioGroup {
                        selected: model::STEREO_ON.iter().position(|x| x.0 == s.stereo_on).unwrap_or(0),
                        select: Box::new(|t: &mut Self, i| t.edit(0, true, |s| s.stereo_on = model::STEREO_ON[i].0.into())),
                        // From the same list as the window, so they can't disagree.
                        options: model::STEREO_ON.iter()
                            .map(|(_, l, _)| RadioItem { label: l.to_string(), ..Default::default() }).collect(),
                    }.into()],
                    ..Default::default()
                }.into());
            }
        }
        items.push(MenuItem::Separator);
        items.push(StandardItem {
            label: "Open Sound Blaster Command".into(),
            icon_name: crate::ICON.into(),
            activate: Box::new(|_| open_window()),
            ..Default::default()
        }.into());
        items
    }
}

/// Starts the tray icon; keep the handle to update it.
pub fn start() -> Option<ksni::blocking::Handle<Tray>> {
    if crate::card::dry_run() {
        return None;
    }
    Tray { s: Settings::load() }.spawn().map_err(|e| eprintln!("sbcommand: no tray icon ({e})")).ok()
}
