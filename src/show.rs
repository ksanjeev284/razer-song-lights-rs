//! Coordinate audio + LRC karaoke lights (seek-safe) + optional console display.

use crate::audio::AudioPlayer;
use crate::chroma::ChromaKeyboard;
use crate::lrc::{lines_to_timed_words, parse_lrc_lines, TimedLine, TimedWord};
use crate::sync_sim::{line_index_for_time, word_index_for_time};
use crate::themes::{themed_color, LightTheme, RepeatMode};
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
    pub repeat: RepeatMode,
    pub theme: LightTheme,
    pub brightness: f32,
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
            repeat: RepeatMode::Off,
            theme: LightTheme::Rainbow,
            brightness: 1.0,
        }
    }
}

/// Handle to a background light/audio show.
pub struct ShowHandle {
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    audio: Option<Arc<AudioPlayer>>,
    lines: Arc<Vec<TimedLine>>,
    duration_s: f64,
    offset_s: Arc<std::sync::atomic::AtomicU64>, // f64 bits
    theme: Arc<std::sync::atomic::AtomicU8>,
    brightness: Arc<std::sync::atomic::AtomicU32>, // f32 bits 0..1
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
            && !self.ended.load(Ordering::SeqCst)
            && self
                .audio
                .as_ref()
                .map(|a| a.is_active())
                .unwrap_or(self.join.as_ref().map(|j| !j.is_finished()).unwrap_or(false))
    }

    /// Natural end of track (for auto-next).
    pub fn has_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
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
            Ok(a.seek(pos.max(0.0).min(self.duration_s.max(0.0)))?)
        } else {
            Ok(pos.max(0.0))
        }
    }

    pub fn seek_relative(&self, delta: f64) -> anyhow::Result<f64> {
        if let Some(a) = &self.audio {
            let cur = a.get_position_s();
            let max = if self.duration_s > 0.0 {
                self.duration_s
            } else {
                f64::MAX
            };
            Ok(a.seek((cur + delta).clamp(0.0, max))?)
        } else {
            Ok(0.0)
        }
    }

    pub fn set_volume(&self, v: f32) {
        if let Some(a) = &self.audio {
            a.set_volume(v);
        }
    }

    pub fn pause(&self) {
        if let Some(a) = &self.audio {
            a.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(a) = &self.audio {
            a.resume();
        }
    }

    pub fn toggle_pause(&self) {
        if let Some(a) = &self.audio {
            a.toggle_pause();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.audio.as_ref().map(|a| a.is_paused()).unwrap_or(false)
    }

    pub fn toggle_mute(&self) {
        if let Some(a) = &self.audio {
            a.toggle_mute();
        }
    }

    pub fn is_muted(&self) -> bool {
        self.audio.as_ref().map(|a| a.is_muted()).unwrap_or(false)
    }

    pub fn set_speed(&self, speed: f32) -> anyhow::Result<()> {
        if let Some(a) = &self.audio {
            a.set_speed(speed)?;
        }
        Ok(())
    }

    pub fn set_offset(&self, offset: f64) {
        self.offset_s
            .store(offset.to_bits(), Ordering::Relaxed);
    }

    pub fn set_theme(&self, theme: LightTheme) {
        self.theme.store(theme.index(), Ordering::Relaxed);
    }

    pub fn set_brightness(&self, b: f32) {
        self.brightness
            .store(b.clamp(0.05, 1.0).to_bits(), Ordering::Relaxed);
    }

    fn offset(&self) -> f64 {
        f64::from_bits(self.offset_s.load(Ordering::Relaxed))
    }

    pub fn lines(&self) -> Arc<Vec<TimedLine>> {
        self.lines.clone()
    }

    pub fn active_line_index(&self) -> isize {
        line_index_for_time(&self.lines, self.position_s(), self.offset())
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
    let word_starts: Vec<f64> = words.iter().map(|w| w.t).collect();
    let lines_arc = Arc::new(lines);
    let offset_s = Arc::new(std::sync::atomic::AtomicU64::new(
        cfg.sync_offset_s.to_bits(),
    ));
    let theme = Arc::new(std::sync::atomic::AtomicU8::new(cfg.theme.index()));
    let brightness = Arc::new(std::sync::atomic::AtomicU32::new(
        cfg.brightness.clamp(0.05, 1.0).to_bits(),
    ));
    let ended = Arc::new(AtomicBool::new(false));

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
    let starts_thread = word_starts;
    let cfg = cfg.clone();
    let stop_thread = stop.clone();
    let offset_thread = offset_s.clone();
    let theme_thread = theme.clone();
    let bright_thread = brightness.clone();
    let ended_thread = ended.clone();
    let duration = duration_s;

    let join = thread::spawn(move || {
        run_engine(
            &lines_thread,
            &words_thread,
            &starts_thread,
            duration,
            audio_thread.as_ref().map(|a| a.as_ref()),
            &cfg,
            stop_thread,
            offset_thread,
            theme_thread,
            bright_thread,
            ended_thread,
        );
    });

    Ok(ShowHandle {
        stop,
        ended,
        join: Some(join),
        audio,
        lines: lines_arc,
        duration_s,
        offset_s,
        theme,
        brightness,
    })
}

fn run_engine(
    lines: &[TimedLine],
    words: &[TimedWord],
    word_starts: &[f64],
    duration_s: f64,
    audio: Option<&AudioPlayer>,
    cfg: &ShowConfig,
    stop: Arc<AtomicBool>,
    offset_s: Arc<std::sync::atomic::AtomicU64>,
    theme: Arc<std::sync::atomic::AtomicU8>,
    brightness: Arc<std::sync::atomic::AtomicU32>,
    ended: Arc<AtomicBool>,
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
    let mut idle_spins = 0u32;

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
            // Paused: keep loop alive, skip light spam
            if a.is_paused() {
                thread::sleep(Duration::from_millis(40));
                continue;
            }
            if !a.is_playing() && a.get_position_s() > 0.5 {
                let repeat_one = cfg.loop_play || cfg.repeat == RepeatMode::One;
                if repeat_one {
                    let _ = a.seek(0.0);
                    last_word = -1;
                    last_line = -1;
                    continue;
                }
                ended.store(true, Ordering::SeqCst);
                break;
            }
            a.get_position_s()
        } else {
            wall0.elapsed().as_secs_f64()
        };

        if duration_s > 0.0 && pos >= duration_s - 0.1 {
            let repeat_one = cfg.loop_play || cfg.repeat == RepeatMode::One;
            if repeat_one {
                if let Some(a) = audio {
                    let _ = a.seek(0.0);
                }
                last_word = -1;
                last_line = -1;
                continue;
            }
            ended.store(true, Ordering::SeqCst);
            break;
        }

        let offset = f64::from_bits(offset_s.load(Ordering::Relaxed));
        let th = LightTheme::from_index(theme.load(Ordering::Relaxed));
        let bright = f32::from_bits(brightness.load(Ordering::Relaxed)).clamp(0.05, 1.0);
        let mut did_work = false;

        // Karaoke console
        if cfg.karaoke_console && !lines.is_empty() {
            let idx = line_index_for_time(lines, pos, offset);
            if idx != last_printed {
                last_printed = idx;
                did_work = true;
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
                        did_work = true;
                        let i = idx as usize;
                        let color = themed_color(th, i, bright);
                        let _ = kb.flash_text_hit(&lines[i].text, color);
                    }
                }
                PlayMode::FlashWord => {
                    if let Some(i) = word_index_for_time(word_starts, pos, offset) {
                        if i as isize != last_word {
                            last_word = i as isize;
                            did_work = true;
                            let color = themed_color(th, i, bright);
                            let _ = kb.flash_text_hit(&words[i].word, color);
                        }
                    }
                }
            }
        }

        // Adaptive sleep: longer when nothing changed (lower CPU)
        if did_work {
            idle_spins = 0;
            thread::sleep(Duration::from_millis(12));
        } else {
            idle_spins = idle_spins.saturating_add(1);
            let ms = if idle_spins > 20 { 35 } else { 18 };
            thread::sleep(Duration::from_millis(ms));
        }
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

/// Last word whose start time has been reached (seek-safe, O(log n)).
pub fn active_word_index(words: &[TimedWord], pos: f64, offset: f64) -> Option<usize> {
    let starts: Vec<f64> = words.iter().map(|w| w.t).collect();
    word_index_for_time(&starts, pos, offset)
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
