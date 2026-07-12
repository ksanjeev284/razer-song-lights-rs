//! Local audio playback with pause, mute, speed, seek + position (rodio 0.21).

use rodio::source::Source;
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio file not found: {0}")]
    NotFound(String),
    #[error("audio device error: {0}")]
    Device(String),
    #[error("decode error: {0}")]
    Decode(String),
}

struct State {
    path: Option<PathBuf>,
    base_s: f64,
    started: Option<Instant>,
    volume: f32,
    speed: f32,
    muted: bool,
    vol_before_mute: f32,
    playing: bool,
    paused: bool,
}

/// Play / pause / stop / seek a single music file; expose song position in seconds.
pub struct AudioPlayer {
    stream: OutputStream,
    sink: Arc<Mutex<Option<Sink>>>,
    state: Arc<Mutex<State>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, AudioError> {
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(Self {
            stream,
            sink: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(State {
                path: None,
                base_s: 0.0,
                started: None,
                volume: 0.85,
                speed: 1.0,
                muted: false,
                vol_before_mute: 0.85,
                playing: false,
                paused: false,
            })),
        })
    }

    pub fn play(&self, path: &Path, volume: f32, start_s: f64) -> Result<(), AudioError> {
        if !path.is_file() {
            return Err(AudioError::NotFound(path.display().to_string()));
        }
        self.stop();
        let vol = volume.clamp(0.0, 1.0);
        self.start_from(path, vol, start_s.max(0.0), 1.0)?;
        Ok(())
    }

    fn start_from(
        &self,
        path: &Path,
        volume: f32,
        start_s: f64,
        speed: f32,
    ) -> Result<(), AudioError> {
        let file = File::open(path).map_err(|e| AudioError::NotFound(e.to_string()))?;
        let reader = BufReader::new(file);
        let decoder = Decoder::new(reader).map_err(|e| AudioError::Decode(e.to_string()))?;
        let source = decoder
            .skip_duration(Duration::from_secs_f64(start_s.max(0.0)))
            .speed(speed.clamp(0.5, 1.5));

        let sink = Sink::connect_new(self.stream.mixer());
        sink.set_volume(volume);
        sink.append(source);
        sink.play();

        {
            let mut s = self.sink.lock().unwrap();
            *s = Some(sink);
        }
        {
            let mut st = self.state.lock().unwrap();
            st.path = Some(path.to_path_buf());
            st.base_s = start_s.max(0.0);
            st.started = Some(Instant::now());
            st.volume = volume;
            st.speed = speed.clamp(0.5, 1.5);
            st.playing = true;
            st.paused = false;
            if st.muted {
                if let Some(sink) = self.sink.lock().unwrap().as_ref() {
                    sink.set_volume(0.0);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        let mut st = self.state.lock().unwrap();
        st.playing = false;
        st.paused = false;
        st.started = None;
        st.base_s = 0.0;
        st.path = None;
    }

    /// Freeze playback; position stays at current song time.
    pub fn pause(&self) {
        let mut st = self.state.lock().unwrap();
        if !st.playing || st.paused {
            return;
        }
        // Capture position before clearing started
        let elapsed = st
            .started
            .map(|t| t.elapsed().as_secs_f64() * st.speed as f64)
            .unwrap_or(0.0);
        st.base_s += elapsed;
        st.started = None;
        st.paused = true;
        drop(st);
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.pause();
        }
    }

    pub fn resume(&self) {
        let mut st = self.state.lock().unwrap();
        if !st.playing || !st.paused {
            return;
        }
        st.started = Some(Instant::now());
        st.paused = false;
        drop(st);
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.play();
        }
    }

    pub fn toggle_pause(&self) {
        let paused = self.state.lock().unwrap().paused;
        if paused {
            self.resume();
        } else {
            self.pause();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    pub fn set_volume(&self, volume: f32) {
        let vol = volume.clamp(0.0, 1.0);
        let mut st = self.state.lock().unwrap();
        st.volume = vol;
        if !st.muted {
            st.vol_before_mute = vol;
        }
        let muted = st.muted;
        drop(st);
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.set_volume(if muted { 0.0 } else { vol });
        }
    }

    pub fn toggle_mute(&self) {
        let mut st = self.state.lock().unwrap();
        if st.muted {
            st.muted = false;
            let v = st.vol_before_mute;
            st.volume = v;
            drop(st);
            if let Some(sink) = self.sink.lock().unwrap().as_ref() {
                sink.set_volume(v);
            }
        } else {
            st.muted = true;
            st.vol_before_mute = st.volume;
            drop(st);
            if let Some(sink) = self.sink.lock().unwrap().as_ref() {
                sink.set_volume(0.0);
            }
        }
    }

    pub fn is_muted(&self) -> bool {
        self.state.lock().unwrap().muted
    }

    /// Playback rate 0.5–1.5 (restarts from current position).
    pub fn set_speed(&self, speed: f32) -> Result<(), AudioError> {
        let speed = speed.clamp(0.5, 1.5);
        let (path, vol, pos) = {
            let st = self.state.lock().unwrap();
            let pos = self.position_unlocked(&st);
            (
                st.path
                    .clone()
                    .ok_or_else(|| AudioError::NotFound("no track loaded".into()))?,
                st.volume,
                pos,
            )
        };
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        self.start_from(&path, vol, pos, speed)?;
        Ok(())
    }

    pub fn speed(&self) -> f32 {
        self.state.lock().unwrap().speed
    }

    pub fn is_playing(&self) -> bool {
        let st = self.state.lock().unwrap();
        if !st.playing || st.paused {
            return false;
        }
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            !sink.empty()
        } else {
            false
        }
    }

    /// True if a track is loaded (playing or paused).
    pub fn is_active(&self) -> bool {
        let st = self.state.lock().unwrap();
        st.playing && st.path.is_some()
    }

    pub fn get_position_s(&self) -> f64 {
        let st = self.state.lock().unwrap();
        self.position_unlocked(&st)
    }

    fn position_unlocked(&self, st: &State) -> f64 {
        if !st.playing {
            return st.base_s;
        }
        if st.paused || st.started.is_none() {
            return st.base_s;
        }
        let elapsed = st
            .started
            .map(|t| t.elapsed().as_secs_f64() * st.speed as f64)
            .unwrap_or(0.0);
        st.base_s + elapsed
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().path.clone()
    }

    pub fn seek(&self, position_s: f64) -> Result<f64, AudioError> {
        let (path, vol, speed, was_paused) = {
            let st = self.state.lock().unwrap();
            (
                st.path
                    .clone()
                    .ok_or_else(|| AudioError::NotFound("no track loaded".into()))?,
                st.volume,
                st.speed,
                st.paused,
            )
        };
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        let pos = position_s.max(0.0);
        self.start_from(&path, vol, pos, speed)?;
        if was_paused {
            self.pause();
        }
        Ok(pos)
    }

    pub fn seek_relative(&self, delta_s: f64) -> Result<f64, AudioError> {
        let cur = self.get_position_s();
        self.seek((cur + delta_s).max(0.0))
    }

    /// Smooth volume fade then stop (blocking, ~duration_ms).
    pub fn fade_out_and_stop(&self, duration_ms: u64) {
        let steps = 20u64;
        let step = duration_ms / steps.max(1);
        let start_vol = {
            let st = self.state.lock().unwrap();
            if st.muted {
                0.0
            } else {
                st.volume
            }
        };
        for i in 0..steps {
            let f = 1.0 - (i as f32 + 1.0) / steps as f32;
            if let Some(sink) = self.sink.lock().unwrap().as_ref() {
                sink.set_volume(start_vol * f);
            }
            std::thread::sleep(Duration::from_millis(step.max(1)));
        }
        self.stop();
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file() {
        if let Ok(ap) = AudioPlayer::new() {
            assert!(ap.play(Path::new("no_such_file.mp3"), 0.5, 0.0).is_err());
        }
    }
}
