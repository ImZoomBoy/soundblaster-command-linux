//! PipeWire / WirePlumber / systemd helpers. Everything here may block, so it
//! only runs on the worker thread or in the `apply` / `watch` commands.

use crate::card;
use crate::model::Settings;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn wp_conf() -> PathBuf {
    home().join(".config/wireplumber/wireplumber.conf.d/51-sbcommand.conf")
}

fn upmix_confs() -> [PathBuf; 2] {
    [
        home().join(".config/pipewire/pipewire-pulse.conf.d/51-sbcommand-upmix.conf"),
        home().join(".config/pipewire/client.conf.d/51-sbcommand-upmix.conf"),
    ]
}

pub fn cache_dir() -> PathBuf {
    home().join(".cache/sbcommand")
}

/// Runs a command with a time limit so a stuck tool can't hang the worker.
pub(crate) fn run(cmd: &str, args: &[&str]) -> Option<String> {
    if card::dry_run() {
        eprintln!("dry-run: {cmd} {}", args.join(" "));
        return Some(String::new());
    }
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read the output while the command runs. Waiting first would deadlock
    // on output bigger than the pipe (pw-dump easily is).
    let mut so = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut out = String::new();
        let _ = so.read_to_string(&mut out);
        out
    });
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().unwrap_or_default();
                return status.success().then_some(out);
            }
            Ok(None) if start.elapsed() > Duration::from_secs(8) => {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("sbcommand: {cmd} timed out");
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => return None,
        }
    }
}

