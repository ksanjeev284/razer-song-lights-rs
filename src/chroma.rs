//! Razer Chroma keyboard control via official Windows SDK DLL.
//!
//! Only available on Windows (`RzChromaSDK64.dll` + Razer Synapse).

use crate::color::rgb;
use crate::key_map::{char_to_position, MAX_COL, MAX_ROW};
use thiserror::Error;

/// True when the official Chroma SDK can load on this host.
pub fn is_chroma_supported() -> bool {
    cfg!(windows)
}

#[derive(Debug, Error)]
pub enum ChromaError {
    #[error("Razer Chroma SDK is only available on Windows (this host is not supported)")]
    UnsupportedPlatform,
    #[error("Could not load RzChromaSDK64.dll — is Razer Synapse installed? ({0})")]
    Load(String),
    #[error("Chroma SDK Init failed with code {0}")]
    Init(i32),
    #[error("Chroma effect error: {0}")]
    Effect(String),
}

/// In-memory frame buffer + SDK session.
pub struct ChromaKeyboard {
    #[cfg(windows)]
    inner: windows_impl::Inner,
    #[cfg(not(windows))]
    _priv: (),
    frame: [[u32; MAX_COL]; MAX_ROW],
}

impl ChromaKeyboard {
    pub fn open() -> Result<Self, ChromaError> {
        if !is_chroma_supported() {
            return Err(ChromaError::UnsupportedPlatform);
        }
        #[cfg(windows)]
        {
            let inner = windows_impl::Inner::open()?;
            Ok(Self {
                inner,
                frame: [[0; MAX_COL]; MAX_ROW],
            })
        }
        #[cfg(not(windows))]
        {
            Err(ChromaError::UnsupportedPlatform)
        }
    }

    pub fn clear(&mut self, color: u32) {
        for row in self.frame.iter_mut() {
            for cell in row.iter_mut() {
                *cell = color & 0x00FF_FFFF;
            }
        }
    }

    pub fn set_key(&mut self, row: usize, col: usize, color: u32) {
        if row < MAX_ROW && col < MAX_COL {
            self.frame[row][col] = color & 0x00FF_FFFF;
        }
    }

    /// Light all known keys that appear in `text`.
    pub fn flash_text(&mut self, text: &str, color: u32) {
        self.clear(0);
        for ch in text.chars() {
            if ch.is_whitespace() {
                continue;
            }
            if let Some((r, c)) = char_to_position(ch) {
                self.set_key(r, c, color);
            }
        }
    }

    pub fn push(&mut self) -> Result<(), ChromaError> {
        #[cfg(windows)]
        {
            self.inner.push(&self.frame)
        }
        #[cfg(not(windows))]
        {
            Err(ChromaError::UnsupportedPlatform)
        }
    }

    /// Flash white then settle on `color` for text keys (hit effect).
    pub fn flash_text_hit(&mut self, text: &str, color: u32) -> Result<(), ChromaError> {
        self.flash_text(text, rgb(255, 255, 255));
        self.push()?;
        std::thread::sleep(std::time::Duration::from_millis(40));
        self.flash_text(text, color);
        self.push()
    }

    fn dim_color(c: u32, f: f32) -> u32 {
        let f = f.clamp(0.0, 1.0);
        let r = ((c & 0xFF) as f32 * f) as u32;
        let g = (((c >> 8) & 0xFF) as f32 * f) as u32;
        let b = (((c >> 16) & 0xFF) as f32 * f) as u32;
        (b << 16) | (g << 8) | r
    }

    /// Soft ambient wash across a band of keys (between lyric events).
    pub fn ambient_pulse(&mut self, phase: f64, color: u32, strength: f32) -> Result<(), ChromaError> {
        self.clear(0);
        let s = strength.clamp(0.05, 1.0);
        let col = ((phase * MAX_COL as f64) as usize) % MAX_COL;
        for r in 0..MAX_ROW {
            for c in 0..MAX_COL {
                let dist = (c as i32 - col as i32).unsigned_abs() as f32;
                let f = (1.0 - (dist / 4.0).min(1.0)) * s * 0.45;
                if f > 0.05 {
                    self.set_key(r, c, Self::dim_color(color, f));
                }
            }
        }
        self.push()
    }

