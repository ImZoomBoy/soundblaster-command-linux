//! Pictures for the page: the page-red print (a picture's light printed in
//! red, its dark left black, like the artist photos on gig posters), auto
//! levels for dark game art, and plain darkening for colour pictures behind
//! words. Work happens on a helper thread; the result becomes a GdkTexture.

use gtk::gdk;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib;
use gtk::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The page red, #e3262f.
pub const RED: (f64, f64, f64) = (0.890, 0.149, 0.184);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Look {
    /// Printed in the page red (dark stays black).
    Red,
    /// Printed in red after stretching its light (for dark game art).
    RedLevelled,
    /// Full colour, darkened so words and the room stay clear.
    Dim,
    /// Untouched, full colour (covers, boxes, posters).
    Plain,
}

/// Processed pixels: width, height, RGBA.
#[derive(Clone)]
pub struct Pixels {
    pub w: i32,
    pub h: i32,
    pub rgba: Vec<u8>,
}

impl Pixels {
    pub fn texture(&self) -> gdk::Texture {
        let bytes = glib::Bytes::from(&self.rgba[..]);
        gdk::MemoryTexture::new(self.w, self.h, gdk::MemoryFormat::R8g8b8a8, &bytes, (self.w * 4) as usize).upcast()
    }

    /// A cairo surface (for drawing in the room: logos, records, pictures).
    pub fn surface(&self) -> Option<gtk::cairo::ImageSurface> {
        let mut s = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, self.w, self.h).ok()?;
        let stride = s.stride() as usize;
        {
            let mut d = s.data().ok()?;
            for y in 0..self.h as usize {
                for x in 0..self.w as usize {
                    let i = (y * self.w as usize + x) * 4;
                    let a = self.rgba[i + 3] as u32;
                    let o = y * stride + x * 4;
                    // cairo wants premultiplied BGRA (little-endian ARGB32).
                    d[o] = (self.rgba[i + 2] as u32 * a / 255) as u8;
                    d[o + 1] = (self.rgba[i + 1] as u32 * a / 255) as u8;
                    d[o + 2] = (self.rgba[i] as u32 * a / 255) as u8;
                    d[o + 3] = a as u8;
                }
            }
        }
        s.mark_dirty();
        Some(s)
    }
}

type Key = (PathBuf, Look, i32);
static CACHE: Mutex<Option<HashMap<Key, Pixels>>> = Mutex::new(None);

/// Loads a picture (at most `max` pixels on its long side) and prepares it.
/// Cached, so pictures that take turns are only worked on once.
pub fn load(path: &Path, look: Look, max: i32) -> Option<Pixels> {
    let key = (path.to_path_buf(), look, max);
    if let Some(p) = CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(&key) {
        return Some(p.clone());
    }
    let pb = Pixbuf::from_file_at_scale(path, max, max, true).ok()?;
    let p = process(&pb, look);
    if let Ok(mut c) = CACHE.lock() {
        let m = c.get_or_insert_with(HashMap::new);
        if m.len() > 48 {
            m.clear();
        }
        m.insert(key, p.clone());
    }
    Some(p)
}

fn rgba_of(pb: &Pixbuf) -> Pixels {
    let pb = if pb.has_alpha() { pb.clone() } else { pb.add_alpha(false, 0, 0, 0).unwrap_or_else(|_| pb.clone()) };
    let nch = pb.n_channels() as usize;
    let (w, h, stride) = (pb.width(), pb.height(), pb.rowstride() as usize);
    let src = pb.read_pixel_bytes();
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let row = &src[y * stride..];
        if nch == 4 {
            rgba.extend_from_slice(&row[..w as usize * 4]);
        } else {
            for x in 0..w as usize {
                rgba.extend_from_slice(&[row[x * nch], row[x * nch + 1], row[x * nch + 2], 255]);
            }
        }
    }
    Pixels { w, h, rgba }
}

fn lum(r: u8, g: u8, b: u8) -> f64 {
    (0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64) / 255.0
}

