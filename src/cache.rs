//! Song package cache (JSON on disk).

use crate::lyrics_match::LyricsHit;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongPackage {
    pub url: String,
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub track: String,
    pub duration_s: f64,
    pub lyrics: String,
    pub source: String,
    pub synced_lrc: Option<String>,
    pub audio_path: Option<String>,
}

impl SongPackage {
    pub fn display_name(&self) -> String {
        if !self.artist.is_empty() && !self.track.is_empty() {
            format!("{} — {}", self.artist, self.track)
        } else {
            self.title.clone()
        }
    }

    pub fn is_synced(&self) -> bool {
        self.synced_lrc
            .as_ref()
            .map(|s| s.contains('['))
            .unwrap_or(false)
    }

    pub fn from_hit(
        artist: &str,
        track: &str,
        duration_s: f64,
        hit: &LyricsHit,
        video_id: &str,
        url: &str,
    ) -> Self {
        Self {
            url: url.into(),
            video_id: video_id.into(),
            title: format!("{artist} - {track}"),
            artist: artist.into(),
            track: track.into(),
            duration_s,
            lyrics: hit.plain.clone(),
            source: hit.source.clone(),
            synced_lrc: hit.synced_lrc.clone(),
            audio_path: None,
        }
    }
}

pub fn default_cache_root() -> PathBuf {
    std::env::temp_dir().join("razer_song_lights_rs")
}

pub fn song_meta_path(root: &Path, video_id: &str) -> PathBuf {
    root.join("songs").join(format!("{video_id}.json"))
}

pub fn save_song_package(root: &Path, song: &SongPackage) -> std::io::Result<()> {
    let dir = root.join("songs");
    fs::create_dir_all(&dir)?;
    let path = song_meta_path(root, &song.video_id);
    let data = serde_json::to_string_pretty(song)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, data)
}

pub fn load_song_package(root: &Path, video_id: &str) -> Option<SongPackage> {
    let path = song_meta_path(root, video_id);
    let data = fs::read_to_string(path).ok()?;
    let song: SongPackage = serde_json::from_str(&data).ok()?;
    // Invalidate track≈artist (broken cache)
    if crate::title::names_similar(&song.track, &song.artist) {
        let _ = fs::remove_file(song_meta_path(root, video_id));
        return None;
    }
    if song.lyrics.len() < 10 {
        return None;
    }
    Some(song)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn roundtrip() {
        let root = env::temp_dir().join(format!("rsl_rs_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let song = SongPackage {
            url: "https://youtu.be/abcXYZ12345".into(),
            video_id: "abcXYZ12345".into(),
            title: "Apocalypse - CAS".into(),
            artist: "Cigarettes After Sex".into(),
            track: "Apocalypse".into(),
            duration_s: 290.0,
            lyrics: "You leapt from crumbling bridges watching cityscapes".into(),
            source: "test".into(),
            synced_lrc: Some("[00:05.00]You leapt".into()),
            audio_path: None,
        };
        save_song_package(&root, &song).unwrap();
        let loaded = load_song_package(&root, "abcXYZ12345").unwrap();
        assert_eq!(loaded.track, "Apocalypse");
        assert!(loaded.is_synced());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalidates_track_eq_artist() {
        let root = env::temp_dir().join(format!("rsl_rs_bad_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let song = SongPackage {
            url: "".into(),
            video_id: "badbadbad01".into(),
            title: "x".into(),
            artist: "Cigarettes After Sex".into(),
            track: "Cigarettes After Sex".into(),
            duration_s: 100.0,
            lyrics: "Baby I'm a firefighter trapped in a house".into(),
            source: "t".into(),
            synced_lrc: None,
            audio_path: None,
        };
        save_song_package(&root, &song).unwrap();
        assert!(load_song_package(&root, "badbadbad01").is_none());
        let _ = fs::remove_dir_all(&root);
    }
}