pub(crate) fn pactl_json(what: &str) -> Vec<Value> {
    run("pactl", &["-f", "json", "list", what])
        .and_then(|o| serde_json::from_str::<Value>(&o).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

pub(crate) fn pci_tag() -> Option<String> {
    card::pci_addr().map(|a| a.replace(':', "_"))
}

pub fn sink_name(s: &Settings) -> Option<String> {
    pci_tag().map(|t| format!("alsa_output.pci-{t}.{}", s.pw_sink_suffix()))
}

/// Puts PipeWire on the right card profile (speaker layout) and output port.
pub fn apply_pipewire(s: &Settings) {
    let Some(tag) = pci_tag() else { return };
    if card::dry_run() {
        eprintln!("dry-run: pipewire profile {}", s.pw_profile());
        return;
    }
    let card = format!("alsa_card.pci-{tag}");
    let want = s.pw_profile();
    for c in pactl_json("cards") {
        if c["name"] == card.as_str() && c["active_profile"] != want.as_str() {
            run("pactl", &["set-card-profile", &card, &want]);
        }
    }
    let Some(sink) = sink_name(s) else { return };
    let word = if s.headphones() { "headphones" } else { "speaker" };
    for k in pactl_json("sinks") {
        if k["name"] != sink.as_str() {
            continue;
        }
        if let Some(ports) = k["ports"].as_array() {
            for p in ports {
                let name = p["name"].as_str().unwrap_or_default();
                if name.contains(word) && k["active_port"] != name {
                    run("pactl", &["set-sink-port", &sink, name]);
                }
            }
        }
    }
}

fn write_if_changed(path: &PathBuf, text: &str) -> bool {
    if card::dry_run() {
        return false;
    }
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return false;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, text).is_ok()
}

/// Software volume for this card (so the card's sliders never boost), the
/// 5.1 channel order, and a clear name in the sound menu. Returns true if the
/// file changed (WirePlumber must restart).
pub fn write_wireplumber_conf(s: &Settings) -> bool {
    let Some(tag) = pci_tag() else { return false };
    let mut props = String::from("node.description = \"Sound Blaster – 5.1 (all speakers)\"");
    if let Some(pos) = &s.audio_position {
        props += &format!(" audio.position = \"{}\"", pos.join(","));
    }
    let text = format!(
        "# Written by Sound Blaster Command (sbcommand). Changes here are overwritten.\n\
         monitor.alsa.rules = [\n  {{\n    matches = [ {{ device.name = \"alsa_card.pci-{tag}\" }} ]\n    \
         actions = {{ update-props = {{ api.alsa.soft-mixer = {} }} }}\n  }}\n  {{\n    \
         matches = [ {{ node.name = \"alsa_output.pci-{tag}.analog-surround-51\" }} ]\n    \
         actions = {{ update-props = {{ {props} }} }}\n  }}\n]\n",
        s.safe_volume
    );
    write_if_changed(&wp_conf(), &text)
}

/// Whether stereo plays on every speaker (PipeWire upmixes it before the
/// card's SBX processing, as it did when the SBX profiles sounded best).
/// Returns true if the files changed (pipewire-pulse must restart).
pub fn write_upmix_conf(s: &Settings) -> bool {
    // Same bytes as earlier versions, so nothing restarts needlessly.
    let text = if s.upmix {
        "# Written by Sound Blaster Command (sbcommand): play stereo on every speaker.\n\
         stream.properties = {\n    channelmix.upmix        = true\n    channelmix.upmix-method = psd\n    \
         channelmix.fc-cutoff    = 12000\n    channelmix.lfe-cutoff   = 0\n    channelmix.rear-delay   = 12.0\n}\n"
    } else {
        "# Written by Sound Blaster Command (sbcommand): stereo plays on the front speakers only.\n\
         stream.properties = {\n    channelmix.upmix = false\n}\n"
    };
    let mut changed = false;
    for p in upmix_confs() {
        changed |= write_if_changed(&p, text);
    }
    changed
}

pub fn restart(wireplumber: bool, pulse: bool) {
    let mut units = vec![];
    if wireplumber {
        units.push("wireplumber");
    }
    if pulse {
        units.push("pipewire-pulse");
    }
    if !units.is_empty() {
        let mut args = vec!["--user", "restart"];
        args.extend(units);
        run("systemctl", &args);
        // Give WirePlumber time to bring the card's sinks back.
        std::thread::sleep(Duration::from_millis(2500));
    }
}

pub fn reset_card() -> bool {
    run("systemctl", &["start", "--no-ask-password", "soundblaster-reset-now.service"]).is_some()
}

pub fn play_test(s: &Settings, ch: &str) {
    let path = if ch == "LFE" {
        let p = cache_dir().join("sub-rumble.wav");
        if !p.exists() {
            let _ = make_rumble(&p);
        }
        p
    } else {
        let f = match ch {
            "FL" => "Front_Left.wav",
            "FR" => "Front_Right.wav",
            "RL" => "Rear_Left.wav",
            "RR" => "Rear_Right.wav",
            _ => "Front_Center.wav",
        };
        PathBuf::from("/usr/share/sounds/alsa").join(f)
    };
    if card::dry_run() {
        eprintln!("dry-run: play {ch}");
        return;
    }
    // A one-channel sound lets PipeWire guess: a mono "centre" was copied to
    // the fronts until the stream settings changed mid-play, and it cut out.
    // So the test plays every channel of the output, silent except this one.
    let layout: &[&str] = if s.headphones() { &["FL", "FR"] } else { &["FL", "FR", "FC", "LFE", "RL", "RR"] };
    let out = cache_dir().join(format!("test-{}-{ch}.wav", layout.len()));
    let file = match spread_to_channel(&path, layout, ch, &out) {
        Ok(()) => out,
        Err(e) => {
            eprintln!("sbcommand: test sound: {e}");
            return;
        }
    };
    let mut cmd = Command::new("pw-play");
    cmd.args(["--channels", &layout.len().to_string(), "--channel-map", &layout.join(",")]);
    // Named like our own outputs, so the stereo settings leave it alone.
    cmd.args(["--properties", "{ node.name = \"sbcommand.test\" media.role = \"Test\" }"]);
    if let Some(sink) = sink_name(s) {
        cmd.args(["--target", &sink]);
    }
    // A new test stops the one still playing, so they never overlap.
    static PLAYING: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);
    let mut playing = PLAYING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut old) = playing.take() {
        let _ = old.kill();
        let _ = old.wait();
    }
    *playing = cmd.arg(file).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok();
}

