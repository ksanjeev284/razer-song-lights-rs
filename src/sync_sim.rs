//! Clock-sync helpers: which lyric line is active at song time `t`.
//! Uses binary search for O(log n) lookups on long LRC files.

use crate::lrc::TimedLine;

/// Active karaoke line index at song time `t` (with optional offset seconds).
/// Returns `-1` when no line has started yet.
pub fn line_index_for_time(lines: &[TimedLine], t: f64, offset_s: f64) -> isize {
    if lines.is_empty() {
        return -1;
    }
    let target = t + 0.05 - offset_s;
    // Find last line with line.t <= target
    let mut lo = 0usize;
    let mut hi = lines.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if lines[mid].t <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        -1
    } else {
        (lo - 1) as isize
    }
}

/// Binary search for last word with t+offset <= pos.
pub fn word_index_for_time(starts: &[f64], pos: f64, offset_s: f64) -> Option<usize> {
    if starts.is_empty() {
        return None;
    }
    let target = pos + 0.02 - offset_s;
    let mut lo = 0usize;
    let mut hi = starts.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if starts[mid] <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        None
    } else {
        Some(lo - 1)
    }
}

/// Sample active indices across the lyric span; must be non-decreasing.
pub fn simulate_sync(lines: &[TimedLine], offset_s: f64) -> Result<Vec<(f64, isize)>, String> {
    if lines.is_empty() {
        return Err("no timed lines".into());
    }
    let span0 = lines[0].t;
    let span1 = lines.last().unwrap().t;
    let mut samples = Vec::new();
    for frac in [0.0, 0.25, 0.5, 0.75, 0.99] {
        let t = span0 + (span1 - span0) * frac;
        let idx = line_index_for_time(lines, t, offset_s);
        if idx < 0 || idx as usize >= lines.len() {
            return Err(format!("bad index at t={t:.1}: {idx}"));
        }
        if lines[idx as usize].t > t + 0.1 {
            return Err(format!(
                "line {} starts {:.1} > t={t:.1}",
                idx, lines[idx as usize].t
            ));
        }
        samples.push((t, idx));
    }
    let idxs: Vec<_> = samples.iter().map(|(_, i)| *i).collect();
    let mut sorted = idxs.clone();
    sorted.sort();
    if idxs != sorted {
        return Err(format!("indices not non-decreasing: {idxs:?}"));
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc::parse_lrc_lines;

    #[test]
    fn index_progresses() {
        let lrc = "[00:05.00]a\n[00:10.00]b\n[00:15.00]c\n";
        let lines = parse_lrc_lines(lrc);
        assert_eq!(line_index_for_time(&lines, 0.0, 0.0), -1);
        assert_eq!(line_index_for_time(&lines, 5.0, 0.0), 0);
        assert_eq!(line_index_for_time(&lines, 10.5, 0.0), 1);
        assert_eq!(line_index_for_time(&lines, 20.0, 0.0), 2);
        assert!(simulate_sync(&lines, 0.0).is_ok());
    }

    #[test]
    fn binary_matches_linear_on_dense() {
        let mut lrc = String::new();
        for i in 0..200 {
            lrc.push_str(&format!("[00:{:02}.{:02}]line {i}\n", i / 4, (i % 4) * 25));
        }
        // simpler dense
        let mut lines = Vec::new();
        for i in 0..500 {
            lines.push(TimedLine {
                t: i as f64 * 0.5,
                text: format!("l{i}"),
                end_t: i as f64 * 0.5 + 0.4,
            });
        }
        for t in [0.0, 1.0, 50.0, 100.0, 249.0] {
            let bi = line_index_for_time(&lines, t, 0.0);
            let mut li = -1isize;
            for (i, ln) in lines.iter().enumerate() {
                if t + 0.05 >= ln.t {
                    li = i as isize;
                } else {
                    break;
                }
            }
            assert_eq!(bi, li, "mismatch at t={t}");
        }
    }

    #[test]
    fn word_index() {
        let starts = vec![1.0, 2.0, 3.0, 10.0];
        assert_eq!(word_index_for_time(&starts, 0.0, 0.0), None);
        assert_eq!(word_index_for_time(&starts, 1.0, 0.0), Some(0));
        assert_eq!(word_index_for_time(&starts, 2.5, 0.0), Some(1));
        assert_eq!(word_index_for_time(&starts, 15.0, 0.0), Some(3));
    }
}