    /// Horizontal sine wave wash.
    pub fn ambient_wave(&mut self, phase: f64, color: u32, strength: f32) -> Result<(), ChromaError> {
        self.clear(0);
        let s = strength.clamp(0.05, 1.0);
        for r in 0..MAX_ROW {
            for c in 0..MAX_COL {
                let x = c as f64 / MAX_COL as f64;
                let wave = ((x * std::f64::consts::TAU * 1.5 + phase * 2.2).sin() + 1.0) * 0.5;
                let f = (wave as f32) * s * 0.5;
                if f > 0.06 {
                    self.set_key(r, c, Self::dim_color(color, f));
                }
            }
        }
        self.push()
    }

    /// Whole-keyboard breathing (global pulse).
    pub fn ambient_breath(&mut self, phase: f64, color: u32, strength: f32) -> Result<(), ChromaError> {
        self.clear(0);
        let s = strength.clamp(0.05, 1.0);
        let breath = (((phase * 1.8).sin() + 1.0) * 0.5) as f32;
        let f = (0.12 + breath * 0.55) * s;
        let c = Self::dim_color(color, f);
        for r in 0..MAX_ROW {
            for col in 0..MAX_COL {
                self.set_key(r, col, c);
            }
        }
        self.push()
    }

    /// Expanding ring from center.
    pub fn ambient_ripple(&mut self, phase: f64, color: u32, strength: f32) -> Result<(), ChromaError> {
        self.clear(0);
        let s = strength.clamp(0.05, 1.0);
        let cx = (MAX_COL as f64 - 1.0) / 2.0;
        let cy = (MAX_ROW as f64 - 1.0) / 2.0;
        let radius = (phase * 0.9).rem_euclid(1.0) * (cx.max(cy) + 2.0);
        for r in 0..MAX_ROW {
            for c in 0..MAX_COL {
                let dist = ((c as f64 - cx).powi(2) + (r as f64 - cy).powi(2)).sqrt();
                let ring = 1.0 - ((dist - radius).abs() / 1.8).min(1.0);
                let f = (ring as f32) * s * 0.7;
                if f > 0.08 {
                    self.set_key(r, c, Self::dim_color(color, f));
                }
            }
        }
        self.push()
    }
}

impl Drop for ChromaKeyboard {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = self.inner.close();
        }
    }
}

