//! Live level of each speaker, read from the card's output monitor (what the
//! apps send, before the card's own SBX processing). It only records from the
//! monitor while the window is open; nothing is played.
//!
//! The subwoofer is the exception: with bass redirection on, the card sends it
//! the bass of the other speakers itself, after the monitor. Stereo sources
//! send nothing on the sub's own channel, so its meter would never move. The
//! sub level therefore adds that redirected bass, worked out the same way
//! (the channels' sum through a low-pass at the crossover). Display only.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RATE: f32 = 48_000.0;

/// Which channels the card's bass redirection sends to the subwoofer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BassRoute {
    pub on: bool,
    pub crossover_hz: f32,
    /// In `ORDER`: whose bass goes to the sub (never the sub's own channel).
    pub from: [bool; 6],
}

impl BassRoute {
    pub fn of(s: &crate::model::Settings) -> Self {
        let front = !s.full_range_front;
        let rear = !s.full_range_rear;
        BassRoute {
            on: s.bass_redirect && s.has_sub() && !s.headphones(),
            crossover_hz: s.crossover.clamp(20, 400) as f32,
            from: [front, front, true, false, rear, rear],
        }
    }
}

static ROUTE: Mutex<Option<BassRoute>> = Mutex::new(None);

/// Keeps the sub meter in step with the speaker settings; called on every apply.
pub fn set_bass_route(s: &crate::model::Settings) {
    if let Ok(mut r) = ROUTE.lock() {
        *r = Some(BassRoute::of(s));
    }
}

fn bass_route() -> Option<BassRoute> {
    ROUTE.lock().ok().and_then(|r| *r)
}

/// Second-order low-pass (RBJ cookbook, Q 0.707); two in a row make the steep
/// crossover slope a speaker crossover uses.
#[derive(Clone, Copy, Default)]
struct LowPass {
    b0: f32,
    b1: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl LowPass {
    fn new(hz: f32) -> Self {
        let (s, c) = (2.0 * std::f32::consts::PI * hz / RATE).sin_cos();
        let alpha = s * std::f32::consts::FRAC_1_SQRT_2;
        let a0 = 1.0 + alpha;
        LowPass { b0: (1.0 - c) / 2.0 / a0, b1: (1.0 - c) / a0, a1: -2.0 * c / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b0 * x - self.a2 * y;
        y
    }
}

/// The subwoofer's sample: its own channel plus the redirected bass. The sum
/// is halved, so a stereo pair's bass shows at about the pair's own level.
fn sub_sample(frame: &[f32; 6], route: &BassRoute, lp: &mut [LowPass; 2]) -> f32 {
    if !route.on {
        return frame[3];
    }
    let bass: f32 = (0..6).filter(|&c| route.from[c]).map(|c| frame[c]).sum::<f32>() * 0.5;
    let once = lp[0].run(bass);
    frame[3] + lp[1].run(once)
}

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
                let (fc, rear) = match spread.as_str() { "rear" => (0.0, 0.7), _ => (0.0, 0.0) };
                send(if six { [w(3.0), w(3.4), w(2.6) * 0.95, lfe, w(4.1) * 0.7, w(3.7) * 0.7] }
                     else { [w(3.0), w(3.4), w(3.2) * fc, lfe * 0.8, w(3.0) * rear, w(3.4) * rear] });
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        return Some(Meter { child: None, stop });
    }
    if bass_route().is_none() {
        set_bass_route(&crate::model::Settings::load());
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
        let mut route = bass_route().unwrap_or(BassRoute { on: false, crossover_hz: 80.0, from: [false; 6] });
        let mut lp = [LowPass::new(route.crossover_hz); 2];
        while !st.load(Ordering::Relaxed) {
            let Ok(n) = out.read(&mut buf) else { break };
            if n == 0 {
                break;
            }
            if let Some(r) = bass_route()
                && r != route
            {
                route = r;
                lp = [LowPass::new(route.crossover_hz); 2];
            }
            for frame in buf[..n - n % 24].as_chunks::<24>().0 {
                let mut f = [0f32; 6];
                for (c, s) in frame.as_chunks::<4>().0.iter().enumerate() {
                    f[c] = f32::from_le_bytes(*s);
                }
                let sub = sub_sample(&f, &route, &mut lp);
                for (c, v) in f.iter().enumerate() {
                    let v = if c == 3 { sub.abs() } else { v.abs() };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Level of a sine through the sub path, in dB relative to its input.
    fn sub_gain(hz: f32, route: &BassRoute, chans: &[usize]) -> f32 {
        let mut lp = [LowPass::new(route.crossover_hz); 2];
        let mut peak = 0f32;
        for i in 0..(RATE as usize) {
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / RATE).sin();
            let mut f = [0f32; 6];
            for &c in chans {
                f[c] = x;
            }
            let y = sub_sample(&f, route, &mut lp);
            if i > RATE as usize / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.max(1e-9).log10()
    }

    #[test]
    fn stereo_bass_reaches_the_sub_meter() {
        let mut s = crate::model::Settings::default();
        s.speaker_config = "5.1".into();
        s.bass_redirect = true;
        s.crossover = 140;
        s.full_range_front = false;
        let r = BassRoute::of(&s);
        assert!(r.on);
        // Front left and right playing the same 50 Hz: the sub shows it at about that level.
        assert!(sub_gain(50.0, &r, &[0, 1]).abs() < 1.5, "{}", sub_gain(50.0, &r, &[0, 1]));
        // 1 kHz stays off the sub.
        assert!(sub_gain(1000.0, &r, &[0, 1]) < -40.0);
        // The sub's own channel always counts.
        assert!(sub_gain(1000.0, &r, &[3]).abs() < 0.1);
    }

    #[test]
    fn no_redirect_or_full_range_means_no_bass_on_the_sub() {
        let mut s = crate::model::Settings::default();
        s.speaker_config = "5.1".into();
        s.bass_redirect = false;
        assert!(sub_gain(50.0, &BassRoute::of(&s), &[0, 1]) < -100.0);
        s.bass_redirect = true;
        s.full_range_front = true;
        assert!(sub_gain(50.0, &BassRoute::of(&s), &[0, 1]) < -100.0);
    }
}
