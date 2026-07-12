//! LRC (synced lyrics) parsing and timing helpers.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// One lyric line at song time `t` (seconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedLine {
    pub t: f64,
    pub text: String,
    pub end_t: f64,
}

/// Per-word event expanded from a timed line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedWord {
    pub t: f64,
    pub word: String,
    pub end_t: f64,
}

fn ts_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?\]").unwrap())
}

fn offset_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\[offset\s*:\s*([+-]?\d+)\]").unwrap())
}

/// Parse `[offset:±ms]` as seconds to add to timestamps.
pub fn parse_lrc_offset_s(lrc: &str) -> f64 {
    offset_re()
        .captures(lrc)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .map(|ms| ms as f64 / 1000.0)
        .unwrap_or(0.0)
}

fn stamp_to_seconds(mins: &str, secs: &str, frac: Option<&str>) -> f64 {
    let mins: f64 = mins.parse().unwrap_or(0.0);
    let secs: f64 = secs.parse().unwrap_or(0.0);
    let frac_s = match frac {
        None => 0.0,
        Some(f) if f.len() == 1 => f.parse::<f64>().unwrap_or(0.0) / 10.0,
        Some(f) if f.len() == 2 => f.parse::<f64>().unwrap_or(0.0) / 100.0,
        Some(f) => {
            let n = f.parse::<f64>().unwrap_or(0.0);
            n / 10f64.powi(f.len() as i32)
        }
    };
    mins * 60.0 + secs + frac_s
}

/// Extract timed lyric lines from LRC content (karaoke standard).
pub fn parse_lrc_lines(lrc: &str) -> Vec<TimedLine> {
    if lrc.trim().is_empty() {
        return Vec::new();
    }
    let offset = parse_lrc_offset_s(lrc);
    let meta = Regex::new(r"(?i)^(ar|ti|al|by|offset|re|ve|length):").unwrap();
    let mut raw: Vec<(f64, String)> = Vec::new();

    for line in lrc.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let stamps: Vec<_> = ts_re().captures_iter(line).collect();
        if stamps.is_empty() {
            continue;
        }
        let text = ts_re().replace_all(line, "").trim().to_string();
        if meta.is_match(&text) {
            continue;
        }
        if text.is_empty() || (text.starts_with('[') && text.ends_with(']')) {
            continue;
        }
        for cap in stamps {
            let mut t = stamp_to_seconds(
                cap.get(1).map(|m| m.as_str()).unwrap_or("0"),
                cap.get(2).map(|m| m.as_str()).unwrap_or("0"),
                cap.get(3).map(|m| m.as_str()),
            ) + offset;
            if t < 0.0 {
                t = 0.0;
            }
            raw.push((t, text.clone()));
        }
    }

    raw.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut dedup: Vec<(f64, String)> = Vec::new();
    for (t, text) in raw {
        if let Some((lt, lt_text)) = dedup.last() {
            if (lt - t).abs() < 0.02 && lt_text == &text {
                continue;
            }
        }
        dedup.push((t, text));
    }

    let n = dedup.len();
    let mut lines = Vec::with_capacity(n);
    for i in 0..n {
        let (t, text) = &dedup[i];
        let mut end = if i + 1 < n { dedup[i + 1].0 } else { t + 4.0 };
        if end <= *t {
            end = t + 2.0;
        }
        lines.push(TimedLine {
            t: *t,
            text: text.clone(),
            end_t: end,
        });
    }
    lines
}

/// Expand lines into word events spaced inside each line window.
pub fn lines_to_timed_words(lines: &[TimedLine], song_duration_s: f64) -> Vec<TimedWord> {
    let mut out = Vec::new();
    for line in lines {
        let words: Vec<&str> = line
            .text
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            continue;
        }
        let start = line.t;
        let mut end = if line.end_t > start {
            line.end_t
        } else {
            start + (0.5 * words.len() as f64).max(2.0)
        };
        if song_duration_s > 0.0 && end > song_duration_s {
            end = (start + 0.5).max(song_duration_s);
        }
        let span = (end - start).max(0.4) * 0.95;
        let slot = span / words.len() as f64;
        for (j, w) in words.iter().enumerate() {
            let t = start + j as f64 * slot;
            out.push(TimedWord {
                t,
                word: (*w).to_string(),
                end_t: t + slot * 0.9,
            });
        }
    }
    out
}

/// LRC quality score for ranking candidates (higher = better karaoke fit).
pub fn lrc_quality_score(lrc: &str, duration_s: f64) -> f64 {
    let lines = parse_lrc_lines(lrc);
    if lines.is_empty() {
        return -1.0;
    }
    let mut score = lines.len() as f64 * 2.0;
    if lines[0].t >= 5.0 {
        score += 3.0;
    } else if lines[0].t >= 1.0 {
        score += 1.0;
    }
    if duration_s > 20.0 {
        let last = lines.last().unwrap().t;
        let coverage = last / duration_s;
        if (0.55..=1.05).contains(&coverage) {
            score += 5.0;
        } else if (0.4..=1.15).contains(&coverage) {
            score += 2.0;
        }
        if coverage < 0.35 {
            score -= 4.0;
        }
    }
    score
}

/// Apply a sync offset (seconds) to timed lines.
pub fn apply_offset_lines(lines: &[TimedLine], offset_s: f64) -> Vec<TimedLine> {
    if offset_s == 0.0 {
        return lines.to_vec();
    }
    lines
        .iter()
        .map(|ln| TimedLine {
            t: (ln.t + offset_s).max(0.0),
            text: ln.text.clone(),
            end_t: if ln.end_t > 0.0 {
                (ln.end_t + offset_s).max(0.0)
            } else {
                0.0
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let lrc = "[00:05.00]hello world\n[00:10.50]second line\n";
        let lines = parse_lrc_lines(lrc);
        assert_eq!(lines.len(), 2);
        assert!((lines[0].t - 5.0).abs() < 0.01);
        assert!((lines[1].t - 10.5).abs() < 0.01);
        assert!((lines[0].end_t - 10.5).abs() < 0.01);
    }

    #[test]
    fn parse_offset() {
        let lrc = "[offset:+1000]\n[00:10.00]hi";
        let lines = parse_lrc_lines(lrc);
        assert!((lines[0].t - 11.0).abs() < 0.01);
    }

    #[test]
    fn empty() {
        assert!(parse_lrc_lines("").is_empty());
    }

    #[test]
    fn words_expand() {
        let lines = vec![TimedLine {
            t: 1.0,
            text: "hello world".into(),
            end_t: 3.0,
        }];
        let words = lines_to_timed_words(&lines, 100.0);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hello");
        assert!(words[1].t > words[0].t);
    }

    #[test]
    fn quality_prefers_coverage() {
        let short = (0..5)
            .map(|i| format!("[00:{i:02}.00]line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let long = (0..30)
            .map(|i| {
                let s = i * 8;
                format!("[{:02}:{:02}.00]line {i}", s / 60, s % 60)
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lrc_quality_score(&long, 250.0) > lrc_quality_score(&short, 250.0));
    }

    #[test]
    fn offset_clamp() {
        let lines = vec![TimedLine {
            t: 0.5,
            text: "x".into(),
            end_t: 1.0,
        }];
        let out = apply_offset_lines(&lines, -2.0);
        assert_eq!(out[0].t, 0.0);
    }
}
