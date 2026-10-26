//! Live level of each speaker, read from the card's output monitor (what the
//! apps send, before the card's own SBX processing). It only records from the
//! monitor while the window is open; nothing is played.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Order of the six levels: FL, FR, FC, LFE, RL, RR.
pub const ORDER: [&str; 6] = ["FL", "FR", "FC", "LFE", "RL", "RR"];

pub struct Meter {
    child: Option<Child>,
    stop: Arc<AtomicBool>,
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Starts metering; `send` gets the six peak levels (0..1) about 20 times a second.
pub fn start(send: impl Fn([f32; 6]) + Send + 'static) -> Option<Meter> {
    let stop = Arc::new(AtomicBool::new(false));
    if crate::card::dry_run() {
        // Self-test and screenshots: a steady, believable pattern.
        let st = stop.clone();
        std::thread::spawn(move || {
            let mut t = 0f32;
            let demo = std::env::var("SBCOMMAND_DEMO_MEDIA").unwrap_or_default();
            let six = demo == "movie" || demo == "game";
            let spread = crate::model::Settings::load().spread_mode().to_string();
            while !st.load(Ordering::Relaxed) {
                t += 1.0;
                let w = |k: f32| 0.55 + 0.3 * (t / k).sin().abs();
                let lfe = if (t as i32) % 4 == 0 { 0.85 } else { 0.5 };
                let (fc, rear) = match spread.as_str() { "rear" => (0.0, 0.7), "all" => (0.8, 0.7), _ => (0.0, 0.0) };
                send(if six { [w(3.0), w(3.4), w(2.6) * 0.95, lfe, w(4.1) * 0.7, w(3.7) * 0.7] }
                     else { [w(3.0), w(3.4), w(3.2) * fc, lfe * 0.8, w(3.0) * rear, w(3.4) * rear] });
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        return Some(Meter { child: None, stop });
    }
    let sink = crate::media::card_sink()?;
    let mut child = Command::new("parec")
        .args([
            &format!("--device={sink}.monitor"),
            "--raw",
            "--format=float32le",
            "--rate=48000",
            "--channels=6",
            "--channel-map=front-left,front-right,front-center,lfe,rear-left,rear-right",
            "--latency-msec=40",
            "--client-name=Sound Blaster Command meters",
            "--stream-name=Level meters",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let st = stop.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 6 * 4 * 480];
        let mut peak = [0f32; 6];
        let mut last = Instant::now();
        while !st.load(Ordering::Relaxed) {
            let Ok(n) = out.read(&mut buf) else { break };
            if n == 0 {
                break;
            }
            for frame in buf[..n - n % 24].as_chunks::<24>().0 {
                for (c, s) in frame.as_chunks::<4>().0.iter().enumerate() {
                    let v = f32::from_le_bytes(*s).abs();
                    if v > peak[c] {
                        peak[c] = v;
                    }
                }
            }
            if last.elapsed() >= Duration::from_millis(50) {
                // Peak in dB mapped to 0..1 over -48..0 dB, so quiet passages still show.
                let lv = peak.map(|p| if p <= 1e-5 { 0.0 } else { ((20.0 * p.log10() + 48.0) / 48.0).clamp(0.0, 1.0) });
                send(lv);
                peak = [0.0; 6];
                last = Instant::now();
            }
        }
    });
    Some(Meter { child: Some(child), stop })
}