/// Writes `src` (16-bit mono WAV) into one channel of a multichannel WAV.
pub fn spread_to_channel(src: &PathBuf, layout: &[&str], ch: &str, dst: &PathBuf) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let bad = |m: &str| Error::new(ErrorKind::InvalidData, format!("{}: {m}", src.display()));
    let slot = layout.iter().position(|c| *c == ch).ok_or_else(|| bad("channel not in layout"))?;
    let d = std::fs::read(src)?;
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let (mut rate, mut fmt_ok, mut data) = (0u32, false, None);
    let mut i = 12;
    while i + 8 <= d.len() {
        let id = &d[i..i + 4];
        let len = u32::from_le_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        let body = &d[i + 8..(i + 8 + len).min(d.len())];
        if id == b"fmt " && body.len() >= 16 {
            let chans = u16::from_le_bytes([body[2], body[3]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            fmt_ok = chans == 1 && bits == 16;
        } else if id == b"data" {
            data = Some(body);
        }
        i += 8 + len + (len & 1);
    }
    let data = data.filter(|_| fmt_ok).ok_or_else(|| bad("needs 16-bit mono"))?;
    // The stock voices peak around -6 dBFS and are hard to hear at normal
    // speaker volume, so bring each test up to -1 dBFS (at most 4x).
    let samples: Vec<i16> = data.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]])).collect();
    let peak = samples.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0).max(1) as f64;
    let gain = (32767.0 * 0.891 / peak).min(4.0);
    let n = layout.len();
    let mut pcm = vec![0u8; samples.len() * n * 2];
    for (k, v) in samples.iter().enumerate() {
        let o = (k * n + slot) * 2;
        let x = (*v as f64 * gain).round().clamp(-32767.0, 32767.0) as i16;
        pcm[o..o + 2].copy_from_slice(&x.to_le_bytes());
    }
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(n as u16).to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * n as u32 * 2).to_le_bytes());
    wav.extend_from_slice(&(n as u16 * 2).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(dst, wav)
}

