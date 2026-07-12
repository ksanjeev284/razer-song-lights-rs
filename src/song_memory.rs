//! Per-song remembered offset and resume position.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::cache::default_cache_root;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SongMemoryEntry {
    pub offset: f32,
    pub resume_s: f64,
    pub plays: u32,
    /// Free-form note for this song.
    #[serde(default)]
    pub note: String,
    /// 0–5 star rating (0 = unset).
    #[serde(default)]
    pub rating: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SongMemory {
    pub by_id: HashMap<String, SongMemoryEntry>,
}

impl SongMemory {
    pub fn path() -> PathBuf {
        default_cache_root().join("song_memory.json")
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

    pub fn get(&self, id: &str) -> Option<&SongMemoryEntry> {
        if id.is_empty() {
            None
        } else {
            self.by_id.get(id)
        }
    }

    pub fn set_offset(&mut self, id: &str, offset: f32) {
        if id.is_empty() {
            return;
        }
        self.by_id.entry(id.to_string()).or_default().offset = offset;
        self.save();
    }

    pub fn set_resume(&mut self, id: &str, t: f64) {
        if id.is_empty() {
            return;
        }
        let e = self.by_id.entry(id.to_string()).or_default();
        e.resume_s = t.max(0.0);
        self.save();
    }

    pub fn bump_plays(&mut self, id: &str) {
        if id.is_empty() {
            return;
        }
        self.by_id.entry(id.to_string()).or_default().plays += 1;
        self.save();
    }

    pub fn clear_resume(&mut self, id: &str) {
        if let Some(e) = self.by_id.get_mut(id) {
            e.resume_s = 0.0;
            self.save();
        }
    }

    pub fn set_note(&mut self, id: &str, note: &str) {
        if id.is_empty() {
            return;
        }
        self.by_id.entry(id.to_string()).or_default().note = note.to_string();
        self.save();
    }

    pub fn set_rating(&mut self, id: &str, rating: u8) {
        if id.is_empty() {
            return;
        }
        self.by_id.entry(id.to_string()).or_default().rating = rating.min(5);
        self.save();
    }

    pub fn note(&self, id: &str) -> String {
        self.get(id).map(|e| e.note.clone()).unwrap_or_default()
    }

    pub fn rating(&self, id: &str) -> u8 {
        self.get(id).map(|e| e.rating).unwrap_or(0)
    }
}

/// Import URLs from a playlist text / m3u file.
pub fn parse_playlist_urls(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with(';'))
        .map(|l| l.to_string())
        .collect()
}

pub fn export_playlist(urls: &[String]) -> String {
    let mut out = String::from("#EXTM3U\n# Razer Song Lights playlist\n");
    for u in urls {
        out.push_str(u);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_offset() {
        let mut m = SongMemory::default();
        m.set_offset("vid", 1.5);
        assert!((m.get("vid").unwrap().offset - 1.5).abs() < 0.01);
    }

    #[test]
    fn parse_m3u() {
        let t = "#EXTM3U\n#comment\nhttps://youtu.be/a\n\nhttps://youtu.be/b\n";
        let u = parse_playlist_urls(t);
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn notes_and_rating() {
        let mut m = SongMemory::default();
        m.set_note("vid", "great chorus");
        m.set_rating("vid", 5);
        assert_eq!(m.note("vid"), "great chorus");
        assert_eq!(m.rating("vid"), 5);
        m.set_rating("vid", 9);
        assert_eq!(m.rating("vid"), 5);
    }
}
