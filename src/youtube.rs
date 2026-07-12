//! YouTube metadata + audio download via `yt-dlp` (same engine as the Python app).

use crate::title::{clean_track_name, split_title};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use which::which;

#[derive(Debug, Error)]
pub enum YoutubeError {
    #[error("yt-dlp not found on PATH — install: pip install -U yt-dlp")]
    MissingYtdlp,
    #[error("yt-dlp failed: {0}")]
    Ytdlp(String),
    #[error("invalid YouTube URL")]
    BadUrl,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(String),
}

#[derive(Debug, Clone)]
pub struct YoutubeMeta {
    pub url: String,
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub track: String,
    pub duration_s: f64,
}

#[derive(Debug, Deserialize)]
struct YtdlpJson {
    id: Option<String>,
    title: Option<String>,
    track: Option<String>,
    artist: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
}

fn ytdlp_bin() -> Result<PathBuf, YoutubeError> {
    which("yt-dlp")
        .or_else(|_| which("yt-dlp.exe"))
        .map_err(|_| YoutubeError::MissingYtdlp)
}

/// Extract metadata without downloading media.
pub fn extract_meta(url: &str) -> Result<YoutubeMeta, YoutubeError> {
    if !crate::title::is_youtube_url(url) {
        return Err(YoutubeError::BadUrl);
    }
    let bin = ytdlp_bin()?;
    let out = Command::new(bin)
        .args([
            "-J",
            "--no-playlist",
            "--no-warnings",
            "--skip-download",
            url,
        ])
        .output()?;
    if !out.status.success() {
        return Err(YoutubeError::Ytdlp(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let info: YtdlpJson =
        serde_json::from_slice(&out.stdout).map_err(|e| YoutubeError::Json(e.to_string()))?;

    let video_id = info.id.unwrap_or_default();
    let full_title = info.title.clone().unwrap_or_default();
    let uploader = info
        .artist
        .clone()
        .or(info.uploader.clone())
        .or(info.channel.clone())
        .unwrap_or_default();
    let uploader_clean = regex::Regex::new(r"(?i)\s*-\s*Topic\s*$")
        .unwrap()
        .replace(uploader.trim(), "")
        .trim()
        .to_string();

    let (mut track, mut artist) = if let Some(t) = info.track.filter(|t| !t.is_empty()) {
        (t, uploader_clean.clone())
    } else {
        split_title(&full_title, &uploader_clean)
    };
    if let Some(a) = info.artist.filter(|a| !a.is_empty()) {
        artist = a;
    }
    track = clean_track_name(&track);
    artist = regex::Regex::new(r"(?i)\s*-\s*Topic\s*$")
        .unwrap()
        .replace(&artist, "")
        .trim()
        .to_string();

    Ok(YoutubeMeta {
        url: info.webpage_url.unwrap_or_else(|| url.to_string()),
        video_id,
        title: full_title,
        artist,
        track,
        duration_s: info.duration.unwrap_or(0.0),
    })
}

/// Download best audio to `out_dir/{video_id}.mp3` (or cached existing file).
pub fn download_audio(url: &str, video_id: &str, out_dir: &Path) -> Result<PathBuf, YoutubeError> {
    std::fs::create_dir_all(out_dir)?;
    // Cache hit
    for ext in ["mp3", "m4a", "opus", "ogg", "webm", "wav"] {
        let p = out_dir.join(format!("{video_id}.{ext}"));
        if p.is_file() && p.metadata().map(|m| m.len() > 8_000).unwrap_or(false) {
            return Ok(p);
        }
    }

    let bin = ytdlp_bin()?;
    let outtmpl = out_dir
        .join(format!("{video_id}.%(ext)s"))
        .to_string_lossy()
        .to_string();

    let status = Command::new(&bin)
        .args([
            "-x",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "192",
            "--no-playlist",
            "--no-warnings",
            "-o",
            &outtmpl,
            url,
        ])
        .status()?;
    if !status.success() {
        // Retry without extract (raw best audio)
        let status2 = Command::new(&bin)
            .args([
                "-f",
                "bestaudio/best",
                "--no-playlist",
                "--no-warnings",
                "-o",
                &outtmpl,
                url,
            ])
            .status()?;
        if !status2.success() {
            return Err(YoutubeError::Ytdlp(
                "audio download failed (try: pip install -U yt-dlp)".into(),
            ));
        }
    }

    for ext in ["mp3", "m4a", "opus", "ogg", "webm", "wav", "mp4"] {
        let p = out_dir.join(format!("{video_id}.{ext}"));
        if p.is_file() && p.metadata().map(|m| m.len() > 8_000).unwrap_or(false) {
            return Ok(p);
        }
    }
    Err(YoutubeError::Ytdlp("downloaded file not found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_url() {
        assert!(extract_meta("not-a-url").is_err());
    }
}
