//! Full desktop GUI (egui) — optimized with pause, mute, speed, shortcuts, favorites.

use crate::cache::{
    cache_size_bytes, cached_song_count, clear_media_cache, default_cache_root, format_bytes,
    load_song_package, save_song_package, SongPackage,
};
use crate::chroma::{is_chroma_supported, ChromaKeyboard};
use crate::history::{load_history_packages, push_history, PlayQueue};
use crate::lrc::{chorus_time_s, lines_to_timed_words, lrc_quality_score, TimedWord};
use crate::lrclib::fetch_lyrics;
use crate::playlists::{timed_lines_to_lrc, PlaylistStore};
use crate::settings::{AppSettings, Favorites};
use crate::show::{build_timed_lines, start_show, PlayMode, ShowConfig, ShowHandle};
use crate::song_catalog::SONG_CATALOG;
use crate::song_memory::{export_playlist, parse_playlist_urls, SongMemory};
use crate::stats::{Bookmarks, ListenStats};
use crate::sync_sim::{line_index_for_time, word_index_for_time};
use crate::themes::{AmbientEffect, LightTheme, RepeatMode};
use crate::title::is_youtube_url;
use crate::youtube::{
    download_audio_with_auth, extract_meta_with_auth, search_youtube, SearchHit, YtdlpAuth,
    COOKIE_BROWSERS,
};
use crate::VERSION;
use eframe::egui::{self, Color32, Key, Modifiers, RichText, ScrollArea, Sense, Vec2};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const ACCENT: Color32 = Color32::from_rgb(0x44, 0xd6, 0x2c);
const BG: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0d);
const PANEL: Color32 = Color32::from_rgb(0x14, 0x14, 0x18);
const DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xaa);
const PAST: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x58);
const DANGER: Color32 = Color32::from_rgb(0xff, 0x4d, 0x4d);

fn read_clipboard_text() -> Option<String> {
    // Windows-friendly clipboard read without extra deps
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "try { Get-Clipboard -Raw } catch { '' }",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

enum LoadMsg {
    Ok(SongPackage),
    Err(String),
    Status(String),
    SearchOk(Vec<SearchHit>),
}

pub fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 840.0])
            .with_min_inner_size([900.0, 700.0])
            .with_title(format!("Razer Song Lights {VERSION}")),
        ..Default::default()
    };
    eframe::run_native(
        "Razer Song Lights",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals.dark_mode = true;
            style.visuals.panel_fill = PANEL;
            style.visuals.window_fill = BG;
            style.visuals.extreme_bg_color = Color32::from_rgb(0x0f, 0x0f, 0x13);
            style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x1c, 0x1c, 0x22);
            style.visuals.selection.bg_fill = Color32::from_rgb(0x1a, 0x3d, 0x14);
            cc.egui_ctx.set_style(style);
            Ok(Box::new(SongLightsGui::new()))
        }),
    )
}

struct SongLightsGui {
    url: String,
    lyrics_edit: String,
    song_label: String,
    status: String,
    duration_s: f64,
    synced_lrc: Option<String>,
    audio_path: Option<PathBuf>,
    video_id: String,

    play_audio: bool,
    clock_sync: bool,
    cache_music: bool,
    lights: bool,
    loop_play: bool,
    auto_next: bool,
    volume: f32,
    offset: f32,
    speed: f32,
    brightness: f32,
    mode: PlayMode,
    theme: LightTheme,
    repeat: RepeatMode,
    shuffle: bool,
    show_history: bool,
    always_on_top: bool,
    mini_player: bool,
    fullscreen_karaoke: bool,
    recent_urls: Vec<String>,

    show: Option<ShowHandle>,
    /// Deferred drop after fade (keeps ShowHandle on GUI thread — required on macOS).
    fading_show: Option<(Instant, ShowHandle)>,
    queue: PlayQueue,
    favorites: Favorites,
    timed_lines: Vec<crate::lrc::TimedLine>,
    active_line: isize,
    scrub: f32,
    scrubbing: bool,
    lyric_filter: String,

    busy: bool,
    load_rx: Option<Receiver<LoadMsg>>,
    chroma_ok: bool,
    error_popup: Option<String>,
    was_ended: bool,
    sleep_until: Option<Instant>,
    cache_label: String,
    ab_a: Option<f64>,
    ab_b: Option<f64>,
    bookmarks: Bookmarks,
    stats: ListenStats,
    lyric_font: f32,
    listen_tick: Instant,
    prefetched: bool,
    fade_on_stop: bool,
    ambient_pulse: bool,
    resume_position: bool,
    remember_offset: bool,
    crossfade_next: bool,
    night_dim: bool,
    song_memory: SongMemory,
    history_filter: String,
    show_help: bool,
    lrc_quality: f64,
    ambient_effect: AmbientEffect,
    sleep_fade: bool,
    seek_snap: bool,
    playlists: PlaylistStore,
    playlist_name: String,
    ytdlp_cookies_file: String,
    ytdlp_cookies_browser: String,
    search_query: String,
    search_hits: Vec<SearchHit>,
    search_busy: bool,
    auto_night_dim: bool,
    mirror_lights: bool,
    fade_in: bool,
    auto_skip_intro: bool,
    hide_past_lyrics: bool,
    search_history: Vec<String>,
    song_note: String,
    fade_in_until: Option<Instant>,
    now_playing_file: bool,
    word_highlight: bool,
    show_upcoming: bool,
    party_mode: bool,
    preroll_countdown: bool,
    timed_words: Vec<TimedWord>,
    active_word: Option<usize>,
    dual_lyrics: String,
    practice_loops: u32,
    last_np_write: Instant,
    preroll_until: Option<Instant>,
    visualizer_phase: f32,
    end_fade: bool,
    pause_on_unfocus: bool,
    mute_lights_with_audio: bool,
    session_start: Instant,
    was_focused: bool,
    end_fading: bool,
}

impl SongLightsGui {
    fn new() -> Self {
        let settings = AppSettings::load();
        let root = default_cache_root();
        let mut queue = PlayQueue::default();
        for s in load_history_packages(&root, 30) {
            queue.push(s);
        }
        if !queue.songs.is_empty() {
            queue.index = queue.songs.len() as isize - 1;
        }

        let root = default_cache_root();
        let cache_label = format!(
            "Cache: {} · {} songs",
            format_bytes(cache_size_bytes(&root)),
            cached_song_count(&root)
        );

        Self {
            url: String::new(),
            lyrics_edit: "hello world\nlights on stage\nsing with me".into(),
            song_label: "No song loaded — paste a YouTube link".into(),
            status: format!(
                "Ready · Space pause · ←/→ seek · F11 karaoke · {}",
                if is_chroma_supported() {
                    "Chroma OK"
                } else {
                    "Chroma N/A"
                }
            ),
            duration_s: 12.0,
            synced_lrc: None,
            audio_path: None,
            video_id: String::new(),
            play_audio: settings.play_audio,
            clock_sync: true,
            cache_music: settings.cache_music,
            lights: settings.lights,
            loop_play: settings.loop_play,
            auto_next: settings.auto_next,
            volume: settings.volume,
            offset: settings.offset,
            speed: settings.speed.clamp(0.5, 1.5),
            brightness: settings.brightness.clamp(0.05, 1.0),
            mode: settings.play_mode(),
            theme: settings.theme,
            repeat: settings.repeat,
            shuffle: settings.shuffle,
            show_history: settings.show_history_panel,
            always_on_top: settings.always_on_top,
            mini_player: settings.mini_player,
            fullscreen_karaoke: false,
            recent_urls: settings.recent_urls,
            show: None,
            fading_show: None,
            queue,
            favorites: Favorites::load(),
            timed_lines: Vec::new(),
            active_line: -1,
            scrub: 0.0,
            scrubbing: false,
            lyric_filter: String::new(),
            busy: false,
            load_rx: None,
            chroma_ok: is_chroma_supported(),
            error_popup: None,
            was_ended: false,
            sleep_until: None,
            cache_label,
            ab_a: None,
            ab_b: None,
            bookmarks: Bookmarks::load(),
            stats: {
                let mut s = ListenStats::load();
                s.start_session();
                s
            },
            lyric_font: 15.0,
            listen_tick: Instant::now(),
            prefetched: false,
            fade_on_stop: true,
            ambient_pulse: settings.ambient_pulse,
            resume_position: settings.resume_position,
            remember_offset: settings.remember_offset,
            crossfade_next: settings.crossfade_next,
            night_dim: settings.night_dim,
            song_memory: SongMemory::load(),
            history_filter: String::new(),
            show_help: false,
            lrc_quality: -1.0,
            ambient_effect: if settings.ambient_pulse {
                settings.ambient_effect
            } else {
                AmbientEffect::Off
            },
            sleep_fade: settings.sleep_fade,
            seek_snap: settings.seek_snap,
            playlists: PlaylistStore::load(),
            playlist_name: String::new(),
            ytdlp_cookies_file: settings.ytdlp_cookies_file,
            ytdlp_cookies_browser: settings.ytdlp_cookies_browser,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_busy: false,
            auto_night_dim: settings.auto_night_dim,
            mirror_lights: settings.mirror_lights,
            fade_in: settings.fade_in,
            auto_skip_intro: settings.auto_skip_intro,
            hide_past_lyrics: settings.hide_past_lyrics,
            search_history: settings.search_history,
            song_note: String::new(),
            fade_in_until: None,
            now_playing_file: settings.now_playing_file,
            word_highlight: settings.word_highlight,
            show_upcoming: settings.show_upcoming,
            party_mode: settings.party_mode,
            preroll_countdown: settings.preroll_countdown,
            timed_words: Vec::new(),
            active_word: None,
            dual_lyrics: String::new(),
            practice_loops: 0,
            last_np_write: Instant::now(),
            preroll_until: None,
            visualizer_phase: 0.0,
            end_fade: settings.end_fade,
            pause_on_unfocus: settings.pause_on_unfocus,
            mute_lights_with_audio: settings.mute_lights_with_audio,
            session_start: Instant::now(),
            was_focused: true,
            end_fading: false,
        }
    }

    fn ytdlp_auth(&self) -> YtdlpAuth {
        YtdlpAuth::resolve(
            if self.ytdlp_cookies_file.trim().is_empty() {
                None
            } else {
                Some(self.ytdlp_cookies_file.as_str())
            },
            Some(self.ytdlp_cookies_browser.as_str()),
        )
    }

    fn persist_settings(&self) {
        let mut s = AppSettings {
            play_audio: self.play_audio,
            cache_music: self.cache_music,
            lights: self.lights,
            loop_play: self.loop_play,
            auto_next: self.auto_next,
            volume: self.volume,
            offset: self.offset,
            speed: self.speed,
            show_history_panel: self.show_history,
            brightness: self.brightness,
            theme: self.theme,
            repeat: self.repeat,
            shuffle: self.shuffle,
            always_on_top: self.always_on_top,
            mini_player: self.mini_player,
            recent_urls: self.recent_urls.clone(),
            ambient_pulse: self.ambient_pulse && self.ambient_effect.is_on(),
            resume_position: self.resume_position,
            remember_offset: self.remember_offset,
            crossfade_next: self.crossfade_next,
            night_dim: self.night_dim,
            ambient_effect: self.ambient_effect,
            sleep_fade: self.sleep_fade,
            seek_snap: self.seek_snap,
            ytdlp_cookies_file: self.ytdlp_cookies_file.clone(),
            ytdlp_cookies_browser: self.ytdlp_cookies_browser.clone(),
            auto_night_dim: self.auto_night_dim,
            mirror_lights: self.mirror_lights,
            fade_in: self.fade_in,
            auto_skip_intro: self.auto_skip_intro,
            hide_past_lyrics: self.hide_past_lyrics,
            search_history: self.search_history.clone(),
            now_playing_file: self.now_playing_file,
            word_highlight: self.word_highlight,
            show_upcoming: self.show_upcoming,
            party_mode: self.party_mode,
            preroll_countdown: self.preroll_countdown,
            end_fade: self.end_fade,
            pause_on_unfocus: self.pause_on_unfocus,
            mute_lights_with_audio: self.mute_lights_with_audio,
            ..AppSettings::default()
        };
        s.set_play_mode(self.mode);
        s.save();
    }

