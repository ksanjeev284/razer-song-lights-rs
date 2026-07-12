//! Persistent user preferences + recent URLs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cache::default_cache_root;
use crate::show::PlayMode;
use crate::themes::{AmbientEffect, LightTheme, RepeatMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub play_audio: bool,
    pub cache_music: bool,
    pub lights: bool,
    pub loop_play: bool,
    pub auto_next: bool,
    pub volume: f32,
    pub offset: f32,
    pub speed: f32,
    pub mode: String,
    pub show_history_panel: bool,
    #[serde(default)]
    pub brightness: f32,
    #[serde(default)]
    pub theme: LightTheme,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default)]
    pub mini_player: bool,
    #[serde(default)]
    pub recent_urls: Vec<String>,
    #[serde(default = "default_true")]
    pub ambient_pulse: bool,
    #[serde(default = "default_true")]
    pub resume_position: bool,
    #[serde(default = "default_true")]
    pub remember_offset: bool,
    #[serde(default)]
    pub crossfade_next: bool,
    #[serde(default)]
    pub night_dim: bool,
    #[serde(default)]
    pub ambient_effect: AmbientEffect,
    #[serde(default = "default_true")]
    pub sleep_fade: bool,
    #[serde(default)]
    pub seek_snap: bool,
    /// Path to Netscape cookies.txt for yt-dlp (YouTube bot check).
    #[serde(default)]
    pub ytdlp_cookies_file: String,
    /// Browser for `--cookies-from-browser` (chrome|edge|firefox|brave|none).
    #[serde(default = "default_cookies_browser")]
    pub ytdlp_cookies_browser: String,
    /// Dim lights automatically 22:00–07:00 local time.
    #[serde(default)]
    pub auto_night_dim: bool,
    /// Mirror ambient light direction.
    #[serde(default)]
    pub mirror_lights: bool,
}

fn default_cookies_browser() -> String {
    // Auto-use Chrome cookies on Windows when user is logged into YouTube.
    if cfg!(windows) {
        "chrome".into()
    } else {
        "none".into()
    }
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            play_audio: true,
            cache_music: true,
            lights: true,
            loop_play: false,
            auto_next: true,
            volume: 0.85,
            offset: 0.0,
            speed: 1.0,
            mode: "flash_word".into(),
            show_history_panel: true,
            brightness: 1.0,
            theme: LightTheme::Rainbow,
            repeat: RepeatMode::Off,
            shuffle: false,
            always_on_top: false,
            mini_player: false,
            recent_urls: Vec::new(),
            ambient_pulse: true,
            resume_position: true,
            remember_offset: true,
            crossfade_next: false,
            night_dim: false,
            ambient_effect: AmbientEffect::Pulse,
            sleep_fade: true,
            seek_snap: false,
            ytdlp_cookies_file: String::new(),
            ytdlp_cookies_browser: default_cookies_browser(),
            auto_night_dim: false,
            mirror_lights: false,
        }
    }
}

impl AppSettings {
    pub fn path() -> PathBuf {
        default_cache_root().join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, data);
        }
    }

    pub fn play_mode(&self) -> PlayMode {
        match self.mode.as_str() {
            "flash_line" => PlayMode::FlashLine,
            _ => PlayMode::FlashWord,
        }
    }

    pub fn set_play_mode(&mut self, m: PlayMode) {
        self.mode = match m {
            PlayMode::FlashLine => "flash_line".into(),
            PlayMode::FlashWord => "flash_word".into(),
        };
    }

    pub fn push_recent_url(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            return;
        }
        self.recent_urls.retain(|u| u != url);
        self.recent_urls.insert(0, url.to_string());
        self.recent_urls.truncate(12);
    }
}

/// Simple favorites store (video ids).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Favorites {
    pub ids: Vec<String>,
}

impl Favorites {
    pub fn path() -> PathBuf {
        default_cache_root().join("favorites.json")
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(p) = path.parent() {
            let _ = fs::create_dir_all(p);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, data);
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        !id.is_empty() && self.ids.iter().any(|x| x == id)
    }

    pub fn toggle(&mut self, id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        if let Some(i) = self.ids.iter().position(|x| x == id) {
            self.ids.remove(i);
            self.save();
            false
        } else {
            self.ids.push(id.to_string());
            self.save();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_mode() {
        let mut s = AppSettings::default();
        s.set_play_mode(PlayMode::FlashLine);
        assert!(matches!(s.play_mode(), PlayMode::FlashLine));
    }

    #[test]
    fn favorites_toggle() {
        let mut f = Favorites::default();
        assert!(f.toggle("abc123"));
        assert!(f.contains("abc123"));
        assert!(!f.toggle("abc123"));
        assert!(!f.contains("abc123"));
    }

    #[test]
    fn recent_urls() {
        let mut s = AppSettings::default();
        s.push_recent_url("https://youtu.be/a");
        s.push_recent_url("https://youtu.be/b");
        s.push_recent_url("https://youtu.be/a");
        assert_eq!(s.recent_urls[0], "https://youtu.be/a");
        assert_eq!(s.recent_urls.len(), 2);
    }
}
