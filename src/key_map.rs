//! Character → approximate Chroma 6×22 matrix positions (BlackWidow-style).

/// Rows × columns of the Razer keyboard grid.
pub const MAX_ROW: usize = 6;
pub const MAX_COL: usize = 22;

/// Map a character to a Chroma grid cell, if known.
pub fn char_to_position(ch: char) -> Option<(usize, usize)> {
    let c = ch.to_ascii_lowercase();
    match c {
        '1' => Some((1, 2)),
        '2' => Some((1, 3)),
        '3' => Some((1, 4)),
        '4' => Some((1, 5)),
        '5' => Some((1, 6)),
        '6' => Some((1, 7)),
        '7' => Some((1, 8)),
        '8' => Some((1, 9)),
        '9' => Some((1, 10)),
        '0' => Some((1, 11)),
        'q' => Some((2, 2)),
        'w' => Some((2, 3)),
        'e' => Some((2, 4)),
        'r' => Some((2, 5)),
        't' => Some((2, 6)),
        'y' => Some((2, 7)),
        'u' => Some((2, 8)),
        'i' => Some((2, 9)),
        'o' => Some((2, 10)),
        'p' => Some((2, 11)),
        'a' => Some((3, 2)),
        's' => Some((3, 3)),
        'd' => Some((3, 4)),
        'f' => Some((3, 5)),
        'g' => Some((3, 6)),
        'h' => Some((3, 7)),
        'j' => Some((3, 8)),
        'k' => Some((3, 9)),
        'l' => Some((3, 10)),
        'z' => Some((4, 3)),
        'x' => Some((4, 4)),
        'c' => Some((4, 5)),
        'v' => Some((4, 6)),
        'b' => Some((4, 7)),
        'n' => Some((4, 8)),
        'm' => Some((4, 9)),
        ' ' => Some((5, 7)),
        _ => None,
    }
}

/// Split lyrics into words.
pub fn split_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Non-empty lyric lines.
pub fn split_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_map() {
        assert_eq!(char_to_position('a'), Some((3, 2)));
        assert_eq!(char_to_position('Q'), Some((2, 2)));
        assert!(char_to_position(' ').is_some());
        assert!(char_to_position('😀').is_none());
    }

    #[test]
    fn split() {
        assert_eq!(split_words("hello  world"), vec!["hello", "world"]);
        assert_eq!(split_lines("a\n\nb\n  \nc"), vec!["a", "b", "c"]);
    }
}