/// Processes pixels for a look (also used directly on tests' pictures).
pub fn process(pb: &Pixbuf, look: Look) -> Pixels {
    let mut p = rgba_of(pb);
    match look {
        Look::Plain => {}
        Look::Dim => {
            for px in p.rgba.chunks_exact_mut(4) {
                for c in &mut px[..3] {
                    *c = (*c as f64 * 0.6) as u8;
                }
            }
        }
        Look::Red | Look::RedLevelled => {
            let (lo, hi, gm) = if look == Look::RedLevelled { levels(&p) } else { (0.0, 1.0, 1.0) };
            for px in p.rgba.chunks_exact_mut(4) {
                let mut l = lum(px[0], px[1], px[2]);
                l = ((l - lo) / (hi - lo)).clamp(0.0, 1.0).powf(gm);
                // Contrast 1.25 around the middle, like the prototype's print.
                l = ((l - 0.5) * 1.25 + 0.5).clamp(0.0, 1.0);
                px[0] = (RED.0 * 255.0 * l) as u8;
                px[1] = (RED.1 * 255.0 * l) as u8;
                px[2] = (RED.2 * 255.0 * l) as u8;
                px[3] = 255;
            }
        }
    }
    p
}

/// Auto levels: where the darkest 2 % and the brightest 1.5 % sit, and a gamma
/// that lifts a dark picture's middle towards 0.42.
fn levels(p: &Pixels) -> (f64, f64, f64) {
    let mut hist = [0u32; 256];
    for px in p.rgba.chunks_exact(4) {
        hist[(lum(px[0], px[1], px[2]) * 255.0) as usize] += 1;
    }
    let n = (p.rgba.len() / 4) as f64;
    let pct = |q: f64| {
        let mut a = 0.0;
        for (v, c) in hist.iter().enumerate() {
            a += *c as f64;
            if a >= q * n {
                return v as f64 / 255.0;
            }
        }
        1.0
    };
    let lo = pct(0.02);
    let hi = pct(0.985).max(lo + 40.0 / 255.0);
    let med = ((pct(0.5) - lo) / (hi - lo)).clamp(0.03, 0.95);
    let gm = (0.42f64.ln() / med.ln()).clamp(0.45, 1.3);
    (lo, hi, gm)
}

/// A logo as white ink on transparent (dark logos become white; colour logos
/// keep their shape). For the title and the neon sign.
pub fn ink(path: &Path, max: i32) -> Option<Pixels> {
    let pb = Pixbuf::from_file_at_scale(path, max, max, true).ok()?;
    let mut p = rgba_of(&pb);
    for px in p.rgba.chunks_exact_mut(4) {
        px[0] = 255;
        px[1] = 255;
        px[2] = 255;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: i32, h: i32, rgb: (u8, u8, u8)) -> Pixbuf {
        let pb = Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, w, h).unwrap();
        pb.fill(((rgb.0 as u32) << 24) | ((rgb.1 as u32) << 16) | ((rgb.2 as u32) << 8) | 0xff);
        pb
    }

    #[test]
    fn red_print_keeps_black_black_and_white_red() {
        let p = process(&solid(4, 4, (0, 0, 0)), Look::Red);
        assert_eq!(&p.rgba[..4], &[0, 0, 0, 255]);
        let p = process(&solid(4, 4, (255, 255, 255)), Look::Red);
        assert_eq!(p.rgba[0], (RED.0 * 255.0) as u8);
        assert!(p.rgba[1] < 50 && p.rgba[3] == 255);
    }

    #[test]
    fn dim_darkens_and_plain_does_not() {
        let p = process(&solid(2, 2, (200, 100, 50)), Look::Dim);
        assert_eq!(&p.rgba[..3], &[120, 60, 30]);
        let p = process(&solid(2, 2, (200, 100, 50)), Look::Plain);
        assert_eq!(&p.rgba[..3], &[200, 100, 50]);
    }
}
