//! YouTube title parsing and URL helpers.

use regex::Regex;
use std::sync::OnceLock;

fn normalize_name(value: &str) -> String {
    let lower = value.to_lowercase();
    let re = Regex::new(r"[^\w\s]").unwrap();
    let cleaned = re.replace_all(&lower, " ");
    let space = Regex::new(r"\s+").unwrap();
    space.replace_all(&cleaned, " ").trim().to_string()
}

/// Fuzzy name equality (song / artist).
pub fn names_similar(a: &str, b: &str) -> bool {
    let a_n = normalize_name(a);
    let b_n = normalize_name(b);
    if a_n.is_empty() || b_n.is_empty() {
        return false;
    }
    if a_n == b_n || a_n.contains(&b_n) || b_n.contains(&a_n) {
        return true;
    }
    let ta: std::collections::HashSet<_> = a_n
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .collect();
    let tb: std::collections::HashSet<_> = b_n
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .collect();
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    let overlap = ta.intersection(&tb).count() as f64;
    let denom = ta.len().max(tb.len()) as f64;
    overlap / denom >= 0.72
}

/// Strip (Official Video), [Lyrics], etc.
pub fn clean_track_name(track: &str) -> String {
    let re = Regex::new(
        r"(?i)\s*[\(\[][^)\]]*(official|video|audio|lyrics|hd|4k|mv|visualizer|remaster|lyric|slowed|reverb)[^)\]]*[\)\]]",
    )
    .unwrap();
    let cleaned = re.replace_all(track, "");
    let space = Regex::new(r"\s+").unwrap();
    space
        .replace_all(&cleaned, " ")
        .trim_matches(|c: char| c == ' ' || c == '-' || c == '\t')
        .to_string()
}

/// Parse YouTube titles into `(track, artist)`.
///
/// Handles both:
/// - `Artist - Song`
/// - `Song - Artist`
pub fn split_title(title: &str, fallback_artist: &str) -> (String, String) {
    let junk = Regex::new(
        r"(?i)\s*[\(\[][^)\]]*(official|video|audio|lyrics|hd|4k|mv|visualizer|remaster|lyric|audio\s*only|slowed|reverb|nightcore)[^)\]]*[\)\]]",
    )
    .unwrap();
    let mut cleaned = junk.replace_all(title.trim(), "").to_string();
    let space = Regex::new(r"\s+").unwrap();
    cleaned = space.replace_all(&cleaned, " ").trim().to_string();

    let fa = Regex::new(r"(?i)\s*-\s*Topic\s*$")
        .unwrap()
        .replace(fallback_artist, "")
        .trim()
        .to_string();

    for sep in [" — ", " – ", " - ", " | ", " ~ "] {
        if let Some((left, right)) = cleaned.split_once(sep) {
            let left = left.trim();
            let right = right.trim();
            if left.is_empty() || right.is_empty() {
                continue;
            }
            let left_is_artist = !fa.is_empty() && names_similar(left, &fa);
            let right_is_artist = !fa.is_empty() && names_similar(right, &fa);

            if right_is_artist && !left_is_artist {
                return (left.to_string(), right.to_string());
            }
            if left_is_artist && !right_is_artist {
                return (right.to_string(), left.to_string());
            }
            // Longer multi-word side is usually the artist
            if right.split_whitespace().count() > left.split_whitespace().count()
                && right.split_whitespace().count() >= 2
            {
                return (left.to_string(), right.to_string());
            }
            return (right.to_string(), left.to_string());
        }
    }
    (cleaned, fa)
}

/// True for http(s) YouTube watch / short / youtu.be URLs.
pub fn is_youtube_url(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)^https?://(www\.)?(youtube\.com|youtu\.be|youtube-nocookie\.com|music\.youtube\.com)([/?#]|$)",
        )
        .unwrap()
    });
    re.is_match(value)
}

/// Best-effort extract YouTube video id.
pub fn video_id_from_url(url: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?:v=|/shorts/|youtu\.be/)([A-Za-z0-9_-]{6,})").unwrap()
    });
    re.captures(url.trim())
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_dash_artist() {
        let (track, artist) =
            split_title("Apocalypse - Cigarettes After Sex", "Cigarettes After Sex");
        let track = clean_track_name(&track);
        assert_eq!(track, "Apocalypse");
        assert!(names_similar(&artist, "Cigarettes After Sex"));
    }

    #[test]
    fn artist_dash_song() {
        let (track, _) =
            split_title("Cigarettes After Sex - Apocalypse", "Cigarettes After Sex");
        assert_eq!(clean_track_name(&track), "Apocalypse");
    }

    #[test]
    fn official_audio() {
        let (track, _) = split_title(
            "Apocalypse - Cigarettes After Sex (Official Audio)",
            "Cigarettes After Sex",
        );
        assert_eq!(clean_track_name(&track), "Apocalypse");
    }

    #[test]
    fn youtube_url() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc123"));
        assert!(!is_youtube_url("ftp://youtube.com/x"));
        assert!(!is_youtube_url("youtube"));
    }

    #[test]
    fn video_id() {
        assert_eq!(
            video_id_from_url("https://www.youtube.com/watch?v=sElE_BfQ67s").as_deref(),
            Some("sElE_BfQ67s")
        );
        assert_eq!(
            video_id_from_url("https://youtu.be/sElE_BfQ67s").as_deref(),
            Some("sElE_BfQ67s")
        );
    }
}

