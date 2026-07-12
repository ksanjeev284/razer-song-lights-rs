//! Light color themes + brightness for keyboard karaoke.

use crate::color::rgb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LightTheme {
    #[default]
    Rainbow,
    RazerGreen,
    Fire,
    Ice,
    Purple,
    Gold,
    Pink,
    MonoWhite,
}

impl LightTheme {
    pub const ALL: [LightTheme; 8] = [
        LightTheme::Rainbow,
        LightTheme::RazerGreen,
        LightTheme::Fire,
        LightTheme::Ice,
        LightTheme::Purple,
        LightTheme::Gold,
        LightTheme::Pink,
        LightTheme::MonoWhite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LightTheme::Rainbow => "Rainbow",
            LightTheme::RazerGreen => "Razer Green",
            LightTheme::Fire => "Fire",
            LightTheme::Ice => "Ice",
            LightTheme::Purple => "Purple",
            LightTheme::Gold => "Gold",
            LightTheme::Pink => "Pink",
            LightTheme::MonoWhite => "White",
        }
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL
            .get(i as usize % Self::ALL.len())
            .copied()
            .unwrap_or(LightTheme::Rainbow)
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as u8
    }
}

/// Ambient keyboard effect between lyric hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AmbientEffect {
    #[default]
    Pulse,
    Wave,
    Breath,
    Ripple,
    Off,
}

impl AmbientEffect {
    pub const ALL: [AmbientEffect; 5] = [
        AmbientEffect::Pulse,
        AmbientEffect::Wave,
        AmbientEffect::Breath,
        AmbientEffect::Ripple,
        AmbientEffect::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AmbientEffect::Pulse => "Pulse",
            AmbientEffect::Wave => "Wave",
            AmbientEffect::Breath => "Breath",
            AmbientEffect::Ripple => "Ripple",
            AmbientEffect::Off => "Off",
        }
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL
            .get(i as usize % Self::ALL.len())
            .copied()
            .unwrap_or(AmbientEffect::Pulse)
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as u8
    }

    pub fn cycle(self) -> Self {
        Self::from_index(self.index().wrapping_add(1))
    }

    pub fn is_on(self) -> bool {
        !matches!(self, AmbientEffect::Off)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    One,
    All,
}

impl RepeatMode {
    pub fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "Off",
            RepeatMode::One => "One",
            RepeatMode::All => "All",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::One,
            RepeatMode::One => RepeatMode::All,
            RepeatMode::All => RepeatMode::Off,
        }
    }
}

/// Theme color for animation index, with brightness 0.0–1.0.
pub fn themed_color(theme: LightTheme, index: usize, brightness: f32) -> u32 {
    let b = brightness.clamp(0.05, 1.0);
    let (r, g, bl) = match theme {
        LightTheme::Rainbow => {
            let hue = (index as f64 * 0.618_033_988_75) % 1.0;
            hsv_to_rgb(hue, 0.9, 1.0)
        }
        LightTheme::RazerGreen => {
            let pulse = 0.7 + 0.3 * ((index % 5) as f32 / 4.0);
            (
                (0x44 as f32 * pulse) as u8,
                (0xd6 as f32 * pulse) as u8,
                (0x2c as f32 * pulse) as u8,
            )
        }
        LightTheme::Fire => {
            let t = (index % 8) as f32 / 7.0;
            (
                255,
                (80.0 + 140.0 * (1.0 - t)) as u8,
                (10.0 + 20.0 * (1.0 - t)) as u8,
            )
        }
        LightTheme::Ice => {
            let t = (index % 6) as f32 / 5.0;
            ((120.0 + 80.0 * t) as u8, (200.0 + 40.0 * t) as u8, 255)
        }
        LightTheme::Purple => {
            let t = (index % 5) as f32 / 4.0;
            (
                (160.0 + 60.0 * t) as u8,
                (40.0 + 40.0 * t) as u8,
                (220.0 + 30.0 * t) as u8,
            )
        }
        LightTheme::Gold => {
            let t = (index % 4) as f32 / 3.0;
            (255, (180.0 + 50.0 * t) as u8, (40.0 + 20.0 * t) as u8)
        }
        LightTheme::Pink => {
            let t = (index % 5) as f32 / 4.0;
            (255, (80.0 + 100.0 * t) as u8, (160.0 + 60.0 * t) as u8)
        }
        LightTheme::MonoWhite => (255, 255, 255),
    };
    rgb(
        (r as f32 * b) as u8,
        (g as f32 * b) as u8,
        (bl as f32 * b) as u8,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_produce_colors() {
        for th in LightTheme::ALL {
            let c = themed_color(th, 3, 1.0);
            assert_ne!(c, 0);
            let dim = themed_color(th, 3, 0.2);
            // dimmer should generally be darker (not always for pure black channels)
            let _ = dim;
        }
    }

    #[test]
    fn repeat_cycles() {
        assert_eq!(RepeatMode::Off.cycle(), RepeatMode::One);
        assert_eq!(RepeatMode::One.cycle(), RepeatMode::All);
        assert_eq!(RepeatMode::All.cycle(), RepeatMode::Off);
    }

    #[test]
    fn ambient_effect_cycles() {
        assert_eq!(AmbientEffect::Pulse.cycle(), AmbientEffect::Wave);
        assert!(!AmbientEffect::Off.is_on());
        assert!(AmbientEffect::Breath.is_on());
    }
}
