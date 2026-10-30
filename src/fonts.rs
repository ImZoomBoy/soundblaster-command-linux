//! The page's typefaces, bundled with the app: Anton for big titles and
//! numbers, Archivo Narrow for the small capitals, Archivo for the page's
//! running text (bio, credits). All three are under the SIL Open
//! Font License (assets/fonts). They are made available to this app only
//! (fontconfig's "application fonts"), so nothing is installed on the system.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigAppFontAddFile(config: *mut c_void, file: *const c_char) -> c_int;
}

const FONTS: [(&str, &[u8]); 3] = [
    ("Anton-Regular.ttf", include_bytes!("../assets/fonts/Anton-Regular.ttf")),
    ("ArchivoNarrow.ttf", include_bytes!("../assets/fonts/ArchivoNarrow.ttf")),
    ("Archivo.ttf", include_bytes!("../assets/fonts/Archivo.ttf")),
];

/// Writes the fonts to the cache folder (once) and adds them for this
/// process. Call before GTK starts, so Pango sees them from the first frame.
pub fn register() {
    let dir = crate::system::cache_dir().join("fonts");
    let _ = std::fs::create_dir_all(&dir);
    for (name, data) in FONTS {
        let path = dir.join(name);
        let same = std::fs::metadata(&path).map(|m| m.len() == data.len() as u64).unwrap_or(false);
        if !same && std::fs::write(&path, data).is_err() {
            continue;
        }
        if let Ok(c) = CString::new(path.to_string_lossy().as_bytes()) {
            // SAFETY: a null config means fontconfig's current configuration;
            // the path is a valid C string that outlives the call.
            unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), c.as_ptr()) };
        }
    }
}
