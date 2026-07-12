//! Local audio playback with seek + position (rodio 0.21).

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
    playing: bool,
}

/// Play / stop / seek a single music file; expose song position in seconds.
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
                playing: false,
            })),
        })
    }

    pub fn play(&self, path: &Path, volume: f32, start_s: f64) -> Result<(), AudioError> {
        if !path.is_file() {
            return Err(AudioError::NotFound(path.display().to_string()));
        }
        self.stop();
        let vol = volume.clamp(0.0, 1.0);
        self.start_from(path, vol, start_s.max(0.0))?;
        Ok(())
    }

    fn start_from(&self, path: &Path, volume: f32, start_s: f64) -> Result<(), AudioError> {
        let file = File::open(path).map_err(|e| AudioError::NotFound(e.to_string()))?;
        let reader = BufReader::new(file);
        let decoder = Decoder::new(reader).map_err(|e| AudioError::Decode(e.to_string()))?;
        let source = decoder.skip_duration(Duration::from_secs_f64(start_s.max(0.0)));

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
            st.playing = true;
        }
        std::thread::sleep(Duration::from_millis(40));
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        let mut st = self.state.lock().unwrap();
        st.playing = false;
        st.started = None;
        st.base_s = 0.0;
        st.path = None;
    }

    pub fn set_volume(&self, volume: f32) {
        let vol = volume.clamp(0.0, 1.0);
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.set_volume(vol);
        }
        self.state.lock().unwrap().volume = vol;
    }

    pub fn is_playing(&self) -> bool {
        let st = self.state.lock().unwrap();
        if !st.playing {
            return false;
        }
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            !sink.empty()
        } else {
            false
        }
    }

    pub fn get_position_s(&self) -> f64 {
        let st = self.state.lock().unwrap();
        if !st.playing {
            return st.base_s;
        }
        let elapsed = st
            .started
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        st.base_s + elapsed
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().path.clone()
    }

    pub fn seek(&self, position_s: f64) -> Result<f64, AudioError> {
        let (path, vol) = {
            let st = self.state.lock().unwrap();
            (
                st.path
                    .clone()
                    .ok_or_else(|| AudioError::NotFound("no track loaded".into()))?,
                st.volume,
            )
        };
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        let pos = position_s.max(0.0);
        self.start_from(&path, vol, pos)?;
        Ok(pos)
    }

    pub fn seek_relative(&self, delta_s: f64) -> Result<f64, AudioError> {
        let cur = self.get_position_s();
        self.seek((cur + delta_s).max(0.0))
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
        // Device may be unavailable on headless CI — only assert path error if player opens
        match AudioPlayer::new() {
            Ok(ap) => {
                assert!(ap.play(Path::new("no_such_file.mp3"), 0.5, 0.0).is_err());
            }
            Err(_) => {
                // no audio device — still OK for unit CI
            }
        }
    }
}
