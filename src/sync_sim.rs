//! Clock-sync helpers: which lyric line is active at song time `t`.

use crate::lrc::TimedLine;

/// Active karaoke line index at song time `t` (with optional offset seconds).
/// Returns `-1` when no line has started yet.
pub fn line_index_for_time(lines: &[TimedLine], t: f64, offset_s: f64) -> isize {
    let mut active: isize = -1;
    for (i, ln) in lines.iter().enumerate() {
        if t + 0.05 >= ln.t + offset_s {
            active = i as isize;
        } else {
            break;
        }
    }
    active
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
                idx,
                lines[idx as usize].t
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
}