/// A rumble (50/70/90 Hz plus low noise) that any PC subwoofer can play.
/// A pure 45 Hz tone was too deep for the user's subwoofer.
pub fn make_rumble(path: &PathBuf) -> std::io::Result<()> {
    let rate = 48_000u32;
    let n = (2.5 * rate as f64) as usize;
    let mut data = Vec::with_capacity(n * 2);
    let (mut y, mut seed) = (0.0f64, 1u64);
    for i in 0..n {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = ((seed >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0;
        y = 0.97 * y + 0.03 * r;
        let t = i as f64 / rate as f64;
        let tone: f64 = [50.0, 70.0, 90.0].iter().map(|f| (2.0 * std::f64::consts::PI * f * t).sin()).sum();
        let env = (i as f64 / 2400.0).min((n - i) as f64 / 2400.0).min(1.0);
        let v = ((0.25 * tone + 3.0 * y) * 0.8 * env).clamp(-1.0, 1.0);
        data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, wav)
}

// ------------------------------------------------------------------ extra outputs

pub const OUT_STEREO: &str = "sbcommand.stereo";
pub const OUT_21: &str = "sbcommand.2_1";
pub const OUT_MAIN: &str = "sbcommand.main";
/// Apps whose stereo isn't labelled left/right (the Spotify Flatpak sends
/// "aux0, aux1") are passed through here, so they can be spread too.
pub const OUT_RELABEL: &str = "sbcommand.relabel";
pub const MIC_CLEAN: &str = "sbcommand.mic";

pub fn mic_name() -> Option<String> {
    pci_tag().map(|t| format!("alsa_input.pci-{t}.analog-stereo"))
}

/// Whether the outputs process is needed at all.
pub fn outputs_needed(s: &Settings) -> bool {
    s.extra_outputs || s.distance_on || s.echo_cancel || s.mic_monitor || s.upmix
}

fn outputs_conf_path() -> PathBuf {
    crate::model::config_dir().join("pipewire-outputs.conf")
}

fn unit_path() -> PathBuf {
    home().join(".config/systemd/user/sbcommand-outputs.service")
}

/// PipeWire config for the extra outputs. It runs in its own small PipeWire
/// process (sbcommand-outputs.service), so changing it never restarts the
/// main audio system.
pub fn outputs_conf(s: &Settings) -> String {
    let target = sink_name(s).unwrap_or_default();
    let mic = mic_name().unwrap_or_default();
    let mut mods = String::new();
    if s.distance_on {
        let delays = crate::model::channel_delays(s);
        let mut nodes = String::new();
        let mut ins = String::new();
        let mut outs = String::new();
        for (ch, d) in &delays {
            nodes += &format!(
                "          {{ type = builtin name = d{ch} label = delay config = {{ \"max-delay\" = 0.05 }} control = {{ \"Delay (s)\" = {d:.6} }} }}\n"
            );
            ins += &format!(" \"d{ch}:In\"");
            outs += &format!(" \"d{ch}:Out\"");
        }
        let pos = crate::model::ALL_CHANNELS.join(" ");
        mods += &format!(r#"
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "Sound Blaster – 5.1 (distance corrected)"
      filter.graph = {{
        nodes = [
{nodes}        ]
        inputs = [{ins} ]
        outputs = [{outs} ]
      }}
      capture.props = {{ node.name = "{OUT_MAIN}" media.class = Audio/Sink audio.position = [ {pos} ] }}
      playback.props = {{ node.name = "{OUT_MAIN}.out" audio.position = [ {pos} ] stream.dont-remix = true
                         node.passive = true target.object = "{target}" }}
    }} }}"#);
    }
    if s.echo_cancel {
        mods += &format!(r#"
  {{ name = libpipewire-module-echo-cancel
    args = {{
      library.name = aec/libspa-aec-webrtc
      monitor.mode = true
      aec.args = {{ webrtc.noise_suppression = true webrtc.gain_control = true webrtc.extended_filter = true }}
      capture.props = {{ node.name = "{MIC_CLEAN}.capture" target.object = "{mic}" node.passive = true }}
      source.props = {{ node.name = "{MIC_CLEAN}" node.description = "Sound Blaster – Microphone (clean)" }}
    }} }}"#);
    }
    if s.mic_monitor {
        mods += &format!(r#"
  {{ name = libpipewire-module-loopback
    args = {{
      node.description = "Sound Blaster – microphone monitor"
      capture.props = {{ node.name = "sbcommand.monitor.in" target.object = "{mic}" node.dont-reconnect = true }}
      playback.props = {{ node.name = "sbcommand.monitor.out" target.object = "{target}" node.passive = true }}
    }} }}"#);
    }
    let relabel_target = if s.distance_on && !s.headphones() { OUT_MAIN.to_string() } else { target.clone() };
    // Spreading is set live per stream (spread_streams); keep this text as it
    // was so the outputs process isn't restarted.
    let spread = false;
    mods += &format!(r#"
  {{ name = libpipewire-module-loopback
    args = {{
      node.description = "Sound Blaster – stereo for Spotify"
      capture.props = {{ node.name = "{OUT_RELABEL}" media.class = Audio/Sink audio.position = [ FL FR ] priority.session = 50 }}
      playback.props = {{ node.name = "{OUT_RELABEL}.out" audio.position = [ FL FR ] node.passive = true
                         channelmix.upmix = {spread} channelmix.upmix-method = psd channelmix.fc-cutoff = 12000
                         channelmix.rear-delay = 12.0 channelmix.lfe-cutoff = 0 target.object = "{relabel_target}" }}
    }} }}"#);
    if !s.extra_outputs {
        return wrap_conf(&mods);
    }
    mods += &format!(r#"
  {{ name = libpipewire-module-loopback
    args = {{
      node.description = "Sound Blaster – Stereo (front speakers)"
      capture.props = {{ node.name = "{OUT_STEREO}" media.class = Audio/Sink audio.position = [ FL FR ] priority.session = 100 }}
      playback.props = {{ node.name = "{OUT_STEREO}.out" audio.position = [ FL FR ] stream.dont-remix = true
                         node.passive = true target.object = "{target}" }}
    }} }}
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "Sound Blaster – 2.1 (front + subwoofer)"
      filter.graph = {{
        nodes = [
          {{ type = builtin name = inL label = copy }}
          {{ type = builtin name = inR label = copy }}
          {{ type = builtin name = inLFE label = copy }}
          {{ type = builtin name = outL label = copy }}
          {{ type = builtin name = outR label = copy }}
          {{ type = builtin name = sum label = mixer control = {{ "Gain 1" = 0.5 "Gain 2" = 0.5 }} }}
          {{ type = builtin name = lp1 label = bq_lowpass control = {{ "Freq" = 120.0 }} }}
          {{ type = builtin name = lp2 label = bq_lowpass control = {{ "Freq" = 120.0 }} }}
          {{ type = builtin name = sub label = mixer control = {{ "Gain 1" = 1.0 "Gain 2" = 1.0 }} }}
        ]
        links = [
          {{ output = "inL:Out" input = "outL:In" }} {{ output = "inR:Out" input = "outR:In" }}
          {{ output = "inL:Out" input = "sum:In 1" }} {{ output = "inR:Out" input = "sum:In 2" }}
          {{ output = "sum:Out" input = "lp1:In" }} {{ output = "lp1:Out" input = "lp2:In" }}
          {{ output = "lp2:Out" input = "sub:In 1" }} {{ output = "inLFE:Out" input = "sub:In 2" }}
        ]
        inputs = [ "inL:In" "inR:In" "inLFE:In" ]
        outputs = [ "outL:Out" "outR:Out" "sub:Out" ]
      }}
      capture.props = {{ node.name = "{OUT_21}" media.class = Audio/Sink audio.position = [ FL FR LFE ] priority.session = 100 }}
      playback.props = {{ node.name = "{OUT_21}.out" audio.position = [ FL FR LFE ] stream.dont-remix = true
                         node.passive = true target.object = "{target}" }}
    }} }}"#);
    wrap_conf(&mods)
}

