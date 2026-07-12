//! YouTube metadata + audio download via `yt-dlp` (same engine as the Python app).
//!
//! YouTube often requires authentication ("Please sign in"). We support:
//! - Netscape `cookies.txt` (`--cookies`)
//! - Browser cookie import (`--cookies-from-browser chrome|edge|firefox|brave`)
//! - Client fallbacks via `--extractor-args youtube:player_client=...`
//!
//! Resolution order: explicit opts → env → settings file → default cache paths.

use crate::cache::default_cache_root;
use crate::title::{clean_track_name, split_title};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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

/// How yt-dlp should authenticate with YouTube.
#[derive(Debug, Clone, Default)]
pub struct YtdlpAuth {
    /// Path to Netscape cookies.txt
    pub cookies_file: Option<PathBuf>,
    /// Browser name for `--cookies-from-browser` (chrome, edge, firefox, brave, chromium, …)
    pub cookies_from_browser: Option<String>,
}

impl YtdlpAuth {
    /// Load from env + optional settings fields + default cookie file locations.
    pub fn resolve(cookies_file: Option<&str>, cookies_browser: Option<&str>) -> Self {
        let mut auth = Self::default();

        // Env overrides everything
        if let Ok(p) = std::env::var("YTDLP_COOKIES") {
            let p = p.trim();
            if !p.is_empty() {
                auth.cookies_file = Some(PathBuf::from(p));
            }
        }
        if let Ok(b) = std::env::var("YTDLP_COOKIES_FROM_BROWSER") {
            let b = b.trim().to_lowercase();
            if !b.is_empty() && b != "none" && b != "off" {
                auth.cookies_from_browser = Some(b);
            }
        }

        // Settings (if env not set)
        if auth.cookies_file.is_none() {
            if let Some(p) = cookies_file.map(str::trim).filter(|s| !s.is_empty()) {
                auth.cookies_file = Some(PathBuf::from(p));
            }
        }
        if auth.cookies_from_browser.is_none() {
            if let Some(b) = cookies_browser
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty() && s != "none" && s != "off")
            {
                auth.cookies_from_browser = Some(b);
            }
        }

        // Default cookie file in app cache if present
        if auth.cookies_file.is_none() {
            let candidates = [
                default_cache_root().join("youtube_cookies.txt"),
                default_cache_root().join("cookies.txt"),
            ];
            for c in candidates {
                if c.is_file() {
                    auth.cookies_file = Some(c);
                    break;
                }
            }
        }

        auth
    }

    fn push_args(&self, args: &mut Vec<String>) {
        if let Some(p) = &self.cookies_file {
            if p.is_file() {
                args.push("--cookies".into());
                args.push(p.display().to_string());
            }
        }
        if let Some(b) = &self.cookies_from_browser {
            args.push("--cookies-from-browser".into());
            args.push(b.clone());
        }
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(p) = &self.cookies_file {
            if p.is_file() {
                parts.push(format!("cookies file: {}", p.display()));
            }
        }
        if let Some(b) = &self.cookies_from_browser {
            parts.push(format!("browser: {b}"));
        }
        if parts.is_empty() {
            "no cookies configured".into()
        } else {
            parts.join(" · ")
        }
    }
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

fn run_ytdlp(bin: &Path, args: &[String]) -> Result<Output, YoutubeError> {
    Command::new(bin)
        .args(args)
        .output()
        .map_err(YoutubeError::Io)
}

fn stderr_msg(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let out_s = String::from_utf8_lossy(&out.stdout);
    let mut s = err.trim().to_string();
    if s.is_empty() {
        s = out_s.trim().to_string();
    }
    // Keep last useful lines (yt-dlp is verbose)
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > 6 {
        lines[lines.len() - 6..].join("\n")
    } else {
        s
    }
}

fn is_signin_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("sign in")
        || m.contains("not a bot")
        || m.contains("cookies")
        || m.contains("login required")
        || m.contains("confirm you")
}

fn signin_help(auth: &YtdlpAuth) -> String {
    format!(
        "YouTube requires sign-in (bot check).\n\
         Current auth: {}.\n\n\
         Fix options:\n\
         1) In the app: set Cookies browser to Chrome/Edge (while logged into YouTube),\n\
         2) Or export Netscape cookies.txt → place at:\n\
            {}\n\
         3) Or set env YTDLP_COOKIES / YTDLP_COOKIES_FROM_BROWSER.\n\
         Also run: pip install -U yt-dlp",
        auth.describe(),
        default_cache_root().join("youtube_cookies.txt").display()
    )
}

fn base_args(auth: &YtdlpAuth) -> Vec<String> {
    let mut args = vec![
        "--no-playlist".into(),
        "--no-warnings".into(),
        // Prefer modern mobile/TV clients; reduces some bot challenges.
        "--extractor-args".into(),
        "youtube:player_client=android,web".into(),
    ];
    auth.push_args(&mut args);
    args
}

/// Extract metadata without downloading media.
pub fn extract_meta(url: &str) -> Result<YoutubeMeta, YoutubeError> {
    extract_meta_with_auth(url, &YtdlpAuth::resolve(None, None))
}

