//! Client for https://lrclib.net — synced + plain lyrics.

use crate::lrc::lrc_quality_score;
use crate::lyrics_match::{clean_lyrics, hit_matches_track, LyricsHit};
use crate::title::names_similar;
use serde::Deserialize;
use thiserror::Error;

const USER_AGENT: &str = "RazerSongLights-rs/0.1 (local; lyrics for keyboard light show)";

#[derive(Debug, Error)]
pub enum LyricsError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("No lyrics found for {artist} — {track}")]
    NotFound { artist: String, track: String },
    #[error("JSON error: {0}")]
    Json(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibRecord {
    id: Option<i64>,
    track_name: Option<String>,
    artist_name: Option<String>,
    duration: Option<f64>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
    instrumental: Option<bool>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(25))
        .build()
}

fn hit_from_record(rec: &LrclibRecord, source: &str) -> Option<LyricsHit> {
    let synced = rec
        .synced_lyrics
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let plain = if let Some(p) = &rec.plain_lyrics {
        clean_lyrics(p)
    } else if let Some(s) = &synced {
        clean_lyrics(s)
    } else {
        return None;
    };
    if plain.len() < 20 {
        return None;
    }
    let synced_lrc = synced.filter(|s| s.contains('['));
    let src = if synced_lrc.is_some() {
        format!("{source} (synced)")
    } else {
        source.to_string()
    };
    Some(LyricsHit {
        plain,
        synced_lrc,
        source: src,
    })
}

fn score_record(rec: &LrclibRecord, artist: &str, track: &str, duration_s: f64) -> f64 {
    let mut s = 0.0;
    let a = rec.artist_name.as_deref().unwrap_or("");
    let t = rec.track_name.as_deref().unwrap_or("");
    if !track.is_empty() && !t.is_empty() {
        if normalize(track) == normalize(t) {
            s += 12.0;
        } else if names_similar(track, t) {
            s += 8.0;
        } else {
            s -= 12.0;
        }
    }
    if !artist.is_empty() && !a.is_empty() {
        if names_similar(artist, a) {
            s += 6.0;
        } else {
            s -= 4.0;
        }
    }
    if rec
        .synced_lyrics
        .as_ref()
        .map(|x| x.contains('['))
        .unwrap_or(false)
    {
        s += 4.0;
    } else if rec.plain_lyrics.is_some() {
        s += 1.0;
    }
    if rec.instrumental.unwrap_or(false) {
        s -= 8.0;
    }
    if duration_s > 0.0 {
        if let Some(d) = rec.duration {
            let diff = (d - duration_s).abs();
            if diff <= 5.0 {
                s += 4.0;
            } else if diff <= 15.0 {
                s += 2.0;
            } else if diff > 60.0 {
                s -= 3.0;
            }
        }
    }
    // Prefer denser LRC for same identity
    if let Some(lrc) = &rec.synced_lyrics {
        s += lrc_quality_score(lrc, duration_s) * 0.05;
    }
    s
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Search + get best match from lrclib.
pub fn fetch_lyrics(artist: &str, track: &str, duration_s: f64) -> Result<LyricsHit, LyricsError> {
    if track.trim().is_empty() {
        return Err(LyricsError::NotFound {
            artist: artist.into(),
            track: track.into(),
        });
    }
    // Refuse track≈artist (broken parse)
    if !artist.is_empty() && names_similar(track, artist) {
        return Err(LyricsError::NotFound {
            artist: artist.into(),
            track: track.into(),
        });
    }

    let agent = agent();

    // 1) Exact-ish get when duration known
    if !artist.is_empty() && duration_s > 0.0 {
        let url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}&duration={}",
            urlencoding(artist),
            urlencoding(track),
            duration_s.round() as i64
        );
        if let Ok(resp) = agent.get(&url).call() {
            if let Ok(rec) = resp.into_json::<LrclibRecord>() {
                if let Some(hit) = hit_from_record(&rec, "lrclib.net") {
                    if hit_matches_track(&hit, artist, track) {
                        return Ok(hit);
                    }
                }
            }
        }
    }

    // 2) Search
    let q = format!("{} {}", artist, track).trim().to_string();
    let url = format!("https://lrclib.net/api/search?q={}", urlencoding(&q));
    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| LyricsError::Http(e.to_string()))?;
    let results: Vec<LrclibRecord> = resp
        .into_json()
        .map_err(|e| LyricsError::Json(e.to_string()))?;

    let mut ranked: Vec<_> = results.iter().collect();
    ranked.sort_by(|a, b| {
        score_record(b, artist, track, duration_s)
            .partial_cmp(&score_record(a, artist, track, duration_s))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for rec in ranked {
        if score_record(rec, artist, track, duration_s) < 6.0 {
            continue;
        }
        // Fetch full body by id if missing lyrics
        let mut full = rec.clone_shallow();
        if full.plain_lyrics.is_none() && full.synced_lyrics.is_none() {
            if let Some(id) = rec.id {
                let detail_url = format!("https://lrclib.net/api/get/{id}");
                if let Ok(r) = agent.get(&detail_url).call() {
                    if let Ok(detail) = r.into_json::<LrclibRecord>() {
                        full = detail;
                    }
                }
            }
        }
        if let Some(hit) = hit_from_record(&full, "lrclib.net") {
            if hit_matches_track(&hit, artist, track) {
                return Ok(hit);
            }
        }
    }

    Err(LyricsError::NotFound {
        artist: artist.into(),
        track: track.into(),
    })
}

// Minimal clone without deriving Clone on everything twice
trait CloneShallow {
    fn clone_shallow(&self) -> Self;
}
impl CloneShallow for LrclibRecord {
    fn clone_shallow(&self) -> Self {
        Self {
            id: self.id,
            track_name: self.track_name.clone(),
            artist_name: self.artist_name.clone(),
            duration: self.duration,
            plain_lyrics: self.plain_lyrics.clone(),
            synced_lyrics: self.synced_lyrics.clone(),
            instrumental: self.instrumental,
        }
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
