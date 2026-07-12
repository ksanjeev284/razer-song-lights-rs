//! Coordinate audio + LRC karaoke lights (seek-safe) + optional console display.

use crate::audio::AudioPlayer;
use crate::chroma::{color_for_index, ChromaKeyboard};
use crate::lrc::{lines_to_timed_words, parse_lrc_lines, TimedLine, TimedWord};
use crate::sync_sim::line_index_for_time;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    FlashWord,
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
    pub loop_play: bool,
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
            loop_play: false,
        }
    }
}

/// Handle to a background light/audio show.
pub struct ShowHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    audio: Option<Arc<AudioPlayer>>,
    lines: Arc<Vec<TimedLine>>,
    duration_s: f64,
    offset_s: Arc<std::sync::Mutex<f64>>,
}

impl ShowHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(a) = &self.audio {
            a.stop();
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    pub fn is_running(&self) -> bool {
        !self.stop.load(Ordering::SeqCst)
            && self
                .audio
                .as_ref()
                .map(|a| a.is_playing())
                .unwrap_or(self.join.as_ref().map(|j| !j.is_finished()).unwrap_or(false))
    }

    pub fn position_s(&self) -> f64 {
        self.audio
            .as_ref()
            .map(|a| a.get_position_s())
            .unwrap_or(0.0)
    }

    pub fn duration_s(&self) -> f64 {
        self.duration_s
    }

    pub fn seek(&self, pos: f64) -> anyhow::Result<f64> {
        if let Some(a) = &self.audio {
            Ok(a.seek(pos.max(0.0))?)
        } else {
            Ok(pos.max(0.0))
        }
    }

    pub fn seek_relative(&self, delta: f64) -> anyhow::Result<f64> {
        if let Some(a) = &self.audio {
            Ok(a.seek_relative(delta)?)
        } else {
            Ok(0.0)
        }
    }

    pub fn set_volume(&self, v: f32) {
        if let Some(a) = &self.audio {
            a.set_volume(v);
        }
    }

    pub fn set_offset(&self, offset: f64) {
        *self.offset_s.lock().unwrap() = offset;
    }

    pub fn lines(&self) -> Arc<Vec<TimedLine>> {
        self.lines.clone()
    }

    pub fn active_line_index(&self) -> isize {
        let pos = self.position_s();
        let off = *self.offset_s.lock().unwrap();
        line_index_for_time(&self.lines, pos, off)
    }
}

impl Drop for ShowHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Blocking CLI show — runs until finished or `stop` is set.
pub fn run_show(
    lyrics_plain: &str,
    synced_lrc: Option<&str>,
    duration_s: f64,
    audio_path: Option<&Path>,
    cfg: &ShowConfig,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let mut handle = start_show_with_stop(
        lyrics_plain,
        synced_lrc,
        duration_s,
        audio_path,
        cfg,
        stop,
    )?;
    if let Some(j) = handle.join.take() {
        let _ = j.join();
    }
    Ok(())
}

/// Start background show; returns handle for GUI control.
pub fn start_show(
    lyrics_plain: &str,
    synced_lrc: Option<&str>,
    duration_s: f64,
    audio_path: Option<&Path>,
    cfg: &ShowConfig,
) -> anyhow::Result<ShowHandle> {
    start_show_with_stop(
        lyrics_plain,
        synced_lrc,
        duration_s,
        audio_path,
        cfg,
        Arc::new(AtomicBool::new(false)),
    )
}

