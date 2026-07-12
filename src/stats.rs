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
    /// YYYY-MM-DD of last daily bucket.
    #[serde(default)]
    pub day_key: String,
    /// Seconds listened on `day_key`.
    #[serde(default)]
    pub day_listen_secs: f64,
    /// Daily goal in hours (0 = off).
    #[serde(default = "default_goal_hours")]
    pub daily_goal_hours: f32,
}

fn default_goal_hours() -> f32 {
    1.0
}

fn today_key() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Approximate UTC date; good enough for a soft daily goal.
    let days = secs / 86400;
    let y = 1970 + days / 365;
    let rem = days % 365;
    let m = rem / 30 + 1;
    let d = rem % 30 + 1;
    format!("{y:04}-{m:02}-{d:02}")
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
            let today = today_key();
            if self.day_key != today {
                self.day_key = today;
                self.day_listen_secs = 0.0;
            }
            self.day_listen_secs += secs;
            self.save();
        }
    }

    pub fn daily_progress(&self) -> (f64, f32) {
        let goal_h = if self.daily_goal_hours <= 0.0 {
            1.0
        } else {
            self.daily_goal_hours
        };
        let day = if self.day_key == today_key() {
            self.day_listen_secs
        } else {
            0.0
        };
        (day / 3600.0, goal_h)
    }

    pub fn daily_summary(&self) -> String {
        let (h, goal) = self.daily_progress();
        format!("Today {:.1}h / {goal:.1}h goal", h)
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

    /// CSV export for listen stats.
    pub fn to_csv(&self) -> String {
        format!(
            "songs_played,total_listen_secs,sessions,last_played\n{},{:.3},{},\"{}\"\n",
            self.songs_played,
            self.total_listen_secs,
            self.sessions,
            self.last_played.replace('"', "'")
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
        assert!(s.to_csv().contains("songs_played"));
    }

    #[test]
    fn bookmarks() {
        let mut b = Bookmarks::default();
        b.add("vid", "Song", 42.0, "chorus");
        assert_eq!(b.for_video("vid").len(), 1);
    }
}
