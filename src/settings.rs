//! Persistent user preferences.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cache::default_cache_root;
use crate::show::PlayMode;

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
}