/// Rainbow-ish color from index (matches Python HSV cascade feel).
pub fn color_for_index(index: usize) -> u32 {
    let hue = (index as f64 * 0.618_033_988_75) % 1.0;
    let (r, g, b) = hsv_to_rgb(hue, 0.9, 1.0);
    rgb(r, g, b)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

#[cfg(windows)]
mod windows_impl {
    use super::ChromaError;
    use crate::key_map::{MAX_COL, MAX_ROW};
    use std::ffi::c_void;
    use std::path::Path;
    use std::ptr;

    type RzResult = i32;
    // GUID is 16 bytes
    type EffectId = [u8; 16];

    const CHROMA_NONE: i32 = 0;
    const CHROMA_CUSTOM: i32 = 2;

    #[repr(C)]
    struct CustomEffect {
        color: [[u32; MAX_COL]; MAX_ROW],
    }

    type FnInit = unsafe extern "C" fn() -> RzResult;
    type FnUnInit = unsafe extern "C" fn() -> RzResult;
    type FnCreateKeyboardEffect =
        unsafe extern "C" fn(i32, *const c_void, *mut EffectId) -> RzResult;
    type FnSetEffect = unsafe extern "C" fn(*const EffectId) -> RzResult;
    type FnDeleteEffect = unsafe extern "C" fn(*const EffectId) -> RzResult;

    pub struct Inner {
        _lib: libloading::Library,
        init: FnInit,
        uninit: FnUnInit,
        create: FnCreateKeyboardEffect,
        set_effect: FnSetEffect,
        delete: FnDeleteEffect,
        effect_id: EffectId,
        has_effect: bool,
        open: bool,
    }

    impl Inner {
        pub fn open() -> Result<Self, ChromaError> {
            let candidates = [
                r"C:\Windows\System32\RzChromaSDK64.dll",
                r"C:\Windows\SysWOW64\RzChromaSDK.dll",
            ];
            let mut last = String::from("not found");
            for path in candidates {
                if !Path::new(path).is_file() {
                    continue;
                }
                match unsafe { libloading::Library::new(path) } {
                    Ok(lib) => {
                        unsafe {
                            let init: libloading::Symbol<FnInit> = lib
                                .get(b"Init")
                                .map_err(|e| ChromaError::Load(e.to_string()))?;
                            let uninit: libloading::Symbol<FnUnInit> = lib
                                .get(b"UnInit")
                                .map_err(|e| ChromaError::Load(e.to_string()))?;
                            let create: libloading::Symbol<FnCreateKeyboardEffect> = lib
                                .get(b"CreateKeyboardEffect")
                                .map_err(|e| ChromaError::Load(e.to_string()))?;
                            let set_effect: libloading::Symbol<FnSetEffect> = lib
                                .get(b"SetEffect")
                                .map_err(|e| ChromaError::Load(e.to_string()))?;
                            let delete: libloading::Symbol<FnDeleteEffect> = lib
                                .get(b"DeleteEffect")
                                .map_err(|e| ChromaError::Load(e.to_string()))?;

                            // Leak symbols into owned fns by copying pointers before drop of Symbol
                            let init_fn: FnInit = *init;
                            let uninit_fn: FnUnInit = *uninit;
                            let create_fn: FnCreateKeyboardEffect = *create;
                            let set_fn: FnSetEffect = *set_effect;
                            let delete_fn: FnDeleteEffect = *delete;

                            let rc = init_fn();
                            if rc != 0 {
                                return Err(ChromaError::Init(rc));
                            }

                            // Keep library alive: transmute-like by forgetting symbols and keeping lib
                            // We need to store the library. Symbols are valid while lib lives.
                            // Re-get is wrong - store raw pointers from symbols.
                            let inner = Inner {
                                _lib: lib,
                                init: init_fn,
                                uninit: uninit_fn,
                                create: create_fn,
                                set_effect: set_fn,
                                delete: delete_fn,
                                effect_id: [0; 16],
                                has_effect: false,
                                open: true,
                            };
                            let _ = inner.init;
                            return Ok(inner);
                        }
                    }
                    Err(e) => last = e.to_string(),
                }
            }
            Err(ChromaError::Load(last))
        }

        pub fn push(&mut self, frame: &[[u32; MAX_COL]; MAX_ROW]) -> Result<(), ChromaError> {
            if !self.open {
                return Err(ChromaError::Effect("closed".into()));
            }
            let mut effect = CustomEffect {
                color: [[0; MAX_COL]; MAX_ROW],
            };
            for r in 0..MAX_ROW {
                for c in 0..MAX_COL {
                    effect.color[r][c] = frame[r][c];
                }
            }
            if self.has_effect {
                unsafe {
                    let _ = (self.delete)(&self.effect_id);
                }
                self.has_effect = false;
            }
            let mut eid: EffectId = [0; 16];
            let rc = unsafe {
                (self.create)(
                    CHROMA_CUSTOM,
                    &effect as *const CustomEffect as *const c_void,
                    &mut eid,
                )
            };
            if rc != 0 {
                return Err(ChromaError::Effect(format!("CreateKeyboardEffect {rc}")));
            }
            let rc = unsafe { (self.set_effect)(&eid) };
            if rc != 0 {
                unsafe {
                    let _ = (self.delete)(&eid);
                }
                return Err(ChromaError::Effect(format!("SetEffect {rc}")));
            }
            self.effect_id = eid;
            self.has_effect = true;
            Ok(())
        }

        pub fn close(&mut self) -> Result<(), ChromaError> {
            if !self.open {
                return Ok(());
            }
            if self.has_effect {
                unsafe {
                    let _ = (self.delete)(&self.effect_id);
                }
                self.has_effect = false;
            }
            let mut none: EffectId = [0; 16];
            unsafe {
                let _ = (self.create)(CHROMA_NONE, ptr::null(), &mut none);
                let _ = (self.set_effect)(&none);
                let _ = (self.uninit)();
            }
            self.open = false;
            Ok(())
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_flag() {
        assert_eq!(is_chroma_supported(), cfg!(windows));
    }

    #[test]
    fn color_index_varies() {
        assert_ne!(color_for_index(0), color_for_index(1));
    }
}