    fn is_night_hours() -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Approximate local hour via local offset from chrono would be better;
        // use Windows-local via simple hour of day in local timezone via format.
        // Fallback: treat 22–7 as night in local TZ using strftime-like approach.
        #[cfg(windows)]
        {
            use std::process::Command;
            if let Ok(out) = Command::new("powershell")
                .args(["-NoProfile", "-Command", "(Get-Date).Hour"])
                .output()
            {
                if let Ok(s) = String::from_utf8(out.stdout) {
                    if let Ok(h) = s.trim().parse::<u32>() {
                        return !(7..22).contains(&h);
                    }
                }
            }
        }
        let hour = ((secs / 3600) % 24) as u32; // UTC fallback
        !(7..22).contains(&hour)
    }

    fn effective_brightness(&self) -> f32 {
        let night = self.night_dim || (self.auto_night_dim && Self::is_night_hours());
        if night {
            self.brightness * 0.45
        } else {
            self.brightness
        }
    }

    fn apply_live_show_opts(&self) {
        if let Some(s) = &self.show {
            s.set_brightness(self.effective_brightness());
            s.set_ambient(self.ambient_pulse && self.ambient_effect.is_on());
            s.set_ambient_effect(if self.ambient_pulse {
                self.ambient_effect
            } else {
                AmbientEffect::Off
            });
            s.set_mirror_lights(self.mirror_lights);
        }
    }

    fn refresh_cache_label(&mut self) {
        let root = default_cache_root();
        self.cache_label = format!(
            "Cache: {} · {} songs",
            format_bytes(cache_size_bytes(&root)),
            cached_song_count(&root)
        );
    }

    fn stop_show(&mut self) {
        // Remember resume position
        if self.resume_position && !self.video_id.is_empty() {
            if let Some(s) = &self.show {
                let t = s.position_s();
                if t > 5.0 && (self.duration_s <= 0.0 || t < self.duration_s - 8.0) {
                    self.song_memory.set_resume(&self.video_id, t);
                }
            }
        }
        if self.remember_offset && !self.video_id.is_empty() {
            self.song_memory.set_offset(&self.video_id, self.offset);
        }
        if let Some(mut s) = self.show.take() {
            if self.fade_on_stop && s.is_running() {
                s.fade_stop();
                // Replace prior fading player (at most one deferred drop)
                if let Some((_, old)) = self.fading_show.take() {
                    drop(old);
                }
                self.fading_show = Some((Instant::now() + Duration::from_millis(1400), s));
            } else {
                s.stop();
            }
        }
        self.was_ended = false;
        self.prefetched = false;
        self.status = "Stopped".into();
    }

    fn poll_fading_show(&mut self) {
        if let Some((until, s)) = self.fading_show.take() {
            if Instant::now() >= until {
                drop(s);
            } else {
                self.fading_show = Some((until, s));
            }
        }
    }

    fn start_current(&mut self) {
        self.stop_show();
        let plain = self.lyrics_edit.clone();
        if plain.trim().is_empty() {
            self.error_popup = Some("Type lyrics or load a YouTube song first.".into());
            return;
        }
        if is_youtube_url(plain.trim()) && !plain.contains('\n') {
            self.url = plain.trim().to_string();
            self.load_youtube();
            return;
        }

        // Per-song remembered offset
        if self.remember_offset {
            if let Some(e) = self.song_memory.get(&self.video_id) {
                self.offset = e.offset;
            }
        }

        self.timed_lines = if self.clock_sync {
            build_timed_lines(&plain, self.synced_lrc.as_deref(), self.duration_s)
        } else {
            build_timed_lines(&plain, None, 0.0)
        };
        self.timed_words = lines_to_timed_words(&self.timed_lines, self.duration_s);
        self.active_word = None;
        self.active_line = -1;
        self.lrc_quality = self
            .synced_lrc
            .as_ref()
            .map(|l| lrc_quality_score(l, self.duration_s))
            .unwrap_or(-1.0);

        let resume = if self.resume_position {
            self.song_memory
                .get(&self.video_id)
                .map(|e| e.resume_s)
                .filter(|t| *t > 5.0)
                .unwrap_or(0.0)
        } else {
            0.0
        };

        let cfg = ShowConfig {
            mode: self.mode,
            volume: self.volume,
            sync_offset_s: self.offset as f64,
            play_audio: self.play_audio && self.audio_path.is_some(),
            lights: self.lights,
            karaoke_console: false,
            loop_play: self.loop_play || self.repeat == RepeatMode::One,
            repeat: self.repeat,
            theme: self.theme,
            brightness: self.effective_brightness(),
            ambient_pulse: self.ambient_pulse && self.ambient_effect.is_on(),
            ambient_effect: if self.ambient_pulse {
                self.ambient_effect
            } else {
                AmbientEffect::Off
            },
            mirror_lights: self.mirror_lights,
        };

        match start_show(
            &plain,
            self.synced_lrc.as_deref(),
            self.duration_s,
            self.audio_path.as_deref(),
            &cfg,
        ) {
            Ok(h) => {
                let _ = h.set_speed(self.speed);
                h.set_theme(self.theme);
                h.set_brightness(self.effective_brightness());
                h.set_ab_loop(self.ab_a, self.ab_b);
                h.set_ambient_effect(cfg.ambient_effect);
                h.set_mirror_lights(self.mirror_lights);
                if resume > 0.0 {
                    let _ = h.seek(resume);
                    self.status =
                        format!("Resumed @ {} — {}", Self::fmt_time(resume), self.song_label);
                    self.song_memory.clear_resume(&self.video_id);
                } else if self.auto_skip_intro {
                    if let Ok(t) = h.skip_intro() {
                        self.status =
                            format!("Skip intro → {} — {}", Self::fmt_time(t), self.song_label);
                    } else {
                        self.status = format!("Playing — {}", self.song_label);
                    }
                } else {
                    self.status = format!("Playing — {}", self.song_label);
                }
                if self.fade_in {
                    h.set_volume(0.0);
                    self.fade_in_until = Some(Instant::now() + Duration::from_millis(900));
                } else {
                    self.fade_in_until = None;
                }
                if self.preroll_countdown && resume <= 0.0 && !self.auto_skip_intro {
                    self.preroll_until = Some(Instant::now() + Duration::from_secs(3));
                    h.pause();
                    self.status = "3… get ready".into();
                } else {
                    self.preroll_until = None;
                }
                self.stats.record_play(&self.song_label);
                if !self.video_id.is_empty() {
                    self.song_memory.bump_plays(&self.video_id);
                }
                self.song_note = self.song_memory.note(&self.video_id);
                self.show = Some(h);
                self.was_ended = false;
                self.prefetched = false;
                self.end_fading = false;
                self.listen_tick = Instant::now();
            }
            Err(e) => self.error_popup = Some(format!("Play failed: {e}")),
        }
        self.persist_settings();
    }

    fn open_local_audio(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["mp3", "m4a", "wav", "ogg", "flac", "opus"])
            .pick_file()
        {
            self.audio_path = Some(path.clone());
            self.synced_lrc = None;
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Local audio".into());
            self.song_label = format!("{name}  ·  local file");
            self.video_id = format!("local:{}", name);
            // Estimate duration unknown; keep existing or default 180s for plain timing
            if self.duration_s < 5.0 {
                self.duration_s = 180.0;
            }
            if self.lyrics_edit.trim().is_empty() {
                self.lyrics_edit = name.clone();
            }
            self.timed_lines = build_timed_lines(&self.lyrics_edit, None, self.duration_s);
            self.status = format!("Loaded local audio: {}", path.display());
            self.song_note = self.song_memory.note(&self.video_id);
        }
    }

    fn play_random_favorite(&mut self) {
        let favs: Vec<_> = self
            .queue
            .songs
            .iter()
            .filter(|s| self.favorites.contains(&s.video_id))
            .cloned()
            .collect();
        if favs.is_empty() {
            self.error_popup = Some("No favorites in queue. Star songs with Ctrl+F.".into());
            return;
        }
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        Instant::now().hash(&mut h);
        let i = (h.finish() as usize) % favs.len();
        self.apply_package(favs[i].clone(), true);
        self.status = "Random favorite".into();
    }

    fn load_catalog_search(&mut self, artist: &str, track: &str) {
        self.search_query = format!("{artist} {track}");
        self.run_search();
    }

    fn chroma_test(&mut self) {
        if !is_chroma_supported() {
            self.error_popup = Some("Chroma not available on this platform.".into());
            return;
        }
        thread::spawn(|| {
            if let Ok(mut kb) = ChromaKeyboard::open() {
                let _ = kb.test_sweep();
            }
        });
        self.status = "Chroma test sweep…".into();
    }

    fn write_now_playing(&self, pos: f64) {
        let root = default_cache_root();
        let _ = std::fs::create_dir_all(&root);
        let line = if self.active_line >= 0 {
            self.timed_lines
                .get(self.active_line as usize)
                .map(|l| l.text.as_str())
                .unwrap_or("")
        } else {
            ""
        };
        let word = self
            .active_word
            .and_then(|i| self.timed_words.get(i))
            .map(|w| w.word.as_str())
            .unwrap_or("");
        let body = format!(
            "{}\n{}\n{} / {}\n{}\n{}\n",
            self.song_label,
            self.url,
            Self::fmt_time(pos),
            Self::fmt_time(self.duration_s),
            line,
            word
        );
        let _ = std::fs::write(root.join("now_playing.txt"), body);
    }

    fn jump_to_chorus(&mut self) {
        let t = chorus_time_s(&self.timed_lines, self.duration_s);
        if let Some(s) = &self.show {
            let _ = s.seek((t + self.offset as f64).max(0.0));
            self.status = format!("Jump chorus ~ {}", Self::fmt_time(t));
        } else {
            self.status = "Start playback to jump to chorus".into();
        }
    }

    fn restart_track(&mut self) {
        if let Some(s) = &self.show {
            let _ = s.seek(0.0);
            self.end_fading = false;
            s.set_volume(self.volume);
            self.status = "Restarted track".into();
        } else {
            self.start_current();
        }
    }

    fn import_lyrics_clipboard(&mut self) {
        if let Some(t) = read_clipboard_text() {
            let t = t.trim().to_string();
            if t.is_empty() {
                return;
            }
            if t.contains('[') {
                self.synced_lrc = Some(t.clone());
                self.lyrics_edit = crate::lyrics_match::clean_lyrics(&t);
            } else {
                self.synced_lrc = None;
                self.lyrics_edit = t;
            }
            self.timed_lines = build_timed_lines(
                &self.lyrics_edit,
                self.synced_lrc.as_deref(),
                self.duration_s,
            );
            self.timed_words = lines_to_timed_words(&self.timed_lines, self.duration_s);
            self.status = "Imported lyrics from clipboard".into();
        }
    }

    fn export_song_package_json(&mut self) {
        let pkg = serde_json::json!({
            "title": self.song_label,
            "url": self.url,
            "video_id": self.video_id,
            "duration_s": self.duration_s,
            "lyrics": self.lyrics_edit,
            "synced_lrc": self.synced_lrc,
            "dual_lyrics": self.dual_lyrics,
            "note": self.song_note,
            "offset": self.offset,
        });
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("song_package.json")
            .save_file()
        {
            if let Ok(data) = serde_json::to_string_pretty(&pkg) {
                if std::fs::write(&path, data).is_ok() {
                    self.status = format!("Exported {}", path.display());
                }
            }
        }
    }

    fn loop_current_line(&mut self) {
        if self.active_line < 0 {
            self.status = "No active line to loop".into();
            return;
        }
        let i = self.active_line as usize;
        if let Some(ln) = self.timed_lines.get(i) {
            let a = ln.t + self.offset as f64;
            let b = if i + 1 < self.timed_lines.len() {
                self.timed_lines[i + 1].t + self.offset as f64
            } else {
                ln.end_t + self.offset as f64
            };
            self.ab_a = Some(a.max(0.0));
            self.ab_b = Some(b.max(a + 0.3));
            if let Some(s) = &self.show {
                s.set_ab_loop(self.ab_a, self.ab_b);
                let _ = s.seek(a.max(0.0));
            }
            self.practice_loops = self.practice_loops.saturating_add(1);
            self.status = format!("Practice loop line {} (×{})", i + 1, self.practice_loops);
        }
    }

    fn export_favorites_m3u(&mut self) {
        let urls: Vec<String> = self
            .queue
            .songs
            .iter()
            .filter(|s| self.favorites.contains(&s.video_id) && !s.url.is_empty())
            .map(|s| s.url.clone())
            .collect();
        if urls.is_empty() {
            self.error_popup = Some("No favorite URLs to export.".into());
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("M3U", &["m3u"])
            .set_file_name("favorites.m3u")
            .save_file()
        {
            if std::fs::write(&path, export_playlist(&urls)).is_ok() {
                self.status = format!("Exported {} favorites", urls.len());
            }
        }
    }

    fn load_youtube(&mut self) {
        if self.busy {
            return;
        }
        let url = self.url.trim().to_string();
        if url.is_empty() {
            self.error_popup = Some("Paste a YouTube URL first.".into());
            return;
        }
        if !is_youtube_url(&url) {
            self.error_popup = Some("That does not look like a YouTube URL.".into());
            return;
        }
        self.busy = true;
        self.status = "Loading YouTube song…".into();
        // Track recent URLs
        self.recent_urls.retain(|u| u != &url);
        self.recent_urls.insert(0, url.clone());
        self.recent_urls.truncate(12);
        self.persist_settings();

        let want_audio = self.play_audio;
        let use_cache = self.cache_music;
        let auth = self.ytdlp_auth();
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        thread::spawn(move || {
            let root = default_cache_root();
            let audio_dir = root.join("audio");
            let send = |m: LoadMsg| {
                let _ = tx.send(m);
            };

            send(LoadMsg::Status(format!(
                "Fetching YouTube metadata… ({})",
                auth.describe()
            )));
            let meta = match extract_meta_with_auth(&url, &auth) {
                Ok(m) => m,
                Err(e) => {
                    send(LoadMsg::Err(e.to_string()));
                    return;
                }
            };
            send(LoadMsg::Status(format!(
                "Resolved: {} — {}",
                meta.artist, meta.track
            )));

            if use_cache {
                if let Some(mut pkg) = load_song_package(&root, &meta.video_id) {
                    send(LoadMsg::Status(format!(
                        "Cache hit: {}",
                        pkg.display_name()
                    )));
                    if want_audio
                        && pkg
                            .audio_path
                            .as_ref()
                            .map(|p| !PathBuf::from(p).is_file())
                            .unwrap_or(true)
                    {
                        send(LoadMsg::Status("Downloading audio…".into()));
                        if let Ok(p) =
                            download_audio_with_auth(&meta.url, &meta.video_id, &audio_dir, &auth)
                        {
                            pkg.audio_path = Some(p.display().to_string());
                        }
                    }
                    push_history(&root, &meta.video_id, 40);
                    send(LoadMsg::Ok(pkg));
                    return;
                }
            }

            send(LoadMsg::Status(format!(
                "Lyrics: {} — {}…",
                meta.artist, meta.track
            )));
            let hit = match fetch_lyrics(&meta.artist, &meta.track, meta.duration_s) {
                Ok(h) => h,
                Err(e) => {
                    send(LoadMsg::Err(e.to_string()));
                    return;
                }
            };
            send(LoadMsg::Status(format!(
                "Lyrics OK ({})",
                if hit.is_synced() {
                    "synced LRC"
                } else {
                    "plain"
                }
            )));

            let mut audio_path = None;
            if want_audio {
                send(LoadMsg::Status(format!(
                    "Downloading audio… ({})",
                    auth.describe()
                )));
                match download_audio_with_auth(&meta.url, &meta.video_id, &audio_dir, &auth) {
                    Ok(p) => audio_path = Some(p.display().to_string()),
                    Err(e) => send(LoadMsg::Status(format!("Audio skip: {e}"))),
                }
            }

            let mut pkg = SongPackage::from_hit(
                &meta.artist,
                &meta.track,
                meta.duration_s,
                &hit,
                &meta.video_id,
                &meta.url,
            );
            pkg.audio_path = audio_path;
            if use_cache {
                let _ = save_song_package(&root, &pkg);
                push_history(&root, &meta.video_id, 40);
            }
            send(LoadMsg::Ok(pkg));
        });
    }

    fn apply_package(&mut self, pkg: SongPackage, autoplay: bool) {
        self.song_label = format!(
            "{}   ·   {:.0}s   ·   {}   ·   {}",
            pkg.display_name(),
            pkg.duration_s,
            if pkg.audio_path.is_some() {
                "audio"
            } else {
                "lights only"
            },
            if pkg.is_synced() { "SYNCED" } else { "est." }
        );
        self.lyrics_edit = pkg.lyrics.clone();
        self.duration_s = pkg.duration_s;
        self.synced_lrc = pkg.synced_lrc.clone();
        self.audio_path = pkg.audio_path.as_ref().map(PathBuf::from);
        self.video_id = pkg.video_id.clone();
        if !pkg.url.is_empty() {
            self.url = pkg.url.clone();
        }
        self.timed_lines =
            build_timed_lines(&pkg.lyrics, pkg.synced_lrc.as_deref(), pkg.duration_s);
        self.timed_words = lines_to_timed_words(&self.timed_lines, pkg.duration_s);
        self.active_word = None;
        self.queue.push(pkg);
        self.status = format!("Loaded {}", self.song_label);
        if autoplay {
            self.start_current();
        }
    }

    fn poll_load(&mut self) {
        let Some(rx) = &self.load_rx else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(LoadMsg::Status(s)) => self.status = s,
                Ok(LoadMsg::Ok(pkg)) => {
                    self.busy = false;
                    self.search_busy = false;
                    self.load_rx = None;
                    self.apply_package(pkg, true);
                    return;
                }
                Ok(LoadMsg::SearchOk(hits)) => {
                    self.busy = false;
                    self.search_busy = false;
                    self.load_rx = None;
                    let n = hits.len();
                    self.search_hits = hits;
                    self.status = format!("Search: {n} results — click to play");
                    return;
                }
                Ok(LoadMsg::Err(e)) => {
                    self.busy = false;
                    self.search_busy = false;
                    self.load_rx = None;
                    self.status = "Load failed".into();
                    self.error_popup = Some(e);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.search_busy = false;
                    self.load_rx = None;
                    break;
                }
            }
        }
    }

    fn run_search(&mut self) {
        if self.busy || self.search_busy {
            return;
        }
        let q = self.search_query.trim().to_string();
        if q.is_empty() {
            self.error_popup = Some("Type a search query first.".into());
            return;
        }
        self.search_history.retain(|x| x != &q);
        self.search_history.insert(0, q.clone());
        self.search_history.truncate(12);
        self.persist_settings();
        self.search_busy = true;
        self.busy = true;
        self.status = format!("Searching YouTube: {q}…");
        let auth = self.ytdlp_auth();
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        thread::spawn(move || match search_youtube(&q, 8, &auth) {
            Ok(hits) => {
                let _ = tx.send(LoadMsg::SearchOk(hits));
            }
            Err(e) => {
                let _ = tx.send(LoadMsg::Err(e.to_string()));
            }
        });
    }

    fn fmt_time(s: f64) -> String {
        let s = s.max(0.0) as i64;
        format!("{}:{:02}", s / 60, s % 60)
    }

    fn seek_rel(&mut self, delta: f64) {
        if let Some(show) = &self.show {
            if let Ok(p) = show.seek_relative(delta) {
                self.status = format!("Seek → {}", Self::fmt_time(p));
                if self.duration_s > 0.0 {
                    self.scrub = (p / self.duration_s) as f32;
                }
            }
        }
    }

    fn prev_song(&mut self) {
        if self.queue.index <= 0 {
            if let Some(show) = &self.show {
                let _ = show.seek(0.0);
                self.status = "Restarted track".into();
            }
            return;
        }
        self.queue.index -= 1;
        if let Some(pkg) = self.queue.current().cloned() {
            let name = pkg.display_name();
            self.apply_package(pkg, true);
            self.status = format!("Previous: {name}");
        }
    }

    fn next_song(&mut self) {
        // Save resume/offset for current before switching
        if self.resume_position || self.remember_offset {
            if let Some(s) = &self.show {
                if self.resume_position && !self.video_id.is_empty() {
                    self.song_memory.set_resume(&self.video_id, s.position_s());
                }
            }
            if self.remember_offset && !self.video_id.is_empty() {
                self.song_memory.set_offset(&self.video_id, self.offset);
            }
        }
        if self.crossfade_next {
            if let Some(mut s) = self.show.take() {
                s.fade_stop();
                if let Some((_, old)) = self.fading_show.take() {
                    drop(old);
                }
                self.fading_show = Some((Instant::now() + Duration::from_millis(900), s));
            }
        }
        // Repeat all: wrap; shuffle: random; else sequential
        let idx = if self.shuffle {
            self.queue.next_index(true)
        } else if self.queue.can_next() {
            Some(self.queue.index as usize + 1)
        } else if self.repeat == RepeatMode::All && !self.queue.songs.is_empty() {
            Some(0)
        } else {
            None
        };
        let Some(i) = idx else {
            self.status = "No next song".into();
            return;
        };
        self.queue.index = i as isize;
        if let Some(pkg) = self.queue.current().cloned() {
            let name = pkg.display_name();
            self.apply_package(pkg, true);
            self.status = format!("Next: {name}");
        }
    }

    fn import_playlist(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Playlist", &["m3u", "m3u8", "txt"])
            .pick_file()
        {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let urls = parse_playlist_urls(&text);
                let n = urls.len();
                for u in urls.into_iter().rev() {
                    if is_youtube_url(&u) {
                        self.recent_urls.retain(|x| x != &u);
                        self.recent_urls.insert(0, u);
                    }
                }
                self.recent_urls.truncate(40);
                self.persist_settings();
                self.status = format!("Imported {n} playlist entries → Recent");
            }
        }
    }

    fn export_queue_playlist(&mut self) {
        let urls: Vec<String> = self
            .queue
            .songs
            .iter()
            .map(|s| s.url.clone())
            .filter(|u| !u.is_empty())
            .collect();
        if urls.is_empty() {
            self.error_popup = Some("No URLs in queue to export.".into());
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("M3U", &["m3u"])
            .set_file_name("razer_queue.m3u")
            .save_file()
        {
            if std::fs::write(&path, export_playlist(&urls)).is_ok() {
                self.status = format!("Exported {} tracks", urls.len());
            }
        }
    }

    fn queue_urls(&self) -> Vec<String> {
        self.queue
            .songs
            .iter()
            .map(|s| s.url.clone())
            .filter(|u| !u.is_empty())
            .collect()
    }

    fn save_named_playlist(&mut self) {
        let urls = self.queue_urls();
        if urls.is_empty() {
            self.error_popup = Some("Queue has no URLs to save.".into());
            return;
        }
        let name = if self.playlist_name.trim().is_empty() {
            format!("Queue {}", urls.len())
        } else {
            self.playlist_name.trim().to_string()
        };
        self.playlists.save_named(&name, urls);
        self.playlist_name = name.clone();
        self.status = format!("Saved playlist “{name}”");
    }

    fn load_named_playlist(&mut self, name: &str) {
        if let Some(pl) = self.playlists.get(name).cloned() {
            let n = pl.urls.len();
            for u in pl.urls.into_iter().rev() {
                if is_youtube_url(&u) {
                    self.recent_urls.retain(|x| x != &u);
                    self.recent_urls.insert(0, u);
                }
            }
            self.recent_urls.truncate(40);
            self.playlist_name = name.to_string();
            self.persist_settings();
            self.status = format!("Loaded playlist “{name}” ({n} URLs → Recent)");
        }
    }

    fn export_timed_as_lrc(&mut self) {
        if self.timed_lines.is_empty() {
            self.error_popup = Some("No timed lyrics to export.".into());
            return;
        }
        let pairs: Vec<(f64, &str)> = self
            .timed_lines
            .iter()
            .map(|l| (l.t, l.text.as_str()))
            .collect();
        let lrc = timed_lines_to_lrc(&pairs);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("LRC", &["lrc"])
            .set_file_name("timed.lrc")
            .save_file()
        {
            if std::fs::write(&path, lrc).is_ok() {
                self.status = format!("Exported timed LRC → {}", path.display());
            }
        }
    }

    fn seek_percent(&mut self, pct: f64) {
        let dur = self.duration_s.max(0.01);
        let t = (pct.clamp(0.0, 1.0) * dur).max(0.0);
        if let Some(s) = &self.show {
            let _ = s.seek(t);
            self.status = format!("Seek → {} ({:.0}%)", Self::fmt_time(t), pct * 100.0);
            self.scrub = pct as f32;
        }
    }

    fn top_played(&self, limit: usize) -> Vec<(String, u32, String)> {
        // (video_id, plays, display name from queue if known)
        let mut items: Vec<_> = self
            .song_memory
            .by_id
            .iter()
            .filter(|(_, e)| e.plays > 0)
            .map(|(id, e)| {
                let name = self
                    .queue
                    .songs
                    .iter()
                    .find(|s| s.video_id == *id)
                    .map(|s| s.display_name())
                    .unwrap_or_else(|| id.clone());
                (id.clone(), e.plays, name)
            })
            .collect();
        items.sort_by_key(|b| std::cmp::Reverse(b.1));
        items.truncate(limit);
        items
    }

    fn export_lrc(&mut self) {
        let Some(lrc) = &self.synced_lrc else {
            self.error_popup = Some("No synced LRC to export.".into());
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("LRC", &["lrc"])
            .set_file_name("lyrics.lrc")
            .save_file()
        {
            match std::fs::write(&path, lrc) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(e) => self.error_popup = Some(e.to_string()),
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut space = false;
        let mut left = false;
        let mut right = false;
        let mut open = false;
        let mut mute = false;
        let mut fav = false;
        let mut next = false;
        let mut prev = false;
        let mut intro = false;
        let mut mark = false;
        let mut snap = false;
        let mut replay_line = false;
        let mut offset_minus = false;
        let mut offset_plus = false;
        let mut pct_key: Option<u8> = None;
        let mut seek_long = false;
        ctx.input(|i| {
            if i.key_pressed(Key::Space) {
                space = true;
            }
            seek_long = i.modifiers.shift;
            if i.key_pressed(Key::ArrowLeft) {
                left = true;
            }
            if i.key_pressed(Key::ArrowRight) {
                right = true;
            }
            if i.key_pressed(Key::R) && !i.modifiers.matches_logically(Modifiers::CTRL) {
                replay_line = true;
            }
            // L handled below via separate flag
        });
        let mut loop_line = false;
        ctx.input(|i| {
            if i.key_pressed(Key::L) && !i.modifiers.matches_logically(Modifiers::CTRL) {
                loop_line = true;
            }
            if i.modifiers.matches_logically(Modifiers::CTRL) && i.key_pressed(Key::O) {
                open = true;
            }
            if i.key_pressed(Key::M) {
                mute = true;
            }
            if i.key_pressed(Key::F) && i.modifiers.matches_logically(Modifiers::CTRL) {
                fav = true;
            }
            if i.key_pressed(Key::N) {
                next = true;
            }
            if i.key_pressed(Key::P) {
                prev = true;
            }
            if i.key_pressed(Key::I) {
                intro = true;
            }
            if i.key_pressed(Key::B) && i.modifiers.matches_logically(Modifiers::CTRL) {
                mark = true;
            }
            if i.key_pressed(Key::S) && !i.modifiers.matches_logically(Modifiers::CTRL) {
                snap = true;
            }
            if i.key_pressed(Key::OpenBracket) {
                offset_minus = true;
            }
            if i.key_pressed(Key::CloseBracket) {
                offset_plus = true;
            }
            for (k, n) in [
                (Key::Num1, 1u8),
                (Key::Num2, 2),
                (Key::Num3, 3),
                (Key::Num4, 4),
                (Key::Num5, 5),
                (Key::Num6, 6),
                (Key::Num7, 7),
                (Key::Num8, 8),
                (Key::Num9, 9),
            ] {
                if i.key_pressed(k) {
                    pct_key = Some(n);
                }
            }
        });
        let f11 = ctx.input(|i| i.key_pressed(Key::F11));
        let f1 = ctx.input(|i| i.key_pressed(Key::F1));
        if f1 {
            self.show_help = !self.show_help;
        }
        // Drag-drop files / URLs
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| {
                    f.path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| f.name.clone())
                })
                .collect()
        });
        for d in dropped {
            let d = d.trim().to_string();
            if is_youtube_url(&d) {
                self.url = d;
                self.load_youtube();
            } else {
                let path = PathBuf::from(&d);
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if ext == "lrc" || ext == "txt" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if ext == "lrc" || content.contains('[') {
                            self.synced_lrc = Some(content.clone());
                            self.lyrics_edit = crate::lyrics_match::clean_lyrics(&content);
                        } else {
                            self.synced_lrc = None;
                            self.lyrics_edit = content;
                        }
                        self.song_label = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        self.timed_lines = build_timed_lines(
                            &self.lyrics_edit,
                            self.synced_lrc.as_deref(),
                            self.duration_s,
                        );
                        self.status = format!("Dropped {}", self.song_label);
                    }
                }
            }
        }

        let focus_text = ctx.memory(|m| m.focused().is_some());
        if space && !focus_text {
            if self.show.is_some() {
                if let Some(s) = &self.show {
                    s.toggle_pause();
                    self.status = if s.is_paused() {
                        "Paused".into()
                    } else {
                        "Resumed".into()
                    };
                }
            } else {
                self.start_current();
            }
        }
        if left && !focus_text {
            self.seek_rel(if seek_long { -30.0 } else { -10.0 });
        }
        if right && !focus_text {
            self.seek_rel(if seek_long { 30.0 } else { 10.0 });
        }
        if replay_line && !focus_text {
            if let Some(s) = &self.show {
                if let Ok(t) = s.replay_line() {
                    self.status = format!("Replay line @ {}", Self::fmt_time(t));
                }
            }
        }
        if loop_line && !focus_text {
            self.loop_current_line();
        }
        if mute && !focus_text {
            if let Some(s) = &self.show {
                s.toggle_mute();
                self.status = if s.is_muted() {
                    "Muted".into()
                } else {
                    "Unmuted".into()
                };
            }
        }
        if next && !focus_text {
            self.next_song();
        }
        if prev && !focus_text {
            self.prev_song();
        }
        if intro && !focus_text {
            if let Some(s) = &self.show {
                if let Ok(t) = s.skip_intro() {
                    self.status = format!("Skipped intro → {}", Self::fmt_time(t));
                }
            }
        }
        if mark && !focus_text {
            if let Some(s) = &self.show {
                let t = s.position_s();
                self.bookmarks
                    .add(&self.video_id, &self.song_label, t, "bookmark");
                self.status = format!("Bookmark @ {}", Self::fmt_time(t));
            }
        }
        if snap && !focus_text {
            if let Some(s) = &self.show {
                match s.snap_to_lyric() {
                    Ok(t) => self.status = format!("Snap → lyric @ {}", Self::fmt_time(t)),
                    Err(_) => self.status = "No lyric to snap".into(),
                }
            }
        }
        if offset_minus && !focus_text {
            self.offset = (self.offset - 0.1).clamp(-10.0, 20.0);
            self.status = format!("Offset {:.1}s", self.offset);
        }
        if offset_plus && !focus_text {
            self.offset = (self.offset + 0.1).clamp(-10.0, 20.0);
            self.status = format!("Offset {:.1}s", self.offset);
        }
        if let Some(n) = pct_key {
            if !focus_text && self.show.is_some() {
                self.seek_percent(n as f64 / 10.0);
            }
        }
        if fav && !self.video_id.is_empty() {
            let on = self.favorites.toggle(&self.video_id);
            self.status = if on {
                "Added to favorites ★".into()
            } else {
                "Removed from favorites".into()
            };
        }
        if open {
            self.open_lyrics_file();
        }
        if f11 {
            self.fullscreen_karaoke = !self.fullscreen_karaoke;
        }
    }

    fn open_lyrics_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Lyrics", &["txt", "lrc"])
            .pick_file()
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if path.extension().map(|e| e == "lrc").unwrap_or(false) || content.contains('[') {
                    self.synced_lrc = Some(content.clone());
                    self.lyrics_edit = crate::lyrics_match::clean_lyrics(&content);
                } else {
                    self.synced_lrc = None;
                    self.lyrics_edit = content;
                }
                self.audio_path = None;
                self.song_label = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.timed_lines = build_timed_lines(
                    &self.lyrics_edit,
                    self.synced_lrc.as_deref(),
                    self.duration_s,
                );
                self.status = format!("Loaded {}", self.song_label);
            }
        }
    }
}

