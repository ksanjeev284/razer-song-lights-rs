//! Local audio playback with pause, mute, speed, seek + position (rodio 0.21).
//!
//! `OutputStream` is not `Send` on macOS (CoreAudio). All rodio state lives on a
//! dedicated audio thread; the public `AudioPlayer` is only a command channel +
//! shared clocks so it can move into the show engine safely on every OS.

use rodio::source::Source;
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
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

enum Cmd {
    Play {
        path: PathBuf,
        volume: f32,
        start_s: f64,
        reply: Sender<Result<(), String>>,
    },
    Stop,
    Pause,
    Resume,
    SetVolume(f32),
    ToggleMute,
    SetSpeed {
        speed: f32,
        reply: Sender<Result<(), String>>,
    },
    Seek {
        position_s: f64,
        reply: Sender<Result<f64, String>>,
    },
    FadeOut {
        duration_ms: u64,
        done: Sender<()>,
    },
    Shutdown,
}

struct Shared {
    /// Position base (seconds) when started is unset / paused.
    base_bits: AtomicU64,
    /// Instant::now() as nanos when playback started; 0 = not running.
    started_ns: AtomicU64,
    speed_bits: AtomicU32,
    volume_bits: AtomicU32,
    muted: AtomicBool,
    playing: AtomicBool,
    paused: AtomicBool,
    /// Epoch for Instant nanos (process start).
    epoch: Instant,
}

impl Shared {
    fn new() -> Self {
        Self {
            base_bits: AtomicU64::new(0f64.to_bits()),
            started_ns: AtomicU64::new(0),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
            volume_bits: AtomicU32::new(0.85f32.to_bits()),
            muted: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            epoch: Instant::now(),
        }
    }

    fn base(&self) -> f64 {
        f64::from_bits(self.base_bits.load(Ordering::Relaxed))
    }

    fn set_base(&self, v: f64) {
        self.base_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    fn speed(&self) -> f32 {
        f32::from_bits(self.speed_bits.load(Ordering::Relaxed))
    }

    fn set_speed(&self, v: f32) {
        self.speed_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    fn set_started_now(&self) {
        let ns = self.epoch.elapsed().as_nanos() as u64;
        self.started_ns.store(ns.max(1), Ordering::Relaxed);
    }

    fn clear_started(&self) {
        self.started_ns.store(0, Ordering::Relaxed);
    }

    fn position_s(&self) -> f64 {
        let base = self.base();
        if !self.playing.load(Ordering::Relaxed) || self.paused.load(Ordering::Relaxed) {
            return base;
        }
        let start = self.started_ns.load(Ordering::Relaxed);
        if start == 0 {
            return base;
        }
        let now = self.epoch.elapsed().as_nanos() as u64;
        let elapsed = now.saturating_sub(start) as f64 / 1e9 * self.speed() as f64;
        base + elapsed
    }

    fn capture_position_to_base(&self) {
        let pos = self.position_s();
        self.set_base(pos);
        self.clear_started();
    }
}

/// Play / pause / stop / seek a single music file; expose song position in seconds.
/// Thread-safe and `Send` on all platforms (including macOS).
pub struct AudioPlayer {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    /// Path of current track (updated from worker via shared string is heavy; store local + cmd).
    path: Arc<std::sync::Mutex<Option<PathBuf>>>,
    join: Option<JoinHandle<()>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, AudioError> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let shared = Arc::new(Shared::new());
        let path_slot = Arc::new(std::sync::Mutex::new(None));
        let shared_t = shared.clone();
        let path_t = path_slot.clone();

        let join = thread::Builder::new()
            .name("audio-worker".into())
            .spawn(move || {
                audio_worker(rx, shared_t, path_t);
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;

        Ok(Self {
            tx,
            shared,
            path: path_slot,
            join: Some(join),
        })
    }

    pub fn play(&self, path: &Path, volume: f32, start_s: f64) -> Result<(), AudioError> {
        if !path.is_file() {
            return Err(AudioError::NotFound(path.display().to_string()));
        }
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Cmd::Play {
                path: path.to_path_buf(),
                volume: volume.clamp(0.0, 1.0),
                start_s: start_s.max(0.0),
                reply: rtx,
            })
            .map_err(|_| AudioError::Device("audio thread dead".into()))?;
        rrx.recv()
            .map_err(|_| AudioError::Device("audio thread dead".into()))?
            .map_err(AudioError::Decode)
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Resume);
    }

    pub fn toggle_pause(&self) {
        if self.is_paused() {
            self.resume();
        } else {
            self.pause();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, volume: f32) {
        let _ = self.tx.send(Cmd::SetVolume(volume.clamp(0.0, 1.0)));
    }

    pub fn toggle_mute(&self) {
        let _ = self.tx.send(Cmd::ToggleMute);
    }

    pub fn is_muted(&self) -> bool {
        self.shared.muted.load(Ordering::Relaxed)
    }

    pub fn set_speed(&self, speed: f32) -> Result<(), AudioError> {
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Cmd::SetSpeed {
                speed: speed.clamp(0.5, 1.5),
                reply: rtx,
            })
            .map_err(|_| AudioError::Device("audio thread dead".into()))?;
        rrx.recv()
            .map_err(|_| AudioError::Device("audio thread dead".into()))?
            .map_err(AudioError::Decode)
    }

    pub fn speed(&self) -> f32 {
        self.shared.speed()
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed) && !self.is_paused()
    }

