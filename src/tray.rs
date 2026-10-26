//! The tray icon, run by the background service so it's there even with the
//! window closed: switch activity, SBX profile, SBX/Pure, 5.1/2.1 and the EQ
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
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
}

impl Tray {
    fn summary(&self) -> String {
        let s = &self.s;
        let sound = if s.sbx_active() { format!("SBX {}", s.cur_profile()) } else { "Pure".into() };
        let spk = if s.headphones() { "Headphones".to_string() } else if s.listen == "21" { "2.1".into() } else { "5.1".into() };
        format!("{} · {sound} · {spk}", model::activity_label(&s.activity))
    }

    fn edit(&mut self, parts: u32, listen: bool, f: impl FnOnce(&mut Settings)) {
        let mut s = Settings::load();
        f(&mut s);
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
        crate::APP_ID.into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Sound Blaster Command".into(), description: self.summary(), ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        open_window();
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let s = &self.s;
        let acts: Vec<&str> = model::ACTIVITIES.iter().map(|a| a.0).collect();
        let styles: Vec<String> = model::styles_for(&s.activity).iter().map(|x| x.to_string()).collect();
        let eqs = s.eq_list();
        let cur_style = s.cur_profile();
        let cur_eq = s.cur_eq();
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem { label: self.summary(), enabled: false, ..Default::default() }.into(),
            MenuItem::Separator,
            RadioGroup {
                selected: acts.iter().position(|a| *a == s.activity).unwrap_or(2),
                select: Box::new(|t: &mut Self, i| {
                    let a = model::ACTIVITIES[i].0;
                    t.edit(part::FX | part::EQ | part::SPEAKER, false, |s| s.set_activity(a));
                }),
                options: model::ACTIVITIES.iter().map(|a| RadioItem { label: a.1.into(), ..Default::default() }).collect(),
            }.into(),
            MenuItem::Separator,
            RadioGroup {
                selected: usize::from(!s.sbx_enabled),
                select: Box::new(|t: &mut Self, i| t.edit(part::FX | part::EQ, false, |s| s.sbx_enabled = i == 0)),
                options: vec![RadioItem { label: "SBX".into(), ..Default::default() },
                              RadioItem { label: "Pure (no effects, no EQ)".into(), ..Default::default() }],
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
            items.push(RadioGroup {
                selected: usize::from(s.listen == "21"),
                select: Box::new(|t: &mut Self, i| t.edit(part::FX, true, |s| s.listen = if i == 0 { "51" } else { "21" }.into())),
                options: vec![RadioItem { label: "5.1 · all six speakers".into(), ..Default::default() },
                              RadioItem { label: "2.1 · fronts and subwoofer".into(), ..Default::default() }],
            }.into());
            if s.listen != "21" && model::channels(&s.speaker_config).contains(&"RL") {
                items.push(SubMenu {
                    label: format!("Stereo plays on: {}", model::STEREO_ON.iter().find(|x| x.0 == s.stereo_on).map_or("Fronts", |x| x.1)),
                    submenu: vec![RadioGroup {
                        selected: model::STEREO_ON.iter().position(|x| x.0 == s.stereo_on).unwrap_or(0),
                        select: Box::new(|t: &mut Self, i| t.edit(0, true, |s| s.stereo_on = model::STEREO_ON[i].0.into())),
                        options: [("Front speakers", ""), ("Fronts + rears", ""), ("All speakers", "")].iter()
                            .map(|(l, _)| RadioItem { label: l.to_string(), ..Default::default() }).collect(),
                    }.into()],
                    ..Default::default()
                }.into());
            }
        }
        items.push(MenuItem::Separator);
        items.push(StandardItem {
            label: "Open Sound Blaster Command".into(),
            icon_name: crate::APP_ID.into(),
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
