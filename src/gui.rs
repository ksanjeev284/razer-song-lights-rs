//! Full desktop GUI (egui) — optimized with pause, mute, speed, shortcuts, favorites.

use crate::cache::{
    cache_size_bytes, cached_song_count, clear_media_cache, default_cache_root, format_bytes,
    load_song_package, save_song_package, SongPackage,
};
use crate::chroma::is_chroma_supported;
use crate::history::{load_history_packages, push_history, PlayQueue};
use crate::lrclib::fetch_lyrics;
use crate::settings::{AppSettings, Favorites};
use crate::show::{build_timed_lines, start_show, PlayMode, ShowConfig, ShowHandle};
use crate::stats::{Bookmarks, ListenStats};
use crate::sync_sim::line_index_for_time;
use crate::themes::{LightTheme, RepeatMode};
use crate::title::is_youtube_url;
use crate::youtube::{download_audio, extract_meta};
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

enum LoadMsg {
    Ok(SongPackage),
    Err(String),
    Status(String),
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
        }
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
            ..AppSettings::default()
        };
        s.set_play_mode(self.mode);
        s.save();
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
        if let Some(mut s) = self.show.take() {
            if self.fade_on_stop && s.is_running() {
                s.fade_stop();
                // join in background via drop of remaining handle after brief wait
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(1400));
                    drop(s);
                });
            } else {
                s.stop();
            }
        }
        self.was_ended = false;
        self.prefetched = false;
        self.status = "Stopped".into();
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

        self.timed_lines = if self.clock_sync {
            build_timed_lines(&plain, self.synced_lrc.as_deref(), self.duration_s)
        } else {
            build_timed_lines(&plain, None, 0.0)
        };
        self.active_line = -1;

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
            brightness: self.brightness,
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
                h.set_brightness(self.brightness);
                h.set_ab_loop(self.ab_a, self.ab_b);
                self.status = format!("Playing — {}", self.song_label);
                self.stats.record_play(&self.song_label);
                self.show = Some(h);
                self.was_ended = false;
                self.prefetched = false;
                self.listen_tick = Instant::now();
            }
            Err(e) => self.error_popup = Some(format!("Play failed: {e}")),
        }
        self.persist_settings();
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
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        thread::spawn(move || {
            let root = default_cache_root();
            let audio_dir = root.join("audio");
            let send = |m: LoadMsg| {
                let _ = tx.send(m);
            };

            send(LoadMsg::Status("Fetching YouTube metadata…".into()));
            let meta = match extract_meta(&url) {
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
                        if let Ok(p) = download_audio(&meta.url, &meta.video_id, &audio_dir) {
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
                send(LoadMsg::Status("Downloading audio…".into()));
                match download_audio(&meta.url, &meta.video_id, &audio_dir) {
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
            if pkg.is_synced() {
                "SYNCED"
            } else {
                "est."
            }
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
                    self.load_rx = None;
                    self.apply_package(pkg, true);
                    return;
                }
                Ok(LoadMsg::Err(e)) => {
                    self.busy = false;
                    self.load_rx = None;
                    self.status = "Load failed".into();
                    self.error_popup = Some(e);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.load_rx = None;
                    break;
                }
            }
        }
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
        ctx.input(|i| {
            if i.key_pressed(Key::Space) {
                space = true;
            }
            if i.key_pressed(Key::ArrowLeft) {
                left = true;
            }
            if i.key_pressed(Key::ArrowRight) {
                right = true;
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
        });
        let f11 = ctx.input(|i| i.key_pressed(Key::F11));
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
            self.seek_rel(-10.0);
        }
        if right && !focus_text {
            self.seek_rel(10.0);
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
                if path.extension().map(|e| e == "lrc").unwrap_or(false) || content.contains('[')
                {
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
        self.handle_shortcuts(ctx);

        // Sleep timer
        if let Some(until) = self.sleep_until {
            if Instant::now() >= until {
                self.sleep_until = None;
                self.stop_show();
                self.status = "Sleep timer — stopped".into();
            } else {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }

        // Auto-next / repeat-all when track ends
        if let Some(show) = &self.show {
            if show.has_ended() && !self.was_ended {
                self.was_ended = true;
                let can_advance = self.auto_next
                    || self.repeat == RepeatMode::All
                    || self.shuffle;
                if can_advance
                    && (self.queue.can_next()
                        || self.repeat == RepeatMode::All
                        || self.shuffle)
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
            show.set_volume(self.volume);
            show.set_theme(self.theme);
            show.set_brightness(self.brightness);
            show.set_ab_loop(self.ab_a, self.ab_b);
            let pos = show.position_s();
            let dur = self.duration_s.max(show.duration_s());
            if !self.scrubbing && dur > 0.0 {
                self.scrub = (pos / dur).clamp(0.0, 1.0) as f32;
            }
            self.active_line =
                line_index_for_time(&self.timed_lines, pos, self.offset as f64);
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
                                    thread::spawn(move || {
                                        let root = default_cache_root();
                                        let _ = download_audio(
                                            &url,
                                            &vid,
                                            &root.join("audio"),
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
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            if self.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            },
        ));

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

        egui::TopBottomPanel::bottom("status").exact_height(30.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    4.5,
                    if self.chroma_ok { ACCENT } else { DANGER },
                );
                ui.label(RichText::new(&self.status).color(DIM).size(12.5));
            });
        });

        // History / favorites side panel
        if self.show_history {
            egui::SidePanel::right("history")
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.heading(RichText::new("Library").color(ACCENT).size(14.0));
                    ui.separator();
                    ui.label(RichText::new("History").color(DIM).small().strong());
                    let mut play_idx: Option<usize> = None;
                    ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                        for (i, s) in self.queue.songs.iter().enumerate().rev() {
                            let selected = i as isize == self.queue.index;
                            let star = if self.favorites.contains(&s.video_id) {
                                "★ "
                            } else {
                                ""
                            };
                            let label = format!("{star}{}", s.display_name());
                            if ui
                                .selectable_label(selected, RichText::new(label).size(12.0))
                                .clicked()
                            {
                                play_idx = Some(i);
                            }
                        }
                    });
                    if let Some(i) = play_idx {
                        self.queue.index = i as isize;
                        if let Some(pkg) = self.queue.current().cloned() {
                            self.apply_package(pkg, true);
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Favorites").color(DIM).small().strong());
                    ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
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
                });
        }

        // Fullscreen karaoke overlay
        if self.fullscreen_karaoke {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.label(RichText::new(&self.song_label).color(DIM).size(14.0));
                    ui.add_space(20.0);
                    if self.active_line >= 0 {
                        if let Some(ln) = self.timed_lines.get(self.active_line as usize) {
                            ui.label(
                                RichText::new(&ln.text)
                                    .color(ACCENT)
                                    .size(36.0)
                                    .strong(),
                            );
                        }
                    } else {
                        ui.label(RichText::new("…").color(DIM).size(28.0));
                    }
                    if let Some(next) = self
                        .timed_lines
                        .get((self.active_line + 1) as usize)
                    {
                        ui.add_space(16.0);
                        ui.label(RichText::new(&next.text).color(DIM).size(20.0));
                    }
                    ui.add_space(40.0);
                    ui.label(RichText::new("F11 to exit fullscreen karaoke").color(DIM).small());
                });
            });
            return;
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
                    });
                });
                ui.label(
                    RichText::new(
                        "Space pause · ←/→ seek · N/P next/prev · I skip intro · M mute · Ctrl+B bookmark · F11 karaoke · drop URL/LRC",
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
                        let w = ui.available_width() - 120.0;
                        ui.add_sized(
                            [w, 26.0],
                            egui::TextEdit::singleline(&mut self.url)
                                .hint_text("https://youtube.com/watch?v=…"),
                        );
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
                    ui.label(RichText::new(&self.song_label).size(15.0).strong());

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
                        if ui.small_button("Copy lyric").clicked() {
                            if self.active_line >= 0 {
                                if let Some(ln) =
                                    self.timed_lines.get(self.active_line as usize)
                                {
                                    ui.output_mut(|o| o.copied_text = ln.text.clone());
                                    self.status = "Copied current lyric".into();
                                }
                            }
                        }
                        if ui.small_button("Export LRC").clicked() {
                            self.export_lrc();
                        }
                        if ui.small_button("F11 Karaoke").clicked() {
                            self.fullscreen_karaoke = true;
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
                });

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
                                    let base = self.lyric_font;
                                let (color, size, strong) =
                                        if i as isize == self.active_line {
                                            (ACCENT, base + 3.0, true)
                                        } else if (i as isize) < self.active_line {
                                            (PAST, base - 1.0, false)
                                        } else {
                                            (DIM, base, false)
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

                ui.add_space(6.0);
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.columns(2, |cols| {
                            cols[0].vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Offset").color(DIM));
                                    ui.add(
                                        egui::Slider::new(&mut self.offset, -10.0..=20.0)
                                            .suffix("s"),
                                    );
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
                                        if let Some(s) = &self.show {
                                            s.set_brightness(self.brightness);
                                        }
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
