//! Simple listening stats.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::default_cache_root;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListenStats {
    pub songs_played: u64,
    pub total_listen_secs: f64,
    pub last_played: String,
    pub sessions: u64,
}

impl ListenStats {
    pub fn path() -> PathBuf {
        default_cache_root().join("stats.json")
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

    pub fn record_play(&mut self, title: &str) {
        self.songs_played = self.songs_played.saturating_add(1);
        self.last_played = title.to_string();
        self.save();
    }

    pub fn add_listen_secs(&mut self, secs: f64) {
        if secs > 0.0 && secs < 3600.0 * 4.0 {
            self.total_listen_secs += secs;
            self.save();
        }
    }

    pub fn start_session(&mut self) {
        self.sessions = self.sessions.saturating_add(1);
        self.save();
    }

    pub fn summary(&self) -> String {
        let h = self.total_listen_secs / 3600.0;
        format!(
            "Played {} songs · {:.1}h listen · last: {}",
            self.songs_played,
            h,
            if self.last_played.is_empty() {
                "—"
            } else {
                &self.last_played
            }
        )
    }
}

/// In-song bookmarks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Bookmarks {
    pub items: Vec<Bookmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub video_id: String,
    pub title: String,
    pub t: f64,
    pub label: String,
    pub created: u64,
}

impl Bookmarks {
    pub fn path() -> PathBuf {
        default_cache_root().join("bookmarks.json")
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

    pub fn add(&mut self, video_id: &str, title: &str, t: f64, label: &str) {
        if video_id.is_empty() {
            return;
        }
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.items.push(Bookmark {
            video_id: video_id.into(),
            title: title.into(),
            t,
            label: if label.is_empty() {
                format!("{:.0}s", t)
            } else {
                label.into()
            },
            created,
        });
        // keep last 80
        if self.items.len() > 80 {
            let n = self.items.len() - 80;
            self.items.drain(0..n);
        }
        self.save();
    }

    pub fn for_video<'a>(&'a self, video_id: &str) -> Vec<&'a Bookmark> {
        self.items
            .iter()
            .filter(|b| b.video_id == video_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_record() {
        let mut s = ListenStats::default();
        s.record_play("Test");
        assert_eq!(s.songs_played, 1);
        s.add_listen_secs(30.0);
        assert!((s.total_listen_secs - 30.0).abs() < 0.01);
    }

    #[test]
    fn bookmarks() {
        let mut b = Bookmarks::default();
        b.add("vid", "Song", 42.0, "chorus");
        assert_eq!(b.for_video("vid").len(), 1);
    }
}
