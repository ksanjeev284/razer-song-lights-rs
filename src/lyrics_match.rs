//! Lyrics cleaning and identity matching.

use crate::title::names_similar;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Plain lyrics + optional raw LRC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsHit {
    pub plain: String,
    pub synced_lrc: Option<String>,
    pub source: String,
}

impl LyricsHit {
    pub fn is_synced(&self) -> bool {
        self.synced_lrc
            .as_ref()
            .map(|s| s.contains('['))
            .unwrap_or(false)
    }
}

/// Normalize lyrics for display / keyboard words.
pub fn clean_lyrics(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let ts = Regex::new(r"\[\d{1,2}:\d{2}(?:\.\d+)?\]").unwrap();
    let mut text = ts.replace_all(text, "").to_string();
    let section = Regex::new(r"(?m)^\s*\[[^\]]+\]\s*$").unwrap();
    text = section.replace_all(&text, "").to_string();

    let mut out: Vec<String> = Vec::new();
    for line in text.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let s = line.trim();
        if s.is_empty() {
            if out.last().map(|l| !l.is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
            continue;
        }
        let low = s.to_lowercase();
        if low.starts_with("lyrics for ") || low.starts_with("source:") {
            continue;
        }
        out.push(s.to_string());
    }
    let cleaned = out.join("\n");
    let multi = Regex::new(r"\n{3,}").unwrap();
    multi.replace_all(&cleaned, "\n\n").trim().to_string()
}

fn lrc_metadata_tags(lrc: &str) -> (String, String) {
    let ti_re = Regex::new(r"(?i)\[ti:\s*([^\]]+)\]").unwrap();
    let ar_re = Regex::new(r"(?i)\[ar:\s*([^\]]+)\]").unwrap();
    let ti = ti_re
        .captures(lrc)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();
    let ar = ar_re
        .captures(lrc)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();
    (ti, ar)
}

/// Reject clearly wrong-song lyrics (same-artist random hits, ti: mismatch).
pub fn hit_matches_track(hit: &LyricsHit, artist: &str, track: &str) -> bool {
    if hit.plain.trim().is_empty() {
        return false;
    }
    let track_n = track.trim().to_lowercase();
    let artist_n = artist.trim().to_lowercase();
    if track_n.len() < 2 {
        return false;
    }
    if !artist_n.is_empty() && names_similar(track, artist) {
        return false;
    }
    if let Some(lrc) = &hit.synced_lrc {
        let (ti, ar) = lrc_metadata_tags(lrc);
        if !ti.is_empty() && !names_similar(&ti, track) {
            return false;
        }
        if !ar.is_empty()
            && !artist.is_empty()
            && !ti.is_empty()
            && !names_similar(&ar, artist)
            && !names_similar(&ti, track)
        {
            return false;
        }
    }
    true
}

/// Case-insensitive phrase containment (punctuation-insensitive).
pub fn contains_phrase(hay: &str, needle: &str) -> bool {
    fn norm(s: &str) -> String {
        let re = Regex::new(r"[^\w\s']").unwrap();
        let lower = s.to_lowercase();
        let stripped = re.replace_all(&lower, " ");
        let space = Regex::new(r"\s+").unwrap();
        space.replace_all(&stripped, " ").trim().to_string()
    }
    norm(hay).contains(&norm(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_timestamps() {
        let raw = "[00:12.00]Hello world\n[Verse 1]\nLine two\n";
        let out = clean_lyrics(raw);
        assert!(!out.contains("[00:"));
        assert!(!out.contains("Verse"));
        assert!(out.contains("Hello world"));
    }

    #[test]
    fn rejects_track_eq_artist() {
        let hit = LyricsHit {
            plain: "Baby, I'm a firefighter".into(),
            synced_lrc: None,
            source: "t".into(),
        };
        assert!(!hit_matches_track(
            &hit,
            "Cigarettes After Sex",
            "Cigarettes After Sex"
        ));
    }

    #[test]
    fn rejects_ti_mismatch() {
        let hit = LyricsHit {
            plain: "Baby I'm a firefighter".into(),
            synced_lrc: Some("[ti:I'm a Firefighter]\n[00:01.00]Baby".into()),
            source: "t".into(),
        };
        assert!(!hit_matches_track(
            &hit,
            "Cigarettes After Sex",
            "Apocalypse"
        ));
    }

    #[test]
    fn accepts_matching_ti() {
        let hit = LyricsHit {
            plain: "You leapt from crumbling bridges".into(),
            synced_lrc: Some(
                "[ti:Apocalypse]\n[ar:Cigarettes After Sex]\n[00:01.00]You leapt".into(),
            ),
            source: "t".into(),
        };
        assert!(hit_matches_track(
            &hit,
            "Cigarettes After Sex",
            "Apocalypse"
        ));
    }
}