impl eframe::App for SongLightsGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_load();
        self.poll_fading_show();
        self.handle_shortcuts(ctx);
        if self.fading_show.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Pause when window loses focus (optional)
        let focused = ctx.input(|i| i.focused);
        if self.pause_on_unfocus {
            if self.was_focused && !focused {
                if let Some(s) = &self.show {
                    if s.is_running() && !s.is_paused() {
                        s.pause();
                        self.status = "Paused (window unfocused)".into();
                    }
                }
            }
            self.was_focused = focused;
        }

        // End-of-track volume fade (last ~5s)
        if self.end_fade {
            if let Some(show) = &self.show {
                if show.is_running() && !show.is_paused() && self.duration_s > 15.0 {
                    let left = self.duration_s - show.position_s();
                    if left < 5.0 && left > 0.0 {
                        self.end_fading = true;
                        let f = (left / 5.0).clamp(0.0, 1.0) as f32;
                        show.set_volume(self.volume * f);
                    } else if self.end_fading && left >= 5.0 {
                        self.end_fading = false;
                        show.set_volume(self.volume);
                    }
                }
            }
        }

        // Dim lights when muted
        if self.mute_lights_with_audio {
            if let Some(show) = &self.show {
                if show.is_muted() {
                    show.set_brightness(self.effective_brightness() * 0.15);
                }
            }
        }

        // Pre-roll 3-2-1 countdown (karaoke practice)
        if let Some(until) = self.preroll_until {
            let left = until
                .saturating_duration_since(Instant::now())
                .as_secs_f32();
            if left <= 0.0 {
                self.preroll_until = None;
                if let Some(s) = &self.show {
                    s.resume();
                }
                self.status = format!("Go! — {}", self.song_label);
            } else {
                let n = left.ceil() as i32;
                self.status = format!("{n}…");
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }

        // Fade-in ramp
        if let Some(until) = self.fade_in_until {
            let now = Instant::now();
            if now >= until {
                if let Some(s) = &self.show {
                    s.set_volume(self.volume);
                }
                self.fade_in_until = None;
            } else {
                let total = 0.9_f32;
                let left = until.saturating_duration_since(now).as_secs_f32();
                let progress = (1.0 - left / total).clamp(0.0, 1.0);
                if let Some(s) = &self.show {
                    s.set_volume(self.volume * progress);
                }
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }

        // Sleep timer (optional fade in last ~1.2s)
        if let Some(until) = self.sleep_until {
            let now = Instant::now();
            if now >= until {
                self.sleep_until = None;
                if self.sleep_fade {
                    self.fade_on_stop = true;
                }
                self.stop_show();
                self.status = "Sleep timer — stopped".into();
            } else {
                if self.sleep_fade {
                    let left = until.saturating_duration_since(now).as_secs_f32();
                    if left < 1.4 {
                        if let Some(s) = &self.show {
                            s.set_volume(self.volume * (left / 1.4).clamp(0.0, 1.0));
                        }
                    }
                }
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        // Auto-next / repeat-all when track ends
        if let Some(show) = &self.show {
            if show.has_ended() && !self.was_ended {
                self.was_ended = true;
                let can_advance = self.auto_next || self.repeat == RepeatMode::All || self.shuffle;
                if can_advance
                    && (self.queue.can_next() || self.repeat == RepeatMode::All || self.shuffle)
                {
                    self.status = "Track ended — next…".into();
                    self.next_song();
                } else {
                    self.status = "Track ended".into();
                }
            }
        }

        if let Some(show) = &self.show {
            show.set_offset(self.offset as f64);
            // Don't fight sleep-fade or start fade-in volume ramps
            let sleep_fading = self
                .sleep_until
                .map(|u| u.saturating_duration_since(Instant::now()).as_secs_f32() < 1.4)
                .unwrap_or(false)
                && self.sleep_fade;
            let fading_in = self.fade_in_until.is_some();
            if !sleep_fading && !fading_in && !self.end_fading {
                show.set_volume(self.volume);
            }
            show.set_theme(self.theme);
            show.set_brightness(self.effective_brightness());
            show.set_ab_loop(self.ab_a, self.ab_b);
            show.set_ambient_effect(if self.ambient_pulse {
                self.ambient_effect
            } else {
                AmbientEffect::Off
            });
            show.set_mirror_lights(self.mirror_lights);
            let pos = show.position_s();
            let dur = self.duration_s.max(show.duration_s());
            if !self.scrubbing && dur > 0.0 {
                self.scrub = (pos / dur).clamp(0.0, 1.0) as f32;
            }
            self.active_line = line_index_for_time(&self.timed_lines, pos, self.offset as f64);
            if !self.timed_words.is_empty() {
                let starts: Vec<f64> = self.timed_words.iter().map(|w| w.t).collect();
                self.active_word = word_index_for_time(&starts, pos, self.offset as f64);
            } else {
                self.active_word = None;
            }
            self.visualizer_phase = (pos as f32 * 2.5) % 64.0;
            // Now-playing file for OBS (~1 Hz)
            if self.now_playing_file && self.last_np_write.elapsed() > Duration::from_millis(800) {
                self.write_now_playing(pos);
                self.last_np_write = Instant::now();
            }
            // Accrue listen stats every ~5s while playing
            if show.is_running() && !show.is_paused() {
                let elapsed = self.listen_tick.elapsed().as_secs_f64();
                if elapsed >= 5.0 {
                    self.stats.add_listen_secs(elapsed);
                    self.listen_tick = Instant::now();
                }
                // Prefetch next audio when near end
                if !self.prefetched && dur > 0.0 && pos > dur - 35.0 {
                    self.prefetched = true;
                    if let Some(idx) = self.queue.next_index(self.shuffle) {
                        if let Some(pkg) = self.queue.songs.get(idx).cloned() {
                            if let (Some(url), vid) = (
                                if pkg.url.is_empty() {
                                    None
                                } else {
                                    Some(pkg.url.clone())
                                },
                                pkg.video_id.clone(),
                            ) {
                                if !vid.is_empty() {
                                    let auth = self.ytdlp_auth();
                                    thread::spawn(move || {
                                        let root = default_cache_root();
                                        let _ = download_audio_with_auth(
                                            &url,
                                            &vid,
                                            &root.join("audio"),
                                            &auth,
                                        );
                                    });
                                }
                            }
                        }
                    }
                }
            }
            if show.is_running() || show.is_paused() {
                ctx.request_repaint_after(std::time::Duration::from_millis(40));
            }
        }

        // Viewport: always on top / mini
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));

        if let Some(err) = self.error_popup.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button("OK").clicked() {
                        self.error_popup = None;
                    }
                });
        }

        if self.show_help {
            egui::Window::new("Keyboard shortcuts (F1)")
                .collapsible(true)
                .resizable(true)
                .default_pos([80.0, 80.0])
                .show(ctx, |ui| {
                    ui.monospace(
                        "Space     Pause / Resume / Play\n\
                         ← / →     Seek −10s / +10s\n\
                         N / P     Next / Previous\n\
                         I         Skip intro\n\
                         M         Mute\n\
                         S         Snap to nearest lyric\n\
                         R         Replay current lyric line\n\
                         Shift+←/→ Seek ±30s\n\
                         Local     Open audio… for mp3/m4a without YouTube\n\
                         1–9       Jump to 10%–90%\n\
                         [ / ]     Offset −0.1s / +0.1s\n\
                         Ctrl+B    Bookmark position\n\
                         Ctrl+F    Favorite song\n\
                         Ctrl+O    Open lyrics file\n\
                         F11       Fullscreen karaoke\n\
                         F1        This help\n\
                         Drop      YouTube URL or .lrc/.txt onto window\n\
                         Click lyric line to seek",
                    );
                    if ui.button("Close").clicked() {
                        self.show_help = false;
                    }
                });
        }

        egui::TopBottomPanel::bottom("status")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                    ui.painter().circle_filled(
                        rect.center(),
                        4.5,
                        if self.chroma_ok { ACCENT } else { DANGER },
                    );
                    ui.label(RichText::new(&self.status).color(DIM).size(12.5));
                    if let Some(show) = &self.show {
                        let left = (self.duration_s - show.position_s()).max(0.0);
                        if self.duration_s > 0.0 && (show.is_running() || show.is_paused()) {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("−{}", Self::fmt_time(left)))
                                            .color(DIM)
                                            .small()
                                            .monospace(),
                                    );
                                },
                            );
                        }
                    }
                });
            });

        // History / favorites side panel
        if self.show_history {
            egui::SidePanel::right("history")
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Library").color(ACCENT).size(14.0));
                    ui.horizontal(|ui| {
                        if ui.small_button("Import M3U").clicked() {
                            self.import_playlist();
                        }
                        if ui.small_button("Export M3U").clicked() {
                            self.export_queue_playlist();
                        }
                        if ui
                            .small_button("Clear Q")
                            .on_hover_text("Clear queue")
                            .clicked()
                        {
                            self.queue.songs.clear();
                            self.queue.index = -1;
                            self.status = "Queue cleared".into();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("Shuffle Q")
                            .on_hover_text("Shuffle queue order")
                            .clicked()
                        {
                            self.queue.shuffle_in_place();
                            self.status = "Queue shuffled".into();
                        }
                        if ui
                            .small_button("↑ Top")
                            .on_hover_text("Move current to top of queue")
                            .clicked()
                        {
                            self.queue.move_current_to_top();
                            self.status = "Moved current to top".into();
                        }
                        if ui.small_button("▲").on_hover_text("Move up").clicked() {
                            self.queue.move_current_up();
                        }
                        if ui.small_button("▼").on_hover_text("Move down").clicked() {
                            self.queue.move_current_down();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("Dedupe")
                            .on_hover_text("Remove duplicates")
                            .clicked()
                        {
                            self.queue.dedupe();
                            self.status = "Queue deduped".into();
                        }
                        if ui
                            .small_button("A–Z")
                            .on_hover_text("Sort by name")
                            .clicked()
                        {
                            self.queue.sort_by_name();
                            self.status = "Queue sorted A–Z".into();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("🎲 Fav")
                            .on_hover_text("Play random favorite")
                            .clicked()
                        {
                            self.play_random_favorite();
                        }
                        if ui
                            .small_button("Export ★")
                            .on_hover_text("Export favorites M3U")
                            .clicked()
                        {
                            self.export_favorites_m3u();
                        }
                    });
                    ui.label(RichText::new("Demo catalog").color(DIM).small().strong());
                    egui::ComboBox::from_id_salt("catalog_demo")
                        .selected_text("Search catalog song…")
                        .show_ui(ui, |ui| {
                            for s in SONG_CATALOG {
                                if ui
                                    .selectable_label(false, format!("{} — {}", s.artist, s.track))
                                    .clicked()
                                {
                                    self.load_catalog_search(s.artist, s.track);
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.playlist_name)
                                .hint_text("Playlist name…")
                                .desired_width(110.0),
                        );
                        if ui
                            .small_button("Save")
                            .on_hover_text("Save queue as named playlist")
                            .clicked()
                        {
                            self.save_named_playlist();
                        }
                    });
                    if !self.playlists.lists.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Load").color(DIM).small());
                            egui::ComboBox::from_id_salt("named_pl")
                                .selected_text("Pick…")
                                .show_ui(ui, |ui| {
                                    let names = self.playlists.names();
                                    for name in names {
                                        if ui.selectable_label(false, &name).clicked() {
                                            self.load_named_playlist(&name);
                                        }
                                    }
                                });
                            if ui
                                .small_button("Del")
                                .on_hover_text("Delete named playlist")
                                .clicked()
                                && !self.playlist_name.is_empty()
                            {
                                let n = self.playlist_name.clone();
                                self.playlists.remove(&n);
                                self.status = format!("Deleted playlist “{n}”");
                            }
                        });
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.history_filter)
                            .hint_text("Filter history…")
                            .desired_width(200.0),
                    );
                    ui.separator();
                    ui.label(RichText::new("History").color(DIM).small().strong());
                    let mut play_idx: Option<usize> = None;
                    let mut remove_idx: Option<usize> = None;
                    let filt = self.history_filter.to_lowercase();
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (i, s) in self.queue.songs.iter().enumerate().rev() {
                            if !filt.is_empty() && !s.display_name().to_lowercase().contains(&filt)
                            {
                                continue;
                            }
                            let selected = i as isize == self.queue.index;
                            let star = if self.favorites.contains(&s.video_id) {
                                "★ "
                            } else {
                                ""
                            };
                            let plays = self
                                .song_memory
                                .get(&s.video_id)
                                .map(|e| e.plays)
                                .unwrap_or(0);
                            let label = if plays > 0 {
                                format!("{star}{} ({plays})", s.display_name())
                            } else {
                                format!("{star}{}", s.display_name())
                            };
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(selected, RichText::new(label).size(12.0))
                                    .clicked()
                                {
                                    play_idx = Some(i);
                                }
                                if ui.small_button("×").on_hover_text("Remove").clicked() {
                                    remove_idx = Some(i);
                                }
                            });
                        }
                    });
                    if let Some(i) = remove_idx {
                        if i < self.queue.songs.len() {
                            self.queue.songs.remove(i);
                            if self.queue.index as usize >= self.queue.songs.len() {
                                self.queue.index = self.queue.songs.len() as isize - 1;
                            }
                        }
                    }
                    if let Some(i) = play_idx {
                        self.queue.index = i as isize;
                        if let Some(pkg) = self.queue.current().cloned() {
                            self.apply_package(pkg, true);
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Favorites").color(DIM).small().strong());
                    ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        let favs: Vec<_> = self
                            .queue
                            .songs
                            .iter()
                            .filter(|s| self.favorites.contains(&s.video_id))
                            .cloned()
                            .collect();
                        if favs.is_empty() {
                            ui.label(RichText::new("Ctrl+F to star current").color(DIM).small());
                        }
                        for s in favs {
                            if ui.button(s.display_name()).clicked() {
                                self.apply_package(s, true);
                            }
                        }
                    });
                    ui.separator();
                    ui.label(RichText::new("Top played").color(DIM).small().strong());
                    ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        let top = self.top_played(8);
                        if top.is_empty() {
                            ui.label(RichText::new("Play songs to rank them").color(DIM).small());
                        }
                        for (vid, plays, name) in top {
                            if ui.button(format!("{plays}× {name}")).clicked() {
                                if let Some(pkg) =
                                    self.queue.songs.iter().find(|s| s.video_id == vid).cloned()
                                {
                                    self.apply_package(pkg, true);
                                }
                            }
                        }
                    });
                });
        }

        // Fullscreen karaoke / party overlay
        if self.fullscreen_karaoke || self.party_mode {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.18);
                    ui.label(RichText::new(&self.song_label).color(DIM).size(14.0));
                    if let Some(until) = self.preroll_until {
                        let n = until
                            .saturating_duration_since(Instant::now())
                            .as_secs_f32()
                            .ceil() as i32;
                        ui.add_space(30.0);
                        ui.label(
                            RichText::new(if n > 0 { format!("{n}") } else { "GO".into() })
                                .color(ACCENT)
                                .size(72.0)
                                .strong(),
                        );
                    } else if self.active_line >= 0 {
                        if let Some(ln) = self.timed_lines.get(self.active_line as usize) {
                            ui.add_space(20.0);
                            if self.word_highlight && self.active_word.is_some() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    for (wi, w) in self.timed_words.iter().enumerate() {
                                        // only words of current line approx by time window
                                        if w.t < ln.t - 0.05 || w.t >= ln.end_t + 0.2 {
                                            continue;
                                        }
                                        let on = self.active_word == Some(wi);
                                        ui.label(
                                            RichText::new(&w.word)
                                                .color(if on { ACCENT } else { DIM })
                                                .size(if on { 40.0 } else { 32.0 })
                                                .strong(),
                                        );
                                    }
                                });
                            } else {
                                ui.label(RichText::new(&ln.text).color(ACCENT).size(40.0).strong());
                            }
                        }
                    } else {
                        ui.label(RichText::new("…").color(DIM).size(28.0));
                    }
                    if self.show_upcoming {
                        for k in 1..=2 {
                            if let Some(next) =
                                self.timed_lines.get((self.active_line + k) as usize)
                            {
                                ui.add_space(12.0);
                                ui.label(RichText::new(&next.text).color(DIM).size(if k == 1 {
                                    22.0
                                } else {
                                    16.0
                                }));
                            }
                        }
                    }
                    if !self.dual_lyrics.trim().is_empty() && self.active_line >= 0 {
                        let dual: Vec<&str> = self.dual_lyrics.lines().collect();
                        if let Some(t) = dual.get(self.active_line as usize) {
                            ui.add_space(18.0);
                            ui.label(
                                RichText::new(*t)
                                    .color(Color32::from_rgb(0xaa, 0xcc, 0xff))
                                    .size(20.0),
                            );
                        }
                    }
                    ui.add_space(40.0);
                    ui.label(
                        RichText::new(if self.party_mode {
                            "Party mode · F11 or uncheck Party to exit"
                        } else {
                            "F11 to exit fullscreen karaoke"
                        })
                        .color(DIM)
                        .small(),
                    );
                });
            });
            if self.party_mode && !self.fullscreen_karaoke {
                // still allow updates but skip normal layout
                return;
            }
            if self.fullscreen_karaoke {
                return;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if !self.mini_player {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("RAZER SONG LIGHTS").color(ACCENT).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.checkbox(&mut self.always_on_top, "On top").changed() {
                            self.persist_settings();
                        }
                        if ui.checkbox(&mut self.mini_player, "Mini").changed() {
                            self.persist_settings();
                        }
                        if ui
                            .checkbox(&mut self.show_history, "Library")
                            .changed()
                        {
                            self.persist_settings();
                        }
                        if ui.checkbox(&mut self.party_mode, "Party").changed() {
                            self.persist_settings();
                        }
                    });
                });
                ui.label(
                    RichText::new(
                        "Space pause · ←/→ seek · N/P next/prev · R replay line · L loop line · F11 karaoke · Party mode",
                    )
                    .color(DIM)
                    .size(11.5),
                );
                ui.add_space(8.0);
            }

            // Search
            egui::Frame::none()
                .fill(PANEL)
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("YouTube").color(ACCENT).strong());
                        let w = ui.available_width() - 200.0;
                        ui.add_sized(
                            [w.max(120.0), 26.0],
                            egui::TextEdit::singleline(&mut self.url)
                                .hint_text("https://youtube.com/watch?v=…"),
                        );
                        if ui
                            .small_button("Paste")
                            .on_hover_text("Paste clipboard URL")
                            .clicked()
                        {
                            if let Some(t) = read_clipboard_text() {
                                let t = t.trim().to_string();
                                if !t.is_empty() {
                                    self.url = t;
                                    self.status = "Pasted URL from clipboard".into();
                                }
                            }
                        }
                        if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(RichText::new("Load + Play").strong())
                                    .fill(ACCENT),
                            )
                            .clicked()
                        {
                            self.load_youtube();
                        }
                    });
                    if !self.mini_player {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Search").color(DIM).small());
                            let w = ui.available_width() - 90.0;
                            let resp = ui.add_sized(
                                [w.max(100.0), 22.0],
                                egui::TextEdit::singleline(&mut self.search_query)
                                    .hint_text("artist or song name…"),
                            );
                            if resp.lost_focus()
                                && ui.input(|i| i.key_pressed(Key::Enter))
                                && !self.search_busy
                            {
                                self.run_search();
                            }
                            if ui
                                .add_enabled(!self.busy, egui::Button::new("Find"))
                                .clicked()
                            {
                                self.run_search();
                            }
                        });
                        if self.search_busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Searching…").color(DIM).small());
                            });
                        }
                        if !self.search_history.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Recent searches").color(DIM).small());
                                egui::ComboBox::from_id_salt("search_hist")
                                    .selected_text("Pick…")
                                    .show_ui(ui, |ui| {
                                        for q in self.search_history.clone() {
                                            if ui.selectable_label(false, &q).clicked() {
                                                self.search_query = q;
                                            }
                                        }
                                    });
                            });
                        }
                        if !self.search_hits.is_empty() {
                            ui.label(RichText::new("Results (click to play)").color(DIM).small());
                            let mut pick: Option<String> = None;
                            ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                                for h in &self.search_hits {
                                    if ui
                                        .selectable_label(false, RichText::new(h.label()).size(12.0))
                                        .clicked()
                                    {
                                        pick = Some(h.url.clone());
                                    }
                                }
                            });
                            if let Some(u) = pick {
                                self.url = u;
                                self.load_youtube();
                            }
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.play_audio, "Audio");
                            ui.checkbox(&mut self.clock_sync, "Sync");
                            ui.checkbox(&mut self.cache_music, "Cache");
                            ui.checkbox(&mut self.lights, "Lights");
                            ui.checkbox(&mut self.auto_next, "Auto-next");
                            ui.checkbox(&mut self.shuffle, "Shuffle");
                            if ui
                                .button(format!("Repeat: {}", self.repeat.label()))
                                .clicked()
                            {
                                self.repeat = self.repeat.cycle();
                                self.loop_play = self.repeat == RepeatMode::One;
                                self.persist_settings();
                            }
                            ui.label(RichText::new("Vol").color(DIM));
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.volume, 0.0..=1.0)
                                        .show_value(false),
                                )
                                .changed()
                            {
                                if let Some(s) = &self.show {
                                    s.set_volume(self.volume);
                                }
                            }
                            ui.label(format!("{:.0}%", self.volume * 100.0));
                            ui.label(RichText::new("Speed").color(DIM));
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.speed, 0.5..=1.5)
                                        .fixed_decimals(2),
                                )
                                .changed()
                            {
                                if let Some(s) = &self.show {
                                    let _ = s.set_speed(self.speed);
                                }
                            }
                        });
                        if !self.recent_urls.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Recent").color(DIM).small());
                                egui::ComboBox::from_id_salt("recent_urls")
                                    .selected_text("Pick…")
                                    .show_ui(ui, |ui| {
                                        for u in self.recent_urls.clone() {
                                            if ui.selectable_label(false, &u).clicked() {
                                                self.url = u;
                                            }
                                        }
                                    });
                            });
                        }
                    }
                    if self.busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Loading…").color(DIM));
                        });
                    }
                });

            ui.add_space(8.0);

            // Player
            egui::Frame::none()
                .fill(PANEL)
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("NOW PLAYING").color(DIM).small().strong());
                        if !self.video_id.is_empty() {
                            let star = if self.favorites.contains(&self.video_id) {
                                "★ Favorited"
                            } else {
                                "☆ Favorite"
                            };
                            if ui.small_button(star).clicked() {
                                let on = self.favorites.toggle(&self.video_id);
                                self.status = if on {
                                    "Added to favorites".into()
                                } else {
                                    "Removed favorite".into()
                                };
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let pos = self.show.as_ref().map(|s| s.position_s()).unwrap_or(0.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    Self::fmt_time(pos),
                                    Self::fmt_time(self.duration_s)
                                ))
                                .color(ACCENT)
                                .monospace()
                                .strong(),
                            );
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&self.song_label).size(15.0).strong());
                        if self.lrc_quality > 0.0 {
                            ui.label(
                                RichText::new(format!("LRC★{:.0}", self.lrc_quality))
                                    .color(ACCENT)
                                    .small(),
                            );
                        }
                        if let Some(show) = &self.show {
                            if let Some(eta) = show.secs_to_next_line() {
                                if eta > 0.05 && eta < 30.0 {
                                    ui.label(
                                        RichText::new(format!("next in {eta:.1}s"))
                                            .color(DIM)
                                            .small(),
                                    );
                                }
                            }
                        }
                    });

                    let scrub = ui.add(
                        egui::Slider::new(&mut self.scrub, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                    if scrub.drag_started() {
                        self.scrubbing = true;
                    }
                    if scrub.drag_stopped() {
                        self.scrubbing = false;
                        if let Some(show) = &self.show {
                            let _ = show.seek(self.scrub as f64 * self.duration_s.max(0.01));
                        }
                    }
                    // Per-line lyric progress (karaoke fill)
                    if let Some(show) = &self.show {
                        if show.is_running() || show.is_paused() {
                            let lp = show.line_progress();
                            ui.add(
                                egui::ProgressBar::new(lp)
                                    .desired_width(f32::INFINITY)
                                    .text(format!("line {:.0}%", lp * 100.0)),
                            );
                            // Soft visualizer bars (position-driven, streamer aesthetic)
                            ui.horizontal(|ui| {
                                let n = 24;
                                let hmax = 18.0_f32;
                                for i in 0..n {
                                    let phase = self.visualizer_phase + i as f32 * 0.45;
                                    let h = ((phase.sin().abs()) * hmax).max(2.0);
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(6.0, hmax),
                                        Sense::hover(),
                                    );
                                    let bar = egui::Rect::from_min_size(
                                        egui::pos2(rect.min.x, rect.max.y - h),
                                        Vec2::new(5.0, h),
                                    );
                                    ui.painter().rect_filled(bar, 1.0, ACCENT);
                                }
                            });
                        }
                    }
                    if self.show_upcoming && self.active_line >= 0 {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Next:").color(DIM).small());
                            for k in 1..=3 {
                                if let Some(ln) =
                                    self.timed_lines.get((self.active_line + k) as usize)
                                {
                                    ui.label(
                                        RichText::new(format!("· {}", ln.text))
                                            .color(DIM)
                                            .small(),
                                    );
                                }
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        if ui.button("⏮").on_hover_text("Previous").clicked() {
                            self.prev_song();
                        }
                        if ui.button("⏪ −10s").clicked() {
                            self.seek_rel(-10.0);
                        }
                        let paused = self.show.as_ref().map(|s| s.is_paused()).unwrap_or(false);
                        if ui
                            .add(
                                egui::Button::new(if paused { "▶ Resume" } else { "⏸ Pause" })
                                    .min_size(Vec2::new(88.0, 28.0)),
                            )
                            .clicked()
                        {
                            if let Some(s) = &self.show {
                                s.toggle_pause();
                            } else {
                                self.start_current();
                            }
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("▶ Play").strong())
                                    .fill(ACCENT)
                                    .min_size(Vec2::new(80.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.start_current();
                        }
                        if ui.button("+10s ⏩").clicked() {
                            self.seek_rel(10.0);
                        }
                        if ui.button("⏭").on_hover_text("Next").clicked() {
                            self.next_song();
                        }
                        if ui
                            .add(
                                egui::Button::new("■ Stop")
                                    .fill(Color32::from_rgb(0x5a, 0x1a, 0x1a)),
                            )
                            .clicked()
                        {
                            self.stop_show();
                        }
                        let muted = self.show.as_ref().map(|s| s.is_muted()).unwrap_or(false);
                        if ui.button(if muted { "🔊" } else { "🔇" }).clicked() {
                            if let Some(s) = &self.show {
                                s.toggle_mute();
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.small_button("Sample").clicked() {
                            self.stop_show();
                            self.synced_lrc = None;
                            self.audio_path = None;
                            self.duration_s = 12.0;
                            self.lyrics_edit =
                                "hello world\nlights on stage\nsing with me".into();
                            self.song_label = "Sample lyrics".into();
                            self.timed_lines =
                                build_timed_lines(&self.lyrics_edit, None, 12.0);
                        }
                        if ui.small_button("Open file…").clicked() {
                            self.open_lyrics_file();
                        }
                        if ui
                            .small_button("Open audio…")
                            .on_hover_text("Local mp3/m4a/wav")
                            .clicked()
                        {
                            self.open_local_audio();
                        }
                        if ui
                            .small_button("Chroma test")
                            .on_hover_text("Sweep keyboard lights")
                            .clicked()
                        {
                            self.chroma_test();
                        }
                        if ui.small_button("Open URL").clicked() && !self.url.is_empty() {
                            let _ = open::that(self.url.trim());
                        }
                        if ui.small_button("Clear").clicked() {
                            self.stop_show();
                            self.lyrics_edit.clear();
                            self.url.clear();
                            self.synced_lrc = None;
                            self.timed_lines.clear();
                            self.song_label = "No song loaded".into();
                        }
                        if ui.small_button("Replay").clicked() {
                            self.start_current();
                        }
                        if ui
                            .small_button("Replay line")
                            .on_hover_text("R — jump to start of current lyric")
                            .clicked()
                        {
                            if let Some(s) = &self.show {
                                if let Ok(t) = s.replay_line() {
                                    self.status =
                                        format!("Replay line @ {}", Self::fmt_time(t));
                                }
                            }
                        }
                        if ui
                            .small_button("Loop line")
                            .on_hover_text("L — A-B practice loop on current line")
                            .clicked()
                        {
                            self.loop_current_line();
                        }
                        if ui
                            .small_button("Chorus")
                            .on_hover_text("Jump near chorus (heuristic)")
                            .clicked()
                        {
                            self.jump_to_chorus();
                        }
                        if ui
                            .small_button("Restart")
                            .on_hover_text("Seek to 0:00")
                            .clicked()
                        {
                            self.restart_track();
                        }
                        if ui
                            .small_button("Paste lyrics")
                            .on_hover_text("Import lyrics/LRC from clipboard")
                            .clicked()
                        {
                            self.import_lyrics_clipboard();
                        }
                        if ui
                            .small_button("Export pack")
                            .on_hover_text("JSON song package")
                            .clicked()
                        {
                            self.export_song_package_json();
                        }
                        if ui.small_button("−30s").clicked() {
                            self.seek_rel(-30.0);
                        }
                        if ui.small_button("+30s").clicked() {
                            self.seek_rel(30.0);
                        }
                        if ui.small_button("Copy lyric").clicked() && self.active_line >= 0 {
                            if let Some(ln) = self.timed_lines.get(self.active_line as usize) {
                                ui.output_mut(|o| o.copied_text = ln.text.clone());
                                self.status = "Copied current lyric".into();
                            }
                        }
                        if ui.small_button("Export LRC").clicked() {
                            self.export_lrc();
                        }
                        if ui
                            .small_button("Export timed")
                            .on_hover_text("Write current timed lines as LRC")
                            .clicked()
                        {
                            self.export_timed_as_lrc();
                        }
                        if ui.small_button("F11 Karaoke").clicked() {
                            self.fullscreen_karaoke = true;
                        }
                        if ui.small_button("25%").clicked() {
                            self.seek_percent(0.25);
                        }
                        if ui.small_button("50%").clicked() {
                            self.seek_percent(0.5);
                        }
                        if ui.small_button("75%").clicked() {
                            self.seek_percent(0.75);
                        }
                        if ui
                            .small_button("Snap")
                            .on_hover_text("Snap to lyric (S)")
                            .clicked()
                        {
                            if let Some(s) = &self.show {
                                if let Ok(t) = s.snap_to_lyric() {
                                    self.status =
                                        format!("Snap → lyric @ {}", Self::fmt_time(t));
                                }
                            }
                        }
                        if ui.small_button("Skip intro").on_hover_text("I").clicked() {
                            if let Some(s) = &self.show {
                                if let Ok(t) = s.skip_intro() {
                                    self.status =
                                        format!("Skip intro → {}", Self::fmt_time(t));
                                }
                            }
                        }
                        if ui.small_button("−5s").clicked() {
                            self.seek_rel(-5.0);
                        }
                        if ui.small_button("A⟷B").on_hover_text("Set A-B loop").clicked() {
                            if let Some(s) = &self.show {
                                let t = s.position_s();
                                if self.ab_a.is_none() {
                                    self.ab_a = Some(t);
                                    self.status = format!("Loop A @ {}", Self::fmt_time(t));
                                } else if self.ab_b.is_none() {
                                    let a = self.ab_a.unwrap_or(0.0);
                                    if t > a + 0.3 {
                                        self.ab_b = Some(t);
                                        s.set_ab_loop(self.ab_a, self.ab_b);
                                        self.status = format!(
                                            "Loop A-B {}–{}",
                                            Self::fmt_time(a),
                                            Self::fmt_time(t)
                                        );
                                    }
                                } else {
                                    self.ab_a = None;
                                    self.ab_b = None;
                                    s.clear_ab_loop();
                                    self.status = "A-B loop cleared".into();
                                }
                            }
                        }
                        if ui.small_button("📌").on_hover_text("Bookmark Ctrl+B").clicked() {
                            if let Some(s) = &self.show {
                                let t = s.position_s();
                                self.bookmarks.add(
                                    &self.video_id,
                                    &self.song_label,
                                    t,
                                    "mark",
                                );
                                self.status = format!("Bookmark @ {}", Self::fmt_time(t));
                            }
                        }
                        if ui.small_button("Share").clicked() {
                            let text = format!(
                                "{} {}\n{}",
                                self.song_label,
                                if self.url.is_empty() {
                                    String::new()
                                } else {
                                    self.url.clone()
                                },
                                self.stats.summary()
                            );
                            ui.output_mut(|o| o.copied_text = text);
                            self.status = "Copied share text".into();
                        }
                        if ui.small_button("Export TXT").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Text", &["txt"])
                                .set_file_name("lyrics.txt")
                                .save_file()
                            {
                                if std::fs::write(&path, &self.lyrics_edit).is_ok() {
                                    self.status = format!("Exported {}", path.display());
                                }
                            }
                        }
                    });
                    // Bookmarks for current song
                    let marks = self.bookmarks.for_video(&self.video_id);
                    if !marks.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Marks:").color(DIM).small());
                            for m in marks {
                                if ui
                                    .small_button(format!("{} ({})", m.label, Self::fmt_time(m.t)))
                                    .clicked()
                                {
                                    if let Some(s) = &self.show {
                                        let _ = s.seek(m.t);
                                    }
                                }
                            }
                        });
                    }
                });

            if !self.mini_player {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("LYRICS").color(ACCENT).strong());
                    ui.label(RichText::new("click a line to seek").color(DIM).small());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.lyric_filter)
                            .hint_text("Filter…")
                            .desired_width(140.0),
                    );
                    ui.checkbox(&mut self.hide_past_lyrics, "Hide past");
                });
                if !self.video_id.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Note").color(DIM).small());
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.song_note)
                                    .hint_text("Personal note…")
                                    .desired_width(220.0),
                            )
                            .lost_focus()
                        {
                            self.song_memory
                                .set_note(&self.video_id, &self.song_note);
                        }
                        ui.label(RichText::new("★").color(DIM).small());
                        let rating = self.song_memory.rating(&self.video_id);
                        for r in 1u8..=5 {
                            let star = if r <= rating { "★" } else { "☆" };
                            if ui.small_button(star).clicked() {
                                let new_r = if r == rating { 0 } else { r };
                                self.song_memory.set_rating(&self.video_id, new_r);
                            }
                        }
                    });
                }

                egui::Frame::none()
                    .fill(Color32::from_rgb(0x0f, 0x0f, 0x13))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        let h = (ui.available_height() - 130.0).max(140.0);
                        let mut seek_t: Option<f64> = None;
                        ScrollArea::vertical().max_height(h).show(ui, |ui| {
                            if !self.timed_lines.is_empty() && self.show.is_some() {
                                let filter = self.lyric_filter.to_lowercase();
                                for (i, ln) in self.timed_lines.iter().enumerate() {
                                    if !filter.is_empty()
                                        && !ln.text.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if self.hide_past_lyrics
                                        && self.active_line >= 0
                                        && (i as isize) < self.active_line
                                    {
                                        continue;
                                    }
                                    let base = self.lyric_font;
                                    let (color, size, strong) =
                                        match (i as isize).cmp(&self.active_line) {
                                            std::cmp::Ordering::Equal => {
                                                (ACCENT, base + 3.0, true)
                                            }
                                            std::cmp::Ordering::Less => (PAST, base - 1.0, false),
                                            std::cmp::Ordering::Greater => (DIM, base, false),
                                        };
                                    let mut rt =
                                        RichText::new(format!("  {}", ln.text)).color(color).size(size);
                                    if strong {
                                        rt = rt.strong();
                                    }
                                    let resp = ui.add(
                                        egui::Label::new(rt).sense(Sense::click()),
                                    );
                                    if resp.clicked() {
                                        seek_t = Some(ln.t);
                                    }
                                    if i as isize == self.active_line {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                        // Word-level highlight under active line
                                        if self.word_highlight && !self.timed_words.is_empty() {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                for (wi, w) in self.timed_words.iter().enumerate()
                                                {
                                                    if w.t < ln.t - 0.05 || w.t > ln.end_t + 0.15
                                                    {
                                                        continue;
                                                    }
                                                    let on = self.active_word == Some(wi);
                                                    ui.label(
                                                        RichText::new(&w.word)
                                                            .color(if on {
                                                                ACCENT
                                                            } else {
                                                                DIM
                                                            })
                                                            .size(if on {
                                                                base + 2.0
                                                            } else {
                                                                base - 1.0
                                                            })
                                                            .strong(),
                                                    );
                                                }
                                            });
                                        }
                                    }
                                }
                            } else {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.lyrics_edit)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(8),
                                );
                            }
                        });
                        if let Some(t) = seek_t {
                            if let Some(show) = &self.show {
                                let _ = show.seek((t + self.offset as f64).max(0.0));
                                self.status =
                                    format!("Jumped to lyric @ {}", Self::fmt_time(t));
                            }
                        }
                    });

                ui.collapsing("Dual lyrics / translation (paste line-by-line)", |ui| {
                    ui.label(
                        RichText::new(
                            "Paste a translation with the same number of lines as the song — shown in party/karaoke mode.",
                        )
                        .color(DIM)
                        .small(),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.dual_lyrics)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4)
                            .hint_text("Optional second language…"),
                    );
                });

                ui.add_space(6.0);
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.columns(2, |cols| {
                            cols[0].vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Offset").color(DIM));
                                    if ui.small_button("−").on_hover_text("[  −0.1s").clicked() {
                                        self.offset = (self.offset - 0.1).clamp(-10.0, 20.0);
                                    }
                                    ui.add(
                                        egui::Slider::new(&mut self.offset, -10.0..=20.0)
                                            .suffix("s"),
                                    );
                                    if ui.small_button("+").on_hover_text("]  +0.1s").clicked() {
                                        self.offset = (self.offset + 0.1).clamp(-10.0, 20.0);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Bright").color(DIM));
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut self.brightness, 0.1..=1.0)
                                                .show_value(false),
                                        )
                                        .changed()
                                    {
                                        self.apply_live_show_opts();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Theme").color(DIM));
                                    egui::ComboBox::from_id_salt("theme")
                                        .selected_text(self.theme.label())
                                        .show_ui(ui, |ui| {
                                            for th in LightTheme::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut self.theme,
                                                        th,
                                                        th.label(),
                                                    )
                                                    .changed()
                                                {
                                                    if let Some(s) = &self.show {
                                                        s.set_theme(th);
                                                    }
                                                }
                                            }
                                        });
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Ambient FX").color(DIM));
                                    egui::ComboBox::from_id_salt("ambient_fx")
                                        .selected_text(self.ambient_effect.label())
                                        .show_ui(ui, |ui| {
                                            for fx in AmbientEffect::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut self.ambient_effect,
                                                        fx,
                                                        fx.label(),
                                                    )
                                                    .changed()
                                                {
                                                    self.ambient_pulse = fx.is_on();
                                                    self.apply_live_show_opts();
                                                    self.persist_settings();
                                                }
                                            }
                                        });
                                });
                            });
                            cols[1].vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Mode").color(DIM));
                                    ui.selectable_value(
                                        &mut self.mode,
                                        PlayMode::FlashWord,
                                        "Word",
                                    );
                                    ui.selectable_value(
                                        &mut self.mode,
                                        PlayMode::FlashLine,
                                        "Line",
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Sleep").color(DIM));
                                    if ui.small_button("15m").clicked() {
                                        self.sleep_until =
                                            Some(Instant::now() + Duration::from_secs(15 * 60));
                                        self.status = "Sleep timer: 15 min".into();
                                    }
                                    if ui.small_button("30m").clicked() {
                                        self.sleep_until =
                                            Some(Instant::now() + Duration::from_secs(30 * 60));
                                        self.status = "Sleep timer: 30 min".into();
                                    }
                                    if ui.small_button("60m").clicked() {
                                        self.sleep_until =
                                            Some(Instant::now() + Duration::from_secs(60 * 60));
                                        self.status = "Sleep timer: 60 min".into();
                                    }
                                    if ui.small_button("Off").clicked() {
                                        self.sleep_until = None;
                                        self.status = "Sleep timer off".into();
                                    }
                                    if let Some(until) = self.sleep_until {
                                        let left = until
                                            .saturating_duration_since(Instant::now())
                                            .as_secs();
                                        ui.label(
                                            RichText::new(format!(
                                                "{:02}:{:02}",
                                                left / 60,
                                                left % 60
                                            ))
                                            .color(ACCENT)
                                            .small(),
                                        );
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Font").color(DIM));
                                    ui.add(
                                        egui::Slider::new(&mut self.lyric_font, 11.0..=24.0)
                                            .show_value(false),
                                    );
                                    ui.checkbox(&mut self.fade_on_stop, "Fade stop");
                                    if ui.checkbox(&mut self.ambient_pulse, "Ambient").changed() {
                                        if self.ambient_pulse
                                            && self.ambient_effect == AmbientEffect::Off
                                        {
                                            self.ambient_effect = AmbientEffect::Pulse;
                                        }
                                        self.apply_live_show_opts();
                                    }
                                    if ui.checkbox(&mut self.night_dim, "Night").changed() {
                                        self.apply_live_show_opts();
                                    }
                                    if ui
                                        .checkbox(&mut self.auto_night_dim, "Auto night")
                                        .on_hover_text("Dim 22:00–07:00 local")
                                        .changed()
                                    {
                                        self.apply_live_show_opts();
                                        self.persist_settings();
                                    }
                                    if ui
                                        .checkbox(&mut self.mirror_lights, "Mirror FX")
                                        .changed()
                                    {
                                        self.apply_live_show_opts();
                                        self.persist_settings();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut self.resume_position, "Resume pos");
                                    ui.checkbox(&mut self.remember_offset, "Remember offset");
                                    ui.checkbox(&mut self.crossfade_next, "Crossfade next");
                                    ui.checkbox(&mut self.sleep_fade, "Sleep fade");
                                    ui.checkbox(&mut self.fade_in, "Fade in");
                                    ui.checkbox(&mut self.auto_skip_intro, "Auto skip intro");
                                    ui.checkbox(&mut self.word_highlight, "Word highlight");
                                    ui.checkbox(&mut self.show_upcoming, "Upcoming");
                                    ui.checkbox(&mut self.now_playing_file, "OBS now_playing");
                                    ui.checkbox(&mut self.preroll_countdown, "3-2-1 countdown");
                                    ui.checkbox(&mut self.end_fade, "End fade");
                                    ui.checkbox(&mut self.pause_on_unfocus, "Pause unfocus");
                                    ui.checkbox(&mut self.mute_lights_with_audio, "Mute→dim lights");
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Vol preset").color(DIM).small());
                                    if ui.small_button("Quiet").clicked() {
                                        self.volume = 0.35;
                                        if let Some(s) = &self.show {
                                            s.set_volume(self.volume);
                                        }
                                    }
                                    if ui.small_button("Norm").clicked() {
                                        self.volume = 0.85;
                                        if let Some(s) = &self.show {
                                            s.set_volume(self.volume);
                                        }
                                    }
                                    if ui.small_button("Loud").clicked() {
                                        self.volume = 1.0;
                                        if let Some(s) = &self.show {
                                            s.set_volume(self.volume);
                                        }
                                    }
                                    if ui.small_button("F1 Help").clicked() {
                                        self.show_help = true;
                                    }
                                    if ui
                                        .small_button("Export stats")
                                        .on_hover_text("CSV of listen stats")
                                        .clicked()
                                    {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("CSV", &["csv"])
                                            .set_file_name("listen_stats.csv")
                                            .save_file()
                                        {
                                            if std::fs::write(&path, self.stats.to_csv()).is_ok()
                                            {
                                                self.status =
                                                    format!("Exported {}", path.display());
                                            }
                                        }
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("YT cookies").color(DIM).small());
                                    egui::ComboBox::from_id_salt("yt_cookies_browser")
                                        .selected_text(&self.ytdlp_cookies_browser)
                                        .width(90.0)
                                        .show_ui(ui, |ui| {
                                            for b in COOKIE_BROWSERS {
                                                if ui
                                                    .selectable_value(
                                                        &mut self.ytdlp_cookies_browser,
                                                        (*b).to_string(),
                                                        *b,
                                                    )
                                                    .changed()
                                                {
                                                    self.persist_settings();
                                                    self.status = format!(
                                                        "YouTube cookies: browser = {b}"
                                                    );
                                                }
                                            }
                                        });
                                    if ui
                                        .small_button("cookies.txt…")
                                        .on_hover_text(
                                            "Pick Netscape cookies.txt exported while logged into YouTube",
                                        )
                                        .clicked()
                                    {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("Cookies", &["txt"])
                                            .pick_file()
                                        {
                                            self.ytdlp_cookies_file =
                                                path.display().to_string();
                                            self.persist_settings();
                                            self.status = format!(
                                                "Cookies file: {}",
                                                self.ytdlp_cookies_file
                                            );
                                        }
                                    }
                                    if !self.ytdlp_cookies_file.is_empty()
                                        && ui.small_button("Clear file").clicked()
                                    {
                                        self.ytdlp_cookies_file.clear();
                                        self.persist_settings();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&self.cache_label).color(DIM).small(),
                                    );
                                    if ui.small_button("Clear cache").clicked() {
                                        let root = default_cache_root();
                                        match clear_media_cache(&root) {
                                            Ok((b, _)) => {
                                                self.refresh_cache_label();
                                                self.status = format!(
                                                    "Cleared cache ({})",
                                                    format_bytes(b)
                                                );
                                            }
                                            Err(e) => {
                                                self.error_popup = Some(e.to_string())
                                            }
                                        }
                                    }
                                    if ui.small_button("Save prefs").clicked() {
                                        self.persist_settings();
                                        self.status = "Settings saved".into();
                                    }
                                });
                                ui.label(
                                    RichText::new(self.stats.summary())
                                        .color(DIM)
                                        .small(),
                                );
                                let sess = self.session_start.elapsed().as_secs();
                                ui.label(
                                    RichText::new(format!(
                                        "{} · session {:02}:{:02}",
                                        self.stats.daily_summary(),
                                        sess / 60,
                                        sess % 60
                                    ))
                                    .color(DIM)
                                    .small(),
                                );
                                let (day_h, goal) = self.stats.daily_progress();
                                ui.add(
                                    egui::ProgressBar::new(
                                        (day_h / goal as f64).clamp(0.0, 1.0) as f32,
                                    )
                                    .desired_width(180.0)
                                    .text(format!("day goal {:.0}%", (day_h / goal as f64) * 100.0)),
                                );
                            });
                        });
                    });
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_settings();
        self.stop_show();
    }
}
