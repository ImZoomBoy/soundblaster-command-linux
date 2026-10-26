//! Direct access to the Sound Blaster's mixer controls through ALSA.

use crate::model::{self, Settings, V};
use alsa::ctl::{ElemIface, ElemType, ElemValue};
use alsa::hctl::HCtl;
use alsa::mixer::{MilliBel, Mixer, SelemChannelId, SelemId};
use alsa::Round;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;

pub const CARD_ID: &str = "Creative";
const HW: &str = "hw:Creative";

pub fn dry_run() -> bool {
    std::env::var_os("SBCOMMAND_DRYRUN").is_some()
}

pub fn present() -> bool {
    dry_run() || std::path::Path::new(&format!("/proc/asound/{CARD_ID}")).exists()
}

/// Changes whenever the driver re-creates the card (start-up, wake, reset).
pub fn signature() -> Option<(String, u64, i64)> {
    let link = std::fs::read_link(format!("/proc/asound/{CARD_ID}")).ok()?;
    let idx = link.to_string_lossy().trim_start_matches("card").to_string();
    let m = std::fs::metadata(format!("/dev/snd/controlC{idx}")).ok()?;
    Some((idx, m.ino(), m.ctime_nsec() + m.ctime() * 1_000_000_000))
}

/// The card's PCI address, e.g. "0000:67:00.0".
pub fn pci_addr() -> Option<String> {
    let base = std::path::Path::new("/sys/bus/pci/devices");
    let mut entries: Vec<_> = std::fs::read_dir(base).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let read = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default();
        if read("vendor").trim() == "0x1102" && read("device").trim() == "0x0012" {
            return Some(e.file_name().to_string_lossy().into_owned());
        }
    }
    None
}

/// Writes control values. Unknown controls are skipped, so other CA0132 cards
/// (which lack some ZxR controls) still work.
pub fn set(pairs: &[(&str, V)]) -> Result<(), String> {
    if pairs.is_empty() {
        return Ok(());
    }
    if dry_run() {
        for (n, v) in pairs {
            eprintln!("dry-run: {n} = {v:?}");
        }
        return Ok(());
    }
    let h = HCtl::new(HW, false).map_err(|e| format!("open card: {e}"))?;
    h.load().map_err(|e| format!("read card controls: {e}"))?;
    let mut elems = HashMap::new();
    for e in h.elem_iter() {
        if let Ok(id) = e.get_id()
            && id.get_interface() == ElemIface::Mixer
                && let Ok(name) = id.get_name() {
                    elems.insert(name.to_string(), e);
                }
    }
    let mut errors = Vec::new();
    for (name, v) in pairs {
        let Some(e) = elems.get(*name) else { continue };
        let res = (|| -> Result<(), String> {
            let info = e.info().map_err(|x| x.to_string())?;
            let ty = info.get_type();
            let mut val = ElemValue::new(ty).map_err(|x| x.to_string())?;
            for i in 0..info.get_count() {
                match (ty, v) {
                    (ElemType::Boolean, V::B(b)) => val.set_boolean(i, *b),
                    (ElemType::Integer, V::I(n)) => val.set_integer(i, *n),
                    (ElemType::Enumerated, V::E(n)) => val.set_enumerated(i, *n),
                    _ => return Err("wrong value type".into()),
                }
                .ok_or("value index out of range")?;
            }
            e.write(&val).map_err(|x| x.to_string())?;
            Ok(())
        })();
        if let Err(msg) = res {
            errors.push(format!("{name}: {msg}"));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

/// Keeps the card's own volume sliders at or below 0 dB, with per-speaker trims.
/// PipeWire handles the actual volume, so the card never adds digital boost.
pub fn set_volumes(s: &Settings) -> Result<(), String> {
    if !s.safe_volume {
        return Ok(());
    }
    let mb = |db: f64| MilliBel((db * 100.0).round() as i64);
    if dry_run() {
        eprintln!("dry-run: volumes {:?}", s.trim);
        return Ok(());
    }
    let mixer = Mixer::new(HW, false).map_err(|e| format!("open mixer: {e}"))?;
    let mut errors = Vec::new();
    let mut each = |name: &str, f: &dyn Fn(&alsa::mixer::Selem) -> alsa::Result<()>| {
        if let Some(sel) = mixer.find_selem(&SelemId::new(name, 0))
            && let Err(e) = f(&sel) {
                errors.push(format!("{name}: {e}"));
            }
    };
    each("Master", &|e| e.set_playback_db_all(mb(0.0), Round::Floor));
    each("PCM", &|e| e.set_playback_db_all(mb(0.0), Round::Floor));
    let fl = model::channel_db(s, "FL");
    let fr = model::channel_db(s, "FR");
    let rl = model::channel_db(s, "RL");
    let rr = model::channel_db(s, "RR");
    each("Front", &|e| {
        e.set_playback_db(SelemChannelId::FrontLeft, mb(fl), Round::Floor)?;
        e.set_playback_db(SelemChannelId::FrontRight, mb(fr), Round::Floor)
    });
    each("Surround", &|e| {
        e.set_playback_db(SelemChannelId::FrontLeft, mb(rl), Round::Floor)?;
        e.set_playback_db(SelemChannelId::FrontRight, mb(rr), Round::Floor)
    });
    let fc = model::channel_db(s, "FC");
    let lfe = model::channel_db(s, "LFE");
    each("Center", &|e| e.set_playback_db_all(mb(fc), Round::Floor));
    each("LFE", &|e| e.set_playback_db_all(mb(lfe), Round::Floor));
    drop(each);
    set(&[
        ("Master Playback Switch", V::B(true)),
        ("Front Playback Switch", V::B(true)),
        ("Surround Playback Switch", V::B(true)),
        ("Center Playback Switch", V::B(true)),
        ("LFE Playback Switch", V::B(true)),
    ])?;
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}