fn start_show_with_stop(
    lyrics_plain: &str,
    synced_lrc: Option<&str>,
    duration_s: f64,
    audio_path: Option<&Path>,
    cfg: &ShowConfig,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<ShowHandle> {
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
    let lines_arc = Arc::new(lines);
    let offset_s = Arc::new(std::sync::Mutex::new(cfg.sync_offset_s));

    let audio = if cfg.play_audio {
        if let Some(path) = audio_path {
            let ap = Arc::new(AudioPlayer::new()?);
            ap.play(path, cfg.volume, 0.0)?;
            Some(ap)
        } else {
            None
        }
    } else {
        None
    };

    let audio_thread = audio.clone();
    let lines_thread = lines_arc.clone();
    let words_thread = words;
    let cfg = cfg.clone();
    let stop_thread = stop.clone();
    let offset_thread = offset_s.clone();
    let duration = duration_s;

    let join = thread::spawn(move || {
        run_engine(
            &lines_thread,
            &words_thread,
            duration,
            audio_thread.as_ref().map(|a| a.as_ref()),
            &cfg,
            stop_thread,
            offset_thread,
        );
    });

    Ok(ShowHandle {
        stop,
        join: Some(join),
        audio,
        lines: lines_arc,
        duration_s,
        offset_s,
    })
}

fn run_engine(
    lines: &[TimedLine],
    words: &[TimedWord],
    duration_s: f64,
    audio: Option<&AudioPlayer>,
    cfg: &ShowConfig,
    stop: Arc<AtomicBool>,
    offset_s: Arc<std::sync::Mutex<f64>>,
) {
    let mut chroma = if cfg.lights && crate::chroma::is_chroma_supported() {
        match ChromaKeyboard::open() {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("Chroma unavailable ({e}) — continuing without lights");
                None
            }
        }
    } else {
        None
    };

    let wall0 = std::time::Instant::now();
    let mut last_word: isize = -1;
    let mut last_line: isize = -1;
    let mut last_printed: isize = -2;

    if cfg.karaoke_console {
        println!();
        println!("▶  Playing…  Ctrl+C / Stop to end");
        println!(
            "   mode={:?}  lines={}  words={}",
            cfg.mode,
            lines.len(),
            words.len()
        );
        println!();
    }

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        let pos = if let Some(a) = audio {
            if !a.is_playing() && a.get_position_s() > 0.5 {
                if cfg.loop_play {
                    let _ = a.seek(0.0);
                    last_word = -1;
                    last_line = -1;
                    continue;
                }
                break;
            }
            a.get_position_s()
        } else {
            wall0.elapsed().as_secs_f64()
        };

        if duration_s > 0.0 && pos >= duration_s - 0.1 {
            if cfg.loop_play {
                if let Some(a) = audio {
                    let _ = a.seek(0.0);
                }
                last_word = -1;
                last_line = -1;
                continue;
            }
            break;
        }

        let offset = *offset_s.lock().unwrap();

        // Karaoke console
        if cfg.karaoke_console && !lines.is_empty() {
            let idx = line_index_for_time(lines, pos, offset);
            if idx != last_printed {
                last_printed = idx;
                if idx >= 0 {
                    let ln = &lines[idx as usize];
                    print!("\r\x1b[2K");
                    let text = if ln.text.len() > 72 {
                        format!("{}…", &ln.text[..70])
                    } else {
                        ln.text.clone()
                    };
                    print!("♪ [{:5.1}s] {}", pos, text);
                    let _ = io::stdout().flush();
                }
            }
        }

        if let Some(kb) = chroma.as_mut() {
            match cfg.mode {
                PlayMode::FlashLine => {
                    let idx = line_index_for_time(lines, pos, offset);
                    if idx != last_line && idx >= 0 {
                        last_line = idx;
                        let i = idx as usize;
                        let color = color_for_index(i);
                        let _ = kb.flash_text_hit(&lines[i].text, color);
                    }
                }
                PlayMode::FlashWord => {
                    let idx = active_word_index(words, pos, offset);
                    if let Some(i) = idx {
                        if i as isize != last_word {
                            last_word = i as isize;
                            let color = color_for_index(i);
                            let _ = kb.flash_text_hit(&words[i].word, color);
                        }
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(20));
    }

    if cfg.karaoke_console {
        println!();
        println!("■  Stopped");
    }
    if let Some(kb) = chroma.as_mut() {
        kb.clear(0);
        let _ = kb.push();
    }
    if let Some(a) = audio {
        a.stop();
    }
    stop.store(true, Ordering::SeqCst);
}

/// Last word whose start time has been reached (seek-safe).
pub fn active_word_index(words: &[TimedWord], pos: f64, offset: f64) -> Option<usize> {
    if words.is_empty() {
        return None;
    }
    let mut active = None;
    for (i, w) in words.iter().enumerate() {
        if pos + 0.02 >= w.t + offset {
            active = Some(i);
        } else {
            break;
        }
    }
    active
}

pub fn build_timed_lines(
    lyrics_plain: &str,
    synced_lrc: Option<&str>,
    duration_s: f64,
) -> Vec<TimedLine> {
    if let Some(lrc) = synced_lrc {
        let parsed = parse_lrc_lines(lrc);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    estimated_lines(lyrics_plain, duration_s)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc::parse_lrc_lines;

    #[test]
    fn word_index_seek_safe() {
        let lrc = "[00:01.00]one two\n[00:05.00]three four\n";
        let lines = parse_lrc_lines(lrc);
        let words = lines_to_timed_words(&lines, 20.0);
        assert!(active_word_index(&words, 0.0, 0.0).is_none() || active_word_index(&words, 0.0, 0.0) == Some(0));
        let mid = active_word_index(&words, 6.0, 0.0);
        assert!(mid.is_some());
        // seeking back
        let early = active_word_index(&words, 1.0, 0.0);
        assert!(early.unwrap_or(0) <= mid.unwrap_or(0));
    }

    #[test]
    fn build_lines_from_lrc() {
        let lines = build_timed_lines("x", Some("[00:01.00]hello\n[00:02.00]world\n"), 10.0);
        assert_eq!(lines.len(), 2);
    }
}
