//! Full desktop GUI (egui) — feature parity with the Python app.

use crate::cache::{
    default_cache_root, load_song_package, save_song_package, SongPackage,
};
use crate::chroma::is_chroma_supported;
use crate::history::{load_history_packages, push_history, PlayQueue};
use crate::lrclib::fetch_lyrics;
use crate::show::{build_timed_lines, start_show, PlayMode, ShowConfig, ShowHandle};
use crate::sync_sim::line_index_for_time;
use crate::title::is_youtube_url;
use crate::youtube::{download_audio, extract_meta};
use crate::VERSION;
use eframe::egui::{self, Color32, RichText, ScrollArea, Sense, Vec2};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

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
            .with_inner_size([980.0, 820.0])
            .with_min_inner_size([860.0, 700.0])
            .with_title(format!("Razer Song Lights {VERSION}")),
        ..Default::default()
    };
    eframe::run_native(
        "Razer Song Lights",
        options,
        Box::new(|cc| {
            // Dark visuals
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
    volume: f32,
    offset: f32,
    mode: PlayMode,

    show: Option<ShowHandle>,
    queue: PlayQueue,
    timed_lines: Vec<crate::lrc::TimedLine>,
    active_line: isize,
    scrub: f32,
    scrubbing: bool,

    busy: bool,
    load_rx: Option<Receiver<LoadMsg>>,
    chroma_ok: bool,
    error_popup: Option<String>,
}

impl SongLightsGui {
    fn new() -> Self {
        let root = default_cache_root();
        let mut queue = PlayQueue::default();
        let hist = load_history_packages(&root, 20);
        for s in hist {
            queue.push(s);
        }
        if !queue.songs.is_empty() {
            queue.index = queue.songs.len() as isize - 1;
        }

        Self {
            url: String::new(),
            lyrics_edit: "hello world\nlights on stage\nsing with me".into(),
            song_label: "No song loaded — paste a YouTube link".into(),
            status: if is_chroma_supported() {
                "Ready — paste a YouTube URL or lyrics, then Play.".into()
            } else {
                "Ready (Chroma lights require Windows + Synapse).".into()
            },
            duration_s: 12.0,
            synced_lrc: None,
            audio_path: None,
            video_id: String::new(),
            play_audio: true,
            clock_sync: true,
            cache_music: true,
            lights: true,
            loop_play: false,
            volume: 0.85,
            offset: 0.0,
            mode: PlayMode::FlashWord,
            show: None,
            queue,
            timed_lines: Vec::new(),
            active_line: -1,
            scrub: 0.0,
            scrubbing: false,
            busy: false,
            load_rx: None,
            chroma_ok: is_chroma_supported(),
            error_popup: None,
        }
    }

    fn stop_show(&mut self) {
        if let Some(mut s) = self.show.take() {
            s.stop();
        }
        self.status = "Stopped".into();
    }

    fn start_current(&mut self) {
        self.stop_show();
        let plain = self.lyrics_edit.clone();
        if plain.trim().is_empty() {
            self.error_popup = Some("Type lyrics or load a YouTube song first.".into());
            return;
        }
        // If lyrics box is only a URL, load youtube
        if is_youtube_url(plain.trim()) && !plain.contains('\n') {
            self.url = plain.trim().to_string();
            self.load_youtube();
            return;
        }

        let lines = if self.clock_sync {
            build_timed_lines(
                &plain,
                self.synced_lrc.as_deref(),
                self.duration_s,
            )
        } else {
            build_timed_lines(&plain, None, 0.0)
        };
        self.timed_lines = lines.clone();
        self.active_line = -1;

        let cfg = ShowConfig {
            mode: self.mode,
            volume: self.volume,
            sync_offset_s: self.offset as f64,
            play_audio: self.play_audio && self.audio_path.is_some(),
            lights: self.lights,
            karaoke_console: false,
            loop_play: self.loop_play,
        };

        match start_show(
            &plain,
            self.synced_lrc.as_deref(),
            self.duration_s,
            self.audio_path.as_deref(),
            &cfg,
        ) {
            Ok(h) => {
                self.status = format!("Playing — {}", self.song_label);
                self.show = Some(h);
            }
            Err(e) => {
                self.error_popup = Some(format!("Play failed: {e}"));
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
                if let Some(pkg) = load_song_package(&root, &meta.video_id) {
                    send(LoadMsg::Status(format!(
                        "Loaded from cache: {}",
                        pkg.display_name()
                    )));
                    let mut pkg = pkg;
                    if want_audio && pkg.audio_path.as_ref().map(|p| !PathBuf::from(p).is_file()).unwrap_or(true)
                    {
                        send(LoadMsg::Status("Downloading audio…".into()));
                        match download_audio(&meta.url, &meta.video_id, &audio_dir) {
                            Ok(p) => pkg.audio_path = Some(p.display().to_string()),
                            Err(e) => send(LoadMsg::Status(format!("Audio skip: {e}"))),
                        }
                    }
                    push_history(&root, &meta.video_id, 40);
                    send(LoadMsg::Ok(pkg));
                    return;
                }
            }

            send(LoadMsg::Status(format!(
                "Searching lyrics for {} — {}…",
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
                "Lyrics: {} ({})",
                hit.source,
                if hit.is_synced() {
                    "synced"
                } else {
                    "plain"
                }
            )));

            let mut audio_path = None;
            if want_audio {
                send(LoadMsg::Status("Downloading audio…".into()));
                match download_audio(&meta.url, &meta.video_id, &audio_dir) {
                    Ok(p) => audio_path = Some(p.display().to_string()),
                    Err(e) => send(LoadMsg::Status(format!("Audio unavailable: {e}"))),
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
                "SYNCED LRC"
            } else {
                "est. timing"
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
        self.timed_lines = build_timed_lines(
            &pkg.lyrics,
            pkg.synced_lrc.as_deref(),
            pkg.duration_s,
        );
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
                    self.status = "YouTube load failed".into();
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
            match show.seek_relative(delta) {
                Ok(p) => {
                    self.status = format!("Seek → {}", Self::fmt_time(p));
                    if self.duration_s > 0.0 {
                        self.scrub = (p / self.duration_s) as f32;
                    }
                }
                Err(e) => self.error_popup = Some(e.to_string()),
            }
        } else {
            self.error_popup = Some("Nothing is playing yet.".into());
        }
    }

    fn prev_song(&mut self) {
        if self.queue.index <= 0 {
            if let Some(show) = &self.show {
                let _ = show.seek(0.0);
                self.status = "Restarted track".into();
            } else {
                self.status = "No previous song".into();
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
        if !self.queue.can_next() {
            self.status = "No next song — load another YouTube URL".into();
            return;
        }
        self.queue.index += 1;
        if let Some(pkg) = self.queue.current().cloned() {
            let name = pkg.display_name();
            self.apply_package(pkg, true);
            self.status = format!("Next: {name}");
        }
    }
}

impl eframe::App for SongLightsGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_load();

        // Live karaoke + scrub
        if let Some(show) = &self.show {
            show.set_offset(self.offset as f64);
            show.set_volume(self.volume);
            let pos = show.position_s();
            let dur = if self.duration_s > 0.0 {
                self.duration_s
            } else {
                show.duration_s()
            };
            if !self.scrubbing && dur > 0.0 {
                self.scrub = (pos / dur).clamp(0.0, 1.0) as f32;
            }
            self.active_line = line_index_for_time(&self.timed_lines, pos, self.offset as f64);
            if show.is_running() || show.position_s() > 0.0 {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }

        if let Some(err) = self.error_popup.clone() {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button("OK").clicked() {
                        self.error_popup = None;
                    }
                });
        }

        egui::TopBottomPanel::bottom("status")
            .exact_height(32.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let dot = if self.chroma_ok { ACCENT } else { DANGER };
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 5.0, dot);
                    ui.label(RichText::new(&self.status).color(DIM).size(13.0));
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.heading(RichText::new("RAZER SONG LIGHTS").color(ACCENT).strong());
            ui.label(
                RichText::new("YouTube or paste lyrics → keyboard lights + karaoke display")
                    .color(DIM)
                    .size(13.0),
            );
            ui.add_space(10.0);

            // ── Search card ──
            egui::Frame::none()
                .fill(PANEL)
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("YouTube").color(ACCENT).strong());
                        let te = ui.add_sized(
                            [ui.available_width() - 120.0, 28.0],
                            egui::TextEdit::singleline(&mut self.url)
                                .hint_text("https://www.youtube.com/watch?v=…"),
                        );
                        if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.load_youtube();
                        }
                        let load = ui.add_enabled(
                            !self.busy,
                            egui::Button::new(RichText::new("Load + Play").strong())
                                .fill(ACCENT)
                                .min_size(Vec2::new(110.0, 28.0)),
                        );
                        if load.clicked() {
                            self.load_youtube();
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.play_audio, "Play audio");
                        ui.checkbox(&mut self.clock_sync, "Sync to audio");
                        ui.checkbox(&mut self.cache_music, "Cache music");
                        ui.checkbox(&mut self.lights, "Chroma lights");
                        ui.checkbox(&mut self.loop_play, "Loop");
                        ui.add_space(12.0);
                        ui.label(RichText::new("Vol").color(DIM));
                        ui.add(
                            egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false),
                        );
                        ui.label(format!("{:.0}%", self.volume * 100.0));
                    });
                    if self.busy {
                        ui.add_space(4.0);
                        ui.spinner();
                        ui.label(RichText::new("Loading…").color(DIM));
                    }
                });

            ui.add_space(10.0);

            // ── Player card ──
            egui::Frame::none()
                .fill(PANEL)
                .rounding(6.0)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("NOW PLAYING").color(DIM).small().strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let pos = self
                                .show
                                .as_ref()
                                .map(|s| s.position_s())
                                .unwrap_or(0.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    Self::fmt_time(pos),
                                    Self::fmt_time(self.duration_s)
                                ))
                                .color(ACCENT)
                                .strong()
                                .monospace(),
                            );
                        });
                    });
                    ui.label(RichText::new(&self.song_label).size(16.0).strong());

                    ui.add_space(6.0);
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
                            let target = self.scrub as f64 * self.duration_s.max(0.01);
                            let _ = show.seek(target);
                        }
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center)
                                .with_main_align(egui::Align::Center)
                                .with_main_justify(true),
                            |ui| {
                                if ui.button("⏮  Prev").clicked() {
                                    self.prev_song();
                                }
                                if ui.button("⏪  −10s").clicked() {
                                    self.seek_rel(-10.0);
                                }
                                let play = ui.add(
                                    egui::Button::new(RichText::new("▶  Play").strong())
                                        .fill(ACCENT)
                                        .min_size(Vec2::new(90.0, 32.0)),
                                );
                                if play.clicked() {
                                    self.start_current();
                                }
                                if ui.button("+10s  ⏩").clicked() {
                                    self.seek_rel(10.0);
                                }
                                if ui.button("Next  ⏭").clicked() {
                                    self.next_song();
                                }
                                let stop = ui.add(
                                    egui::Button::new(RichText::new("■  Stop").strong())
                                        .fill(Color32::from_rgb(0x5a, 0x1a, 0x1a)),
                                );
                                if stop.clicked() {
                                    self.stop_show();
                                }
                            },
                        );
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Library").color(DIM).small().strong());
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
                            self.status = "Sample loaded — press Play".into();
                        }
                        if ui.small_button("Open file…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Lyrics", &["txt", "lrc"])
                                .add_filter("All", &["*"])
                                .pick_file()
                            {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if path
                                        .extension()
                                        .map(|e| e == "lrc")
                                        .unwrap_or(false)
                                        || content.contains('[')
                                    {
                                        self.synced_lrc = Some(content.clone());
                                        self.lyrics_edit =
                                            crate::lyrics_match::clean_lyrics(&content);
                                    } else {
                                        self.synced_lrc = None;
                                        self.lyrics_edit = content;
                                    }
                                    self.audio_path = None;
                                    self.song_label = path
                                        .file_name()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "file".into());
                                    self.timed_lines = build_timed_lines(
                                        &self.lyrics_edit,
                                        self.synced_lrc.as_deref(),
                                        self.duration_s,
                                    );
                                    self.status = format!("Loaded {}", self.song_label);
                                }
                            }
                        }
                        if ui.small_button("Open URL").clicked() {
                            if !self.url.trim().is_empty() {
                                let _ = open::that(self.url.trim());
                            }
                        }
                        if ui.small_button("Clear").clicked() {
                            self.stop_show();
                            self.lyrics_edit.clear();
                            self.url.clear();
                            self.synced_lrc = None;
                            self.audio_path = None;
                            self.timed_lines.clear();
                            self.song_label = "No song loaded".into();
                            self.status = "Cleared".into();
                        }
                        if ui.small_button("Replay").clicked() {
                            self.start_current();
                        }
                    });
                });

            ui.add_space(10.0);

            // ── Lyrics ──
            ui.horizontal(|ui| {
                ui.label(RichText::new("LYRICS").color(ACCENT).strong());
                ui.label(
                    RichText::new("scrolls with audio · edit anytime")
                        .color(DIM)
                        .small(),
                );
            });
            egui::Frame::none()
                .fill(Color32::from_rgb(0x0f, 0x0f, 0x13))
                .rounding(4.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    let h = (ui.available_height() - 120.0).max(180.0);
                    ScrollArea::vertical()
                        .max_height(h)
                        .auto_shrink([false; 2])
                        .stick_to_bottom(false)
                        .show(ui, |ui| {
                            if !self.timed_lines.is_empty() && self.show.is_some() {
                                for (i, ln) in self.timed_lines.iter().enumerate() {
                                    let (color, size, strong) =
                                        if i as isize == self.active_line {
                                            (ACCENT, 18.0, true)
                                        } else if (i as isize) < self.active_line {
                                            (PAST, 14.0, false)
                                        } else {
                                            (DIM, 15.0, false)
                                        };
                                    let mut rt = RichText::new(&ln.text).color(color).size(size);
                                    if strong {
                                        rt = rt.strong();
                                    }
                                    ui.label(rt);
                                }
                            } else {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.lyrics_edit)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(12)
                                        .font(egui::TextStyle::Body),
                                );
                            }
                        });
                });

            ui.add_space(8.0);

            // ── Settings ──
            egui::Frame::none()
                .fill(PANEL)
                .rounding(6.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("SETTINGS").color(ACCENT).strong());
                    ui.add_space(6.0);
                    ui.columns(2, |cols| {
                        cols[0].vertical(|ui| {
                            ui.label(RichText::new("Sync offset").color(DIM).strong());
                            ui.label(
                                RichText::new("Lyrics early → slide right to delay lights")
                                    .color(DIM)
                                    .small(),
                            );
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Slider::new(&mut self.offset, -10.0..=20.0)
                                        .suffix("s"),
                                );
                                if ui.small_button("−1s").clicked() {
                                    self.offset = (self.offset - 1.0).clamp(-10.0, 20.0);
                                }
                                if ui.small_button("+1s").clicked() {
                                    self.offset = (self.offset + 1.0).clamp(-10.0, 20.0);
                                }
                                if ui.small_button("+2s").clicked() {
                                    self.offset = (self.offset + 2.0).clamp(-10.0, 20.0);
                                }
                            });
                        });
                        cols[1].vertical(|ui| {
                            ui.label(RichText::new("Mode").color(DIM).strong());
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.mode,
                                    PlayMode::FlashWord,
                                    "Whole word",
                                );
                                ui.selectable_value(
                                    &mut self.mode,
                                    PlayMode::FlashLine,
                                    "Whole line",
                                );
                            });
                            ui.label(
                                RichText::new(format!(
                                    "History: {} song(s)  ·  Chroma: {}",
                                    self.queue.songs.len(),
                                    if self.chroma_ok { "ready" } else { "N/A" }
                                ))
                                .color(DIM)
                                .small(),
                            );
                        });
                    });
                });
        });
    }
}