    pub fn is_active(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed) && self.path.lock().unwrap().is_some()
    }

    pub fn get_position_s(&self) -> f64 {
        self.shared.position_s()
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.path.lock().unwrap().clone()
    }

    pub fn seek(&self, position_s: f64) -> Result<f64, AudioError> {
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Cmd::Seek {
                position_s: position_s.max(0.0),
                reply: rtx,
            })
            .map_err(|_| AudioError::Device("audio thread dead".into()))?;
        rrx.recv()
            .map_err(|_| AudioError::Device("audio thread dead".into()))?
            .map_err(AudioError::Decode)
    }

    pub fn seek_relative(&self, delta_s: f64) -> Result<f64, AudioError> {
        let cur = self.get_position_s();
        self.seek((cur + delta_s).max(0.0))
    }

    /// Smooth volume fade then stop (blocking, ~duration_ms).
    pub fn fade_out_and_stop(&self, duration_ms: u64) {
        let (done_tx, done_rx) = mpsc::channel();
        if self
            .tx
            .send(Cmd::FadeOut {
                duration_ms,
                done: done_tx,
            })
            .is_ok()
        {
            let _ = done_rx.recv_timeout(Duration::from_millis(duration_ms + 500));
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn audio_worker(
    rx: mpsc::Receiver<Cmd>,
    shared: Arc<Shared>,
    path_slot: Arc<std::sync::Mutex<Option<PathBuf>>>,
) {
    let stream = match rodio::OutputStreamBuilder::open_default_stream() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("audio worker: open stream failed: {e}");
            // Drain until shutdown so senders don't hang forever
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Cmd::Play { reply, .. } => {
                        let _ = reply.send(Err(format!("no audio device: {e}")));
                    }
                    Cmd::SetSpeed { reply, .. } => {
                        let _ = reply.send(Err(format!("no audio device: {e}")));
                    }
                    Cmd::Seek { reply, .. } => {
                        let _ = reply.send(Err(format!("no audio device: {e}")));
                    }
                    Cmd::FadeOut { done, .. } => {
                        let _ = done.send(());
                    }
                    Cmd::Shutdown => break,
                    _ => {}
                }
            }
            return;
        }
    };

    let mut sink: Option<Sink> = None;
    let mut path: Option<PathBuf> = None;
    let mut vol_before_mute = 0.85f32;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Shutdown => {
                if let Some(s) = sink.take() {
                    s.stop();
                }
                break;
            }
            Cmd::Play {
                path: p,
                volume,
                start_s,
                reply,
            } => {
                if let Some(s) = sink.take() {
                    s.stop();
                }
                let res = start_sink(&stream, &p, volume, start_s, 1.0);
                match res {
                    Ok(s) => {
                        path = Some(p.clone());
                        *path_slot.lock().unwrap() = Some(p);
                        shared.set_volume(volume);
                        shared.set_speed(1.0);
                        shared.set_base(start_s);
                        shared.set_started_now();
                        shared.playing.store(true, Ordering::Relaxed);
                        shared.paused.store(false, Ordering::Relaxed);
                        if shared.muted.load(Ordering::Relaxed) {
                            s.set_volume(0.0);
                        }
                        sink = Some(s);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        shared.playing.store(false, Ordering::Relaxed);
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Cmd::Stop => {
                if let Some(s) = sink.take() {
                    s.stop();
                }
                path = None;
                *path_slot.lock().unwrap() = None;
                shared.playing.store(false, Ordering::Relaxed);
                shared.paused.store(false, Ordering::Relaxed);
                shared.clear_started();
                shared.set_base(0.0);
            }
            Cmd::Pause => {
                if shared.playing.load(Ordering::Relaxed) && !shared.paused.load(Ordering::Relaxed)
                {
                    shared.capture_position_to_base();
                    shared.paused.store(true, Ordering::Relaxed);
                    if let Some(s) = sink.as_ref() {
                        s.pause();
                    }
                }
            }
            Cmd::Resume => {
                if shared.playing.load(Ordering::Relaxed) && shared.paused.load(Ordering::Relaxed) {
                    shared.set_started_now();
                    shared.paused.store(false, Ordering::Relaxed);
                    if let Some(s) = sink.as_ref() {
                        s.play();
                    }
                }
            }
            Cmd::SetVolume(vol) => {
                shared.set_volume(vol);
                if !shared.muted.load(Ordering::Relaxed) {
                    vol_before_mute = vol;
                    if let Some(s) = sink.as_ref() {
                        s.set_volume(vol);
                    }
                } else {
                    vol_before_mute = vol;
                }
            }
            Cmd::ToggleMute => {
                if shared.muted.load(Ordering::Relaxed) {
                    shared.muted.store(false, Ordering::Relaxed);
                    shared.set_volume(vol_before_mute);
                    if let Some(s) = sink.as_ref() {
                        s.set_volume(vol_before_mute);
                    }
                } else {
                    shared.muted.store(true, Ordering::Relaxed);
                    vol_before_mute = shared.volume();
                    if let Some(s) = sink.as_ref() {
                        s.set_volume(0.0);
                    }
                }
            }
            Cmd::SetSpeed { speed, reply } => {
                let Some(p) = path.clone() else {
                    let _ = reply.send(Err("no track loaded".into()));
                    continue;
                };
                let pos = shared.position_s();
                let vol = shared.volume();
                let was_paused = shared.paused.load(Ordering::Relaxed);
                if let Some(s) = sink.take() {
                    s.stop();
                }
                match start_sink(&stream, &p, vol, pos, speed) {
                    Ok(s) => {
                        if shared.muted.load(Ordering::Relaxed) {
                            s.set_volume(0.0);
                        }
                        if was_paused {
                            s.pause();
                            shared.set_base(pos);
                            shared.clear_started();
                            shared.paused.store(true, Ordering::Relaxed);
                        } else {
                            shared.set_base(pos);
                            shared.set_started_now();
                            shared.paused.store(false, Ordering::Relaxed);
                        }
                        shared.set_speed(speed);
                        shared.playing.store(true, Ordering::Relaxed);
                        sink = Some(s);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Cmd::Seek { position_s, reply } => {
                let Some(p) = path.clone() else {
                    let _ = reply.send(Err("no track loaded".into()));
                    continue;
                };
                let vol = shared.volume();
                let speed = shared.speed();
                let was_paused = shared.paused.load(Ordering::Relaxed);
                if let Some(s) = sink.take() {
                    s.stop();
                }
                match start_sink(&stream, &p, vol, position_s, speed) {
                    Ok(s) => {
                        if shared.muted.load(Ordering::Relaxed) {
                            s.set_volume(0.0);
                        }
                        if was_paused {
                            s.pause();
                            shared.set_base(position_s);
                            shared.clear_started();
                            shared.paused.store(true, Ordering::Relaxed);
                        } else {
                            shared.set_base(position_s);
                            shared.set_started_now();
                            shared.paused.store(false, Ordering::Relaxed);
                        }
                        shared.playing.store(true, Ordering::Relaxed);
                        sink = Some(s);
                        let _ = reply.send(Ok(position_s));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Cmd::FadeOut { duration_ms, done } => {
                let steps = 20u64;
                let step = duration_ms / steps.max(1);
                let start_vol = if shared.muted.load(Ordering::Relaxed) {
                    0.0
                } else {
                    shared.volume()
                };
                for i in 0..steps {
                    let f = 1.0 - (i as f32 + 1.0) / steps as f32;
                    if let Some(s) = sink.as_ref() {
                        s.set_volume(start_vol * f);
                    }
                    thread::sleep(Duration::from_millis(step.max(1)));
                }
                if let Some(s) = sink.take() {
                    s.stop();
                }
                path = None;
                *path_slot.lock().unwrap() = None;
                shared.playing.store(false, Ordering::Relaxed);
                shared.paused.store(false, Ordering::Relaxed);
                shared.clear_started();
                shared.set_base(0.0);
                let _ = done.send(());
            }
        }
    }
}

fn start_sink(
    stream: &OutputStream,
    path: &Path,
    volume: f32,
    start_s: f64,
    speed: f32,
) -> Result<Sink, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let decoder = Decoder::new(reader).map_err(|e| e.to_string())?;
    let source = decoder
        .skip_duration(Duration::from_secs_f64(start_s.max(0.0)))
        .speed(speed.clamp(0.5, 1.5));
    let sink = Sink::connect_new(stream.mixer());
    sink.set_volume(volume.clamp(0.0, 1.0));
    sink.append(source);
    sink.play();
    thread::sleep(Duration::from_millis(20));
    Ok(sink)
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

    #[test]
    fn player_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AudioPlayer>();
    }
}
