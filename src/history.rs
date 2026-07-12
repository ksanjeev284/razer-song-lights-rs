//! Play history for Prev / Next (Spotify-style).

use crate::cache::{load_song_package, SongPackage};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    video_ids: Vec<String>,
}

pub fn history_path(root: &Path) -> PathBuf {
    root.join("play_history.json")
}

pub fn load_history_ids(root: &Path) -> Vec<String> {
    let path = history_path(root);
    let Ok(data) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<HistoryFile>(&data)
        .map(|h| h.video_ids)
        .unwrap_or_default()
}

pub fn push_history(root: &Path, video_id: &str, max_items: usize) -> Vec<String> {
    if video_id.is_empty() {
        return load_history_ids(root);
    }
    let mut ids: Vec<String> = load_history_ids(root)
        .into_iter()
        .filter(|id| id != video_id)
        .collect();
    ids.push(video_id.to_string());
    if ids.len() > max_items {
        let skip = ids.len() - max_items;
        ids = ids[skip..].to_vec();
    }
    let _ = fs::create_dir_all(root);
    let _ = fs::write(
        history_path(root),
        serde_json::to_string_pretty(&HistoryFile {
            video_ids: ids.clone(),
        })
        .unwrap_or_default(),
    );
    ids
}

pub fn load_history_packages(root: &Path, limit: usize) -> Vec<SongPackage> {
    let ids = load_history_ids(root);
    let start = ids.len().saturating_sub(limit);
    ids[start..]
        .iter()
        .filter_map(|id| load_song_package(root, id))
        .collect()
}

/// Session queue with cursor for Prev/Next.
#[derive(Debug, Default, Clone)]
pub struct PlayQueue {
    pub songs: Vec<SongPackage>,
    pub index: isize, // -1 = empty
}

impl PlayQueue {
    pub fn current(&self) -> Option<&SongPackage> {
        if self.index >= 0 {
            self.songs.get(self.index as usize)
        } else {
            None
        }
    }

    pub fn push(&mut self, song: SongPackage) {
        // Drop forward history when branching
        if self.index >= 0 && (self.index as usize) + 1 < self.songs.len() {
            self.songs.truncate(self.index as usize + 1);
        }
        if let Some(last) = self.songs.last() {
            if (!song.video_id.is_empty() && last.video_id == song.video_id)
                || last.display_name() == song.display_name()
            {
                let i = self.songs.len() - 1;
                self.songs[i] = song;
                self.index = i as isize;
                return;
            }
        }
        if !song.video_id.is_empty() {
            self.songs
                .retain(|s| s.video_id.is_empty() || s.video_id != song.video_id);
        }
        self.songs.push(song);
        self.index = self.songs.len() as isize - 1;
    }

    pub fn prev(&mut self) -> Option<&SongPackage> {
        if self.index > 0 {
            self.index -= 1;
            self.current()
        } else {
            self.current()
        }
    }

    pub fn next(&mut self) -> Option<&SongPackage> {
        if self.index >= 0 && (self.index as usize) + 1 < self.songs.len() {
            self.index += 1;
            self.current()
        } else {
            None
        }
    }

    pub fn can_prev(&self) -> bool {
        self.index > 0
    }

    pub fn can_next(&self) -> bool {
        self.index >= 0 && (self.index as usize) + 1 < self.songs.len()
    }

    /// Next index, optionally shuffled (not current).
    pub fn next_index(&self, shuffle: bool) -> Option<usize> {
        if self.songs.is_empty() {
            return None;
        }
        if shuffle && self.songs.len() > 1 {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            self.index.hash(&mut h);
            self.songs.len().hash(&mut h);
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
                .hash(&mut h);
            let mut idx = (h.finish() as usize) % self.songs.len();
            if idx as isize == self.index {
                idx = (idx + 1) % self.songs.len();
            }
            return Some(idx);
        }
        if self.can_next() {
            Some(self.index as usize + 1)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::SongPackage;
    use std::env;

    fn sample(id: &str, name: &str) -> SongPackage {
        SongPackage {
            url: format!("https://youtu.be/{id}"),
            video_id: id.into(),
            title: name.into(),
            artist: "A".into(),
            track: name.into(),
            duration_s: 100.0,
            lyrics: "line one line two more words here ok".into(),
            source: "t".into(),
            synced_lrc: None,
            audio_path: None,
        }
    }

    #[test]
    fn queue_prev_next() {
        let mut q = PlayQueue::default();
        q.push(sample("aaa111", "One"));
        q.push(sample("bbb222", "Two"));
        assert!(q.can_prev());
        assert!(!q.can_next());
        assert_eq!(q.current().unwrap().track, "Two");
        q.prev();
        assert_eq!(q.current().unwrap().track, "One");
        assert!(q.can_next());
        q.next();
        assert_eq!(q.current().unwrap().track, "Two");
    }

    #[test]
    fn history_file_roundtrip() {
        let root = env::temp_dir().join(format!("rsl_hist_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        push_history(&root, "vid1", 40);
        push_history(&root, "vid2", 40);
        push_history(&root, "vid1", 40);
        let ids = load_history_ids(&root);
        assert_eq!(ids.last().map(|s| s.as_str()), Some("vid1"));
        assert_eq!(ids.iter().filter(|x| *x == "vid1").count(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