pub fn extract_meta_with_auth(url: &str, auth: &YtdlpAuth) -> Result<YoutubeMeta, YoutubeError> {
    if !crate::title::is_youtube_url(url) {
        return Err(YoutubeError::BadUrl);
    }
    let bin = ytdlp_bin()?;
    let mut args = base_args(auth);
    args.extend(["-J".into(), "--skip-download".into(), url.to_string()]);

    let out = run_ytdlp(&bin, &args)?;
    if !out.status.success() {
        let msg = stderr_msg(&out);
        // Retry with browser cookies if not already using them
        if is_signin_error(&msg)
            && auth.cookies_from_browser.is_none()
            && auth.cookies_file.is_none()
        {
            for browser in ["chrome", "edge", "firefox", "brave"] {
                let mut try_auth = auth.clone();
                try_auth.cookies_from_browser = Some(browser.into());
                let mut a2 = base_args(&try_auth);
                a2.extend(["-J".into(), "--skip-download".into(), url.to_string()]);
                let out2 = run_ytdlp(&bin, &a2)?;
                if out2.status.success() {
                    return parse_meta_json(&out2.stdout, url);
                }
            }
            return Err(YoutubeError::Ytdlp(format!(
                "{msg}\n\n{}",
                signin_help(auth)
            )));
        }
        if is_signin_error(&msg) {
            return Err(YoutubeError::Ytdlp(format!(
                "{msg}\n\n{}",
                signin_help(auth)
            )));
        }
        return Err(YoutubeError::Ytdlp(msg));
    }
    parse_meta_json(&out.stdout, url)
}

fn parse_meta_json(stdout: &[u8], url: &str) -> Result<YoutubeMeta, YoutubeError> {
    let info: YtdlpJson =
        serde_json::from_slice(stdout).map_err(|e| YoutubeError::Json(e.to_string()))?;

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
    download_audio_with_auth(url, video_id, out_dir, &YtdlpAuth::resolve(None, None))
}

pub fn download_audio_with_auth(
    url: &str,
    video_id: &str,
    out_dir: &Path,
    auth: &YtdlpAuth,
) -> Result<PathBuf, YoutubeError> {
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

    // Attempt list: primary extract-mp3, then bestaudio, each with optional browser retry.
    let attempts: Vec<Vec<String>> = {
        let mut list = Vec::new();
        let mut a = base_args(auth);
        a.extend([
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--audio-quality".into(),
            "192".into(),
            "-o".into(),
            outtmpl.clone(),
            url.to_string(),
        ]);
        list.push(a);

        let mut b = base_args(auth);
        b.extend([
            "-f".into(),
            "bestaudio/best".into(),
            "-o".into(),
            outtmpl.clone(),
            url.to_string(),
        ]);
        list.push(b);

        // If no auth, try chrome/edge automatically as last resorts
        if auth.cookies_from_browser.is_none() && auth.cookies_file.is_none() {
            for browser in ["chrome", "edge", "firefox"] {
                let mut try_auth = auth.clone();
                try_auth.cookies_from_browser = Some(browser.into());
                let mut c = base_args(&try_auth);
                c.extend([
                    "-x".into(),
                    "--audio-format".into(),
                    "mp3".into(),
                    "--audio-quality".into(),
                    "192".into(),
                    "-o".into(),
                    outtmpl.clone(),
                    url.to_string(),
                ]);
                list.push(c);
            }
        }
        list
    };

    let mut last_err = String::new();
    for args in attempts {
        let out = run_ytdlp(&bin, &args)?;
        if out.status.success() {
            if let Some(p) = find_downloaded(out_dir, video_id) {
                return Ok(p);
            }
        } else {
            last_err = stderr_msg(&out);
        }
    }

    if is_signin_error(&last_err) {
        return Err(YoutubeError::Ytdlp(format!(
            "{last_err}\n\n{}",
            signin_help(auth)
        )));
    }
    if last_err.is_empty() {
        last_err = "audio download failed (try: pip install -U yt-dlp)".into();
    }
    Err(YoutubeError::Ytdlp(last_err))
}

fn find_downloaded(out_dir: &Path, video_id: &str) -> Option<PathBuf> {
    for ext in ["mp3", "m4a", "opus", "ogg", "webm", "wav", "mp4"] {
        let p = out_dir.join(format!("{video_id}.{ext}"));
        if p.is_file() && p.metadata().map(|m| m.len() > 8_000).unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// Browser options shown in the GUI.
pub const COOKIE_BROWSERS: &[&str] = &["none", "chrome", "edge", "firefox", "brave", "chromium"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_url() {
        assert!(extract_meta("not-a-url").is_err());
    }

    #[test]
    fn signin_detect() {
        assert!(is_signin_error(
            "ERROR: [youtube] abc: Please sign in to confirm you're not a bot"
        ));
        assert!(!is_signin_error("video unavailable"));
    }

    #[test]
    fn auth_resolve_env() {
        // just ensure default doesn't panic
        let a = YtdlpAuth::resolve(None, Some("chrome"));
        assert_eq!(a.cookies_from_browser.as_deref(), Some("chrome"));
    }
}