fn wrap_conf(mods: &str) -> String {
    format!(
        "# Written by Sound Blaster Command (sbcommand). Changes here are overwritten.\n\
         context.properties = {{ log.level = 2 }}\n\
         context.spa-libs = {{ audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }}\n\
         context.modules = [\n  \
         {{ name = libpipewire-module-rt args = {{ }} flags = [ ifexists nofail ] }}\n  \
         {{ name = libpipewire-module-protocol-native }}\n  {{ name = libpipewire-module-client-node }}\n  \
         {{ name = libpipewire-module-adapter }}{mods}\n]\n"
    )
}

const UNIT: &str = "[Unit]
Description=Sound Blaster Command: extra outputs (Stereo, 2.1, Enhanced)
After=pipewire.service wireplumber.service
PartOf=pipewire.service

[Service]
ExecStart=/usr/bin/pipewire -c %h/.config/sbcommand/pipewire-outputs.conf
Restart=on-failure
RestartSec=3

[Install]
WantedBy=pipewire.service
";

fn systemctl_user(args: &[&str]) -> bool {
    let mut a = vec!["--user"];
    a.extend_from_slice(args);
    run("systemctl", &a).is_some()
}

/// Starts, updates or stops the extra outputs to match the settings.
pub fn apply_outputs(s: &Settings) {
    if card::dry_run() {
        eprintln!("dry-run: extra outputs {}", if s.extra_outputs { "on" } else { "off" });
        return;
    }
    if !outputs_needed(s) {
        systemctl_user(&["disable", "--now", "sbcommand-outputs.service"]);
        fix_defaults(s);
        return;
    }
    let unit_changed = write_if_changed(&unit_path(), UNIT);
    if unit_changed {
        systemctl_user(&["daemon-reload"]);
        systemctl_user(&["enable", "sbcommand-outputs.service"]);
    }
    let conf_changed = write_if_changed(&outputs_conf_path(), &outputs_conf(s));
    let active = run("systemctl", &["--user", "is-active", "sbcommand-outputs.service"])
        .is_some_and(|o| o.trim() == "active");
    if conf_changed || unit_changed || !active {
        systemctl_user(&["enable", "sbcommand-outputs.service"]);
        systemctl_user(&["restart", "sbcommand-outputs.service"]);
        // Wait for the new nodes before choosing defaults.
        for _ in 0..30 {
            let want_main = (!s.distance_on || node_id(OUT_MAIN).is_some()) && node_id(OUT_RELABEL).is_some();
            let want_mic = !s.echo_cancel || node_id(MIC_CLEAN).is_some();
            if want_main && want_mic {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fix_defaults(s);
    // Restarted outputs come back with default settings; re-apply ours.
    relabel_aux_streams();
    spread_streams(s.spread_mode());
}

fn default_of(kind: &str) -> String {
    run("pactl", &[if kind == "sink" { "get-default-sink" } else { "get-default-source" }])
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Distance correction and the clean microphone only help if they're the
/// defaults; when switched off, hand the defaults back to the card.
fn fix_defaults(s: &Settings) {
    let sink = default_of("sink");
    let hw = sink_name(s).unwrap_or_default();
    let want = if s.listen == "21" && !s.headphones() && node_id(OUT_STEREO).is_some() {
        // True 2.1: front left/right only; the card's bass redirection feeds the subwoofer.
        OUT_STEREO.to_string()
    } else if s.distance_on && !s.headphones() {
        OUT_MAIN.to_string()
    } else {
        hw.clone()
    };
    // Only take over from our own outputs or the card, never from e.g. HDMI.
    let ours = sink.is_empty() || sink == OUT_MAIN || sink == OUT_STEREO || sink == "sbcommand.fill"
        || sink.starts_with(&format!("alsa_output.pci-{}", pci_tag().unwrap_or_default()));
    if ours && sink != want && !want.is_empty() {
        // Carry the volume over, so switching mode doesn't change loudness.
        let vol = run("pactl", &["get-sink-volume", &sink]).and_then(|o| {
            o.split('/').nth(1).map(|p| p.trim().to_string())
        });
        run("pactl", &["set-default-sink", &want]);
        if let Some(v) = vol.filter(|v| v.ends_with('%')) {
            run("pactl", &["set-sink-volume", &want, &v]);
        }
    }
    let src = default_of("source");
    let mic = mic_name().unwrap_or_default();
    if s.echo_cancel && src == mic {
        run("pactl", &["set-default-source", MIC_CLEAN]);
    } else if !s.echo_cancel && src == MIC_CLEAN && !mic.is_empty() {
        run("pactl", &["set-default-source", &mic]);
    }
}

// ------------------------------------------------------------------ volume

/// The default output's volume (percent) and mute state.
pub fn get_volume() -> Option<(u32, bool)> {
    if card::dry_run() {
        return Some((60, false));
    }
    let v = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?;
    let pct = v.split('/').nth(1)?.trim().trim_end_matches('%').trim().parse().ok()?;
    let m = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])?;
    Some((pct, m.contains("yes")))
}

pub fn set_volume(pct: u32) {
    run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", pct.min(100))]);
}

pub fn set_mute(mute: bool) {
    run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", if mute { "1" } else { "0" }]);
}

/// Calls `changed` whenever any output's volume or the default output changes.
/// Switches stereo spreading on or off for every stream that's playing, live
/// (PipeWire's per-stream "channelmix.upmix"), so the listening mode changes
/// without restarting anything. Our own outputs are left alone.
pub fn spread_streams(mode: &str) {
    if card::dry_run() {
        eprintln!("dry-run: stereo on {mode}");
        return;
    }
    let Some(out) = run("pw-dump", &["Node"]) else { return };
    let Ok(v) = serde_json::from_str::<Value>(&out) else { return };
    // Set the method too: apps that don't read the desktop's audio settings
    // (Flatpaks like Spotify, and our own outputs) otherwise default to
    // "none", which doesn't fill the centre and rears at all.
    // "simple" copies left/right to the rears (3 dB quieter); a centre cutoff
    // above 0 also fills the centre with left+right. Measured on a null sink.
    // The cutoff's low-pass sits at about half the value given: 12000 cut the
    // centre by 22 dB at 10 kHz (it sounded muffled). From 48000 up the centre
    // is flat to 14 kHz; 100000 keeps it flat for 96 and 192 kHz streams too.
    let params = match mode {
        "rear" => "{ params = [ \"channelmix.upmix\" true \"channelmix.upmix-method\" \"simple\" \"channelmix.fc-cutoff\" 0.0 \
                   \"channelmix.rear-delay\" 0.0 \"channelmix.lfe-cutoff\" 0.0 ] }",
        "all" => "{ params = [ \"channelmix.upmix\" true \"channelmix.upmix-method\" \"simple\" \"channelmix.fc-cutoff\" 100000.0 \
                  \"channelmix.rear-delay\" 0.0 \"channelmix.lfe-cutoff\" 0.0 ] }",
        _ => "{ params = [ \"channelmix.upmix\" false ] }",
    }.to_string();
    for n in v.as_array().into_iter().flatten() {
        let p = &n["info"]["props"];
        let name = p["node.name"].as_str().unwrap_or("");
        // Our own outputs stay as they are, except the relabelled stereo.
        if p["media.class"] != "Stream/Output/Audio" || (name.starts_with("sbcommand") && name != "sbcommand.relabel.out") {
            continue;
        }
        if let Some(id) = n["id"].as_u64() {
            run("pw-cli", &["set-param", &id.to_string(), "Props", &params]);
        }
    }
}

/// Sends streams whose stereo isn't labelled left/right (e.g. "aux0,aux1")
/// through the relabel output. PipeWire then remembers it for that app.
pub fn relabel_aux_streams() {
    if card::dry_run() {
        return;
    }
    let Some(target) = pactl_json("sinks").iter()
        .find(|k| k["name"] == OUT_RELABEL)
        .and_then(|k| k["index"].as_u64()) else { return };
    for si in pactl_json("sink-inputs") {
        let map = si["channel_map"].as_str().unwrap_or("");
        let aux_stereo = map.split(',').count() == 2 && map.split(',').all(|c| c.starts_with("aux"));
        if aux_stereo && si["sink"].as_u64() != Some(target)
            && let Some(idx) = si["index"].as_u64() {
                run("pactl", &["move-sink-input", &idx.to_string(), OUT_RELABEL]);
            }
    }
}

/// Applies the listening mode: spreading on each stream, and the default
/// output (True 2.1 uses the front-speakers output).
pub fn apply_listen(s: &Settings) {
    relabel_aux_streams();
    spread_streams(s.spread_mode());
    if !card::dry_run() {
        fix_defaults(s);
    }
}

/// Calls `new_stream` whenever an app starts playing.
pub fn watch_new_streams(new_stream: impl Fn() + Send + 'static) {
    if card::dry_run() {
        return;
    }
    std::thread::spawn(move || {
        use std::io::BufRead;
        loop {
            let Ok(mut child) = Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    if line.starts_with("Event 'new' on sink-input") {
                        new_stream();
                    }
                }
            }
            let _ = child.wait();
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

pub fn watch_volume(changed: impl Fn() + Send + 'static) {
    if card::dry_run() {
        return;
    }
    std::thread::spawn(move || {
        use std::io::BufRead;
        loop {
            let Ok(mut child) = Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    if line.contains(" on sink #") || line.contains(" on server") {
                        changed();
                    }
                }
            }
            let _ = child.wait();
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

/// Records 4 seconds from the microphone (with the card's voice effects) and
/// plays it back, like the Test button in Sound Blaster Command.
pub fn voice_test(s: &Settings, done: impl FnOnce(bool) + Send + 'static) {
    let s = s.clone();
    std::thread::spawn(move || {
        if card::dry_run() {
            done(true);
            return;
        }
        let path = cache_dir().join("voice-test.wav");
        let _ = std::fs::create_dir_all(cache_dir());
        let mut rec = Command::new("pw-record");
        if let Some(m) = mic_name() {
            rec.args(["--target", &m]);
        }
        let Ok(mut child) = rec.arg(&path).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
            done(false);
            return;
        };
        std::thread::sleep(Duration::from_secs(4));
        let _ = child.kill();
        let _ = child.wait();
        let mut play = Command::new("pw-play");
        if let Some(sink) = sink_name(&s) {
            play.args(["--target", &sink]);
        }
        let ok = play.arg(&path).status().is_ok_and(|st| st.success());
        done(ok);
    });
}

fn node_id(name: &str) -> Option<u64> {
    let out = run("pw-dump", &["Node"])?;
    let v: Value = serde_json::from_str(&out).ok()?;
    v.as_array()?
        .iter()
        .find(|o| o["info"]["props"]["node.name"] == name)
        .and_then(|o| o["id"].as_u64())
}




#[cfg(test)]
mod test_sound_tests {
    use super::*;

    #[test]
    fn test_sound_plays_on_one_channel_only() {
        let dir = std::env::temp_dir().join(format!("sbc-test-{}", std::process::id()));
        let src = dir.join("mono.wav");
        make_rumble(&src).unwrap();
        let six = ["FL", "FR", "FC", "LFE", "RL", "RR"];
        for (slot, ch) in six.iter().enumerate() {
            let dst = dir.join(format!("{ch}.wav"));
            spread_to_channel(&src, &six, ch, &dst).unwrap();
            let d = std::fs::read(&dst).unwrap();
            assert_eq!(u16::from_le_bytes([d[22], d[23]]), 6);
            let pcm = &d[44..];
            let mut energy = [0u64; 6];
            for (k, s) in pcm.chunks_exact(2).enumerate() {
                energy[k % 6] += i16::from_le_bytes([s[0], s[1]]).unsigned_abs() as u64;
            }
            for (c, e) in energy.iter().enumerate() {
                assert_eq!(*e > 0, c == slot, "{ch}: channel {c}");
            }
            let peak = pcm.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs()).max().unwrap();
            assert!((28_000..=29_300).contains(&peak), "{ch}: test sound peak {peak} should be about -1 dBFS");
        }
        if let Ok(keep) = std::env::var("SBC_KEEP_TEST_WAVS") {
            for ch in six {
                let _ = std::fs::copy(dir.join(format!("{ch}.wav")), format!("{keep}/{ch}.wav"));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
