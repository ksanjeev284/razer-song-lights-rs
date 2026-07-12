//! Coordinate audio + LRC karaoke lights + console lyric display.

use crate::audio::AudioPlayer;
use crate::chroma::{color_for_index, ChromaKeyboard};
use crate::lrc::{lines_to_timed_words, parse_lrc_lines, TimedLine, TimedWord};
use crate::sync_sim::line_index_for_time;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// Whole word lights at once (default)
    FlashWord,
    /// Whole lyric line at once
    FlashLine,
}

#[derive(Debug, Clone)]
pub struct ShowConfig {
    pub mode: PlayMode,
    pub volume: f32,
    pub sync_offset_s: f64,
    pub play_audio: bool,
    pub lights: bool,
    pub karaoke_console: bool,
}

impl Default for ShowConfig {
    fn default() -> Self {
        Self {
            mode: PlayMode::FlashWord,
            volume: 0.85,
            sync_offset_s: 0.0,
            play_audio: true,
            lights: true,
            karaoke_console: true,
        }
    }
}

/// Run a full light show until audio ends or stop flag is set.
pub fn run_show(
    lyrics_plain: &str,
    synced_lrc: Option<&str>,
    duration_s: f64,
    audio_path: Option<&std::path::Path>,
    cfg: &ShowConfig,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let lines: Vec<TimedLine> = if let Some(lrc) = synced_lrc {
        let parsed = parse_lrc_lines(lrc);
        if !parsed.is_empty() {
            parsed
        } else {
            estimated_lines(lyrics_plain, duration_s)
        }
    } else {
        estimated_lines(lyrics_plain, duration_s)
    };

    let words = lines_to_timed_words(&lines, duration_s);

    let mut chroma = if cfg.lights && crate::chroma::is_chroma_supported() {
        match ChromaKeyboard::open() {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("Chroma unavailable ({e}) — continuing without lights");
                None
            }
        }
    } else {
        if cfg.lights && !crate::chroma::is_chroma_supported() {
            eprintln!("Chroma SDK is Windows-only — lights skipped on this OS");
        }
        None
    };

    let audio = if cfg.play_audio {
        if let Some(path) = audio_path {
            let ap = AudioPlayer::new()?;
            ap.play(path, cfg.volume, 0.0)?;
            Some(ap)
        } else {
            eprintln!("No audio file — lights/lyrics will use wall-clock");
            None
        }
    } else {
        None
    };

    let wall0 = std::time::Instant::now();
    let mut word_i = 0usize;
    let mut line_i: isize = -1;
    let mut last_printed: isize = -2;
    let mut last_lit_line: isize = -99;

    println!();
    println!("▶  Playing…  Ctrl+C to stop");
    println!("   mode={:?}  lines={}  words={}  offset={:+.1}s",
        cfg.mode, lines.len(), words.len(), cfg.sync_offset_s);
    println!();

    while !stop.load(Ordering::SeqCst) {
        let pos = if let Some(ref a) = audio {
            if !a.is_playing() && a.get_position_s() > 1.0 {
                // track ended
                break;
            }
            a.get_position_s()
        } else {
            wall0.elapsed().as_secs_f64()
        };

        if duration_s > 0.0 && pos >= duration_s - 0.15 {
            break;
        }

        // Karaoke console line
        if cfg.karaoke_console && !lines.is_empty() {
            let idx = line_index_for_time(&lines, pos, cfg.sync_offset_s);
            if idx != last_printed {
                last_printed = idx;
                if idx >= 0 {
                    let ln = &lines[idx as usize];
                    // Clear-ish single line display
                    print!("\r\x1b[2K");
                    print!(
                        "♪ [{:5.1}s] {}",
                        pos,
                        if ln.text.len() > 72 {
                            format!("{}…", &ln.text[..70])
                        } else {
                            ln.text.clone()
                        }
                    );
                    let _ = io::stdout().flush();
                }
            }
            line_i = idx;
        }

        // Keyboard lights
        if let Some(ref mut kb) = chroma {
            match cfg.mode {
                PlayMode::FlashLine => {
                    if line_i != last_lit_line && line_i >= 0 {
                        last_lit_line = line_i;
                        let i = line_i as usize;
                        if i < lines.len() {
                            let color = color_for_index(i);
                            let _ = kb.flash_text_hit(&lines[i].text, color);
                        }
                    }
                }
                PlayMode::FlashWord => {
                    while word_i < words.len() {
                        let w = &words[word_i];
                        let target = w.t + cfg.sync_offset_s;
                        if pos + 0.02 < target {
                            break;
                        }
                        if pos > target + 0.55 {
                            // late — skip
                            word_i += 1;
                            continue;
                        }
                        let color = color_for_index(word_i);
                        let _ = kb.flash_text_hit(&w.word, color);
                        // hold briefly while still in word window
                        let hold_until = if w.end_t > w.t {
                            w.end_t + cfg.sync_offset_s
                        } else {
                            target + 0.35
                        };
                        let next_t = words
                            .get(word_i + 1)
                            .map(|n| n.t + cfg.sync_offset_s - 0.02)
                            .unwrap_or(hold_until);
                        let until = hold_until.min(next_t);
                        while !stop.load(Ordering::SeqCst) {
                            let now = if let Some(ref a) = audio {
                                a.get_position_s()
                            } else {
                                wall0.elapsed().as_secs_f64()
                            };
                            if now >= until {
                                break;
                            }
                            thread::sleep(Duration::from_millis(15));
                        }
                        kb.clear(0);
                        let _ = kb.push();
                        word_i += 1;
                    }
                    if word_i >= words.len() && audio.as_ref().map(|a| !a.is_playing()).unwrap_or(true)
                    {
                        // wait for audio tail or exit
                        if audio.is_none() {
                            break;
                        }
                    }
                }
            }
        } else if cfg.mode == PlayMode::FlashWord {
            // no lights — still advance words so console can finish
            while word_i < words.len()
                && pos >= words[word_i].t + cfg.sync_offset_s + 0.05
            {
                word_i += 1;
            }
            if word_i >= words.len() && audio.as_ref().map(|a| !a.is_playing()).unwrap_or(pos > duration_s) {
                break;
            }
        }

        thread::sleep(Duration::from_millis(20));
    }

    println!();
    if let Some(ref mut kb) = chroma {
        kb.clear(0);
        let _ = kb.push();
    }
    if let Some(a) = audio {
        a.stop();
    }
    println!("■  Stopped");
    Ok(())
}

fn estimated_lines(plain: &str, duration_s: f64) -> Vec<TimedLine> {
    let rows: Vec<String> = plain
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let dur = duration_s.max(20.0);
    let intro = (dur * 0.15).clamp(12.0, 40.0);
    let end = (dur - 3.0).max(intro + 5.0);
    let span = (end - intro).max(1.0);
    let slot = span / rows.len() as f64;
    rows.into_iter()
        .enumerate()
        .map(|(i, text)| {
            let t = intro + i as f64 * slot;
            TimedLine {
                t,
                text,
                end_t: t + slot * 0.92,
            }
        })
        .collect()
}

/// Public re-export for tests that need word events.
pub fn _words_for(lines: &[TimedLine], duration_s: f64) -> Vec<TimedWord> {
    lines_to_timed_words(lines, duration_s)
}
