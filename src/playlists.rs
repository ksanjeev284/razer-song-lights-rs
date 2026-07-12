//! Named playlists of YouTube URLs (save / load).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cache::default_cache_root;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NamedPlaylist {
    pub name: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistStore {
    pub lists: Vec<NamedPlaylist>,
}

impl PlaylistStore {
    pub fn path() -> PathBuf {
        default_cache_root().join("playlists.json")
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

    pub fn save_named(&mut self, name: &str, urls: Vec<String>) {
        let name = name.trim();
        if name.is_empty() || urls.is_empty() {
            return;
        }
        if let Some(p) = self.lists.iter_mut().find(|p| p.name == name) {
            p.urls = urls;
        } else {
            self.lists.push(NamedPlaylist {
                name: name.to_string(),
                urls,
            });
        }
        // keep last 30
        if self.lists.len() > 30 {
            let n = self.lists.len() - 30;
            self.lists.drain(0..n);
        }
        self.save();
    }

    pub fn get(&self, name: &str) -> Option<&NamedPlaylist> {
        self.lists.iter().find(|p| p.name == name)
    }

    pub fn remove(&mut self, name: &str) {
        self.lists.retain(|p| p.name != name);
        self.save();
    }

    pub fn names(&self) -> Vec<String> {
        self.lists.iter().map(|p| p.name.clone()).collect()
    }
}

/// Build minimal LRC from timed lines (for export of estimated timing).
pub fn timed_lines_to_lrc(lines: &[(f64, &str)]) -> String {
    let mut out = String::from("[re:Razer Song Lights]\n");
    for (t, text) in lines {
        let total_cs = (*t * 100.0).round().max(0.0) as i64;
        let m = total_cs / 6000;
        let s = (total_cs % 6000) / 100;
        let cs = total_cs % 100;
        out.push_str(&format!("[{m:02}:{s:02}.{cs:02}]{text}\n"));
    }
    out
}

/// Snap position to nearest lyric time.
pub fn snap_to_nearest_lyric(pos: f64, times: &[f64]) -> Option<f64> {
    if times.is_empty() {
        return None;
    }
    let mut best = times[0];
    let mut best_d = (pos - best).abs();
    for &t in times.iter().skip(1) {
        let d = (pos - t).abs();
        if d < best_d {
            best_d = d;
            best = t;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_named_playlist() {
        let mut s = PlaylistStore::default();
        s.save_named("chill", vec!["https://youtu.be/a".into()]);
        assert_eq!(s.get("chill").unwrap().urls.len(), 1);
        s.save_named("chill", vec!["https://youtu.be/b".into()]);
        assert_eq!(s.get("chill").unwrap().urls[0], "https://youtu.be/b");
    }

    #[test]
    fn lrc_export_format() {
        let lrc = timed_lines_to_lrc(&[(1.5, "hello"), (65.0, "world")]);
        assert!(lrc.contains("[00:01.50]hello"));
        assert!(lrc.contains("[01:05.00]world"));
    }

    #[test]
    fn snap_lyric() {
        let times = [10.0, 20.0, 30.0];
        assert!((snap_to_nearest_lyric(21.0, &times).unwrap() - 20.0).abs() < 0.01);
        assert!((snap_to_nearest_lyric(5.0, &times).unwrap() - 10.0).abs() < 0.01);
    }
}
