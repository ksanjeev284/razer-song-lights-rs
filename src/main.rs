//! Razer Song Lights (Rust) — GUI by default + full CLI.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use razer_song_lights_rs::cache::{
    default_cache_root, load_song_package, save_song_package, SongPackage,
};
use razer_song_lights_rs::lrc::{lrc_quality_score, parse_lrc_lines};
use razer_song_lights_rs::lrclib::fetch_lyrics;
use razer_song_lights_rs::lyrics_match::contains_phrase;
use razer_song_lights_rs::show::{run_show, PlayMode, ShowConfig};
use razer_song_lights_rs::song_catalog::SONG_CATALOG;
use razer_song_lights_rs::sync_sim::simulate_sync;
use razer_song_lights_rs::title::{clean_track_name, is_youtube_url, names_similar, split_title};
use razer_song_lights_rs::youtube::{download_audio, extract_meta};
use razer_song_lights_rs::VERSION;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "razer-song-lights",
    version = VERSION,
    about = "Razer Song Lights (Rust) — GUI, YouTube, audio, Chroma lights, karaoke"
)]
struct Cli {
    /// Force console CLI (default opens GUI when no subcommand)
    #[arg(long)]
    console: bool,

    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(Clone, Debug, ValueEnum)]
enum ModeArg {
    FlashWord,
    FlashLine,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Open the desktop GUI (default)
    Gui,
    /// Fetch synced lyrics for an artist / track
    Lyrics {
        artist: String,
        track: String,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
        #[arg(long)]
        lrc: bool,
    },
    ParseLrc {
        path: PathBuf,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
    },
    ParseTitle {
        title: String,
        #[arg(long, default_value = "")]
        artist_hint: String,
    },
    /// Load a YouTube URL → lyrics + audio + light show (CLI)
    Play {
        url_or_artist: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
        #[arg(long, default_value_t = 0.85)]
        volume: f32,
        #[arg(long, default_value_t = 0.0)]
        offset: f64,
        #[arg(long, value_enum, default_value_t = ModeArg::FlashWord)]
        mode: ModeArg,
        #[arg(long)]
        no_audio: bool,
        #[arg(long)]
        no_lights: bool,
        #[arg(long)]
        no_cache: bool,
    },
    PlayFile {
        audio: PathBuf,
        #[arg(long)]
        lrc: Option<PathBuf>,
        #[arg(long)]
        lyrics: Option<PathBuf>,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
        #[arg(long, default_value_t = 0.85)]
        volume: f32,
        #[arg(long, default_value_t = 0.0)]
        offset: f64,
        #[arg(long, value_enum, default_value_t = ModeArg::FlashWord)]
        mode: ModeArg,
        #[arg(long)]
        no_lights: bool,
    },
    Qa10 {
        #[arg(long)]
        json: Option<PathBuf>,
    },
    Demo {
        #[arg(long)]
        no_lights: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Default → GUI
    if cli.cmd.is_none() && !cli.console {
        return launch_gui();
    }

    match cli.cmd.unwrap_or(Commands::Gui) {
        Commands::Gui => launch_gui()?,
        Commands::Lyrics {
            artist,
            track,
            duration,
            lrc,
        } => {
            let hit = fetch_lyrics(&artist, &track, duration)
                .with_context(|| format!("lyrics for {artist} — {track}"))?;
            println!("source: {}", hit.source);
            println!("synced: {}", hit.is_synced());
            if lrc {
                println!("{}", hit.synced_lrc.as_deref().unwrap_or("(none)"));
            } else {
                println!("{}", hit.plain);
            }
        }
        Commands::ParseLrc { path, duration } => {
            let text = fs::read_to_string(&path)?;
            let lines = parse_lrc_lines(&text);
            if lines.is_empty() {
                bail!("no timed lines");
            }
            let q = lrc_quality_score(&text, duration);
            let last = lines.last().unwrap().t;
            let cov = if duration > 0.0 {
                last / duration
            } else {
                0.0
            };
            println!(
                "lines={} first={:.1}s last={:.1}s coverage={:.0}% quality={:.1}",
                lines.len(),
                lines[0].t,
                last,
                cov * 100.0,
                q
            );
            simulate_sync(&lines, 0.0).map_err(|e| anyhow::anyhow!(e))?;
        }
        Commands::ParseTitle { title, artist_hint } => {
            let (track, artist) = split_title(&title, &artist_hint);
            println!("track:  {}", clean_track_name(&track));
            println!("artist: {artist}");
        }
        Commands::Play {
            url_or_artist,
            track,
            duration,
            volume,
            offset,
            mode,
            no_audio,
            no_lights,
            no_cache,
        } => play_command(
            &url_or_artist,
            track.as_deref(),
            duration,
            volume,
            offset,
            mode,
            no_audio,
            no_lights,
            no_cache,
        )?,
        Commands::PlayFile {
            audio,
            lrc,
            lyrics,
            duration,
            volume,
            offset,
            mode,
            no_lights,
        } => play_file(
            &audio,
            lrc.as_deref(),
            lyrics.as_deref(),
            duration,
            volume,
            offset,
            mode,
            no_lights,
        )?,
        Commands::Qa10 { json } => run_qa10(json)?,
        Commands::Demo { no_lights } => {
            let stop = install_ctrlc();
            let cfg = ShowConfig {
                mode: PlayMode::FlashWord,
                volume: 0.0,
                play_audio: false,
                lights: !no_lights,
                karaoke_console: true,
                sync_offset_s: 0.0,
                loop_play: false,
                ..ShowConfig::default()
            };
            run_show(
                "hello world\nlights on stage\nsing with me",
                None,
                12.0,
                None,
                &cfg,
                stop,
            )?;
        }
    }
    Ok(())
}

fn launch_gui() -> Result<()> {
    razer_song_lights_rs::gui::run_gui().map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}

fn install_ctrlc() -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let _ = ctrlc::set_handler(move || {
        s.store(true, Ordering::SeqCst);
    });
    stop
}

fn mode_from(m: ModeArg) -> PlayMode {
    match m {
        ModeArg::FlashWord => PlayMode::FlashWord,
        ModeArg::FlashLine => PlayMode::FlashLine,
    }
}

#[allow(clippy::too_many_arguments)]
fn play_command(
    url_or_artist: &str,
    track: Option<&str>,
    duration: f64,
    volume: f32,
    offset: f64,
    mode: ModeArg,
    no_audio: bool,
    no_lights: bool,
    no_cache: bool,
) -> Result<()> {
    let stop = install_ctrlc();
    let cache_root = default_cache_root();
    let audio_dir = cache_root.join("audio");

    let (artist, track_name, duration_s, hit, audio_path) = if is_youtube_url(url_or_artist) {
        println!("Fetching YouTube metadata…");
        let meta = extract_meta(url_or_artist)?;
        println!(
            "Resolved: {} — {} ({:.0}s)",
            meta.artist, meta.track, meta.duration_s
        );

        if !no_cache {
            if let Some(mut pkg) = load_song_package(&cache_root, &meta.video_id) {
                println!("Loaded from cache: {}", pkg.display_name());
                if !no_audio
                    && pkg
                        .audio_path
                        .as_ref()
                        .map(|p| !PathBuf::from(p).is_file())
                        .unwrap_or(true)
                {
                    println!("Downloading audio…");
                    if let Ok(p) = download_audio(&meta.url, &meta.video_id, &audio_dir) {
                        pkg.audio_path = Some(p.display().to_string());
                    }
                }
                let hit = razer_song_lights_rs::LyricsHit {
                    plain: pkg.lyrics.clone(),
                    synced_lrc: pkg.synced_lrc.clone(),
                    source: format!("{} (cache)", pkg.source),
                };
                (
                    pkg.artist,
                    pkg.track,
                    pkg.duration_s,
                    hit,
                    pkg.audio_path.map(PathBuf::from),
                )
            } else {
                println!("Searching lyrics…");
                let hit = fetch_lyrics(&meta.artist, &meta.track, meta.duration_s)?;
                println!("Lyrics: {}", hit.source);
                let audio = if !no_audio {
                    println!("Downloading audio…");
                    Some(download_audio(&meta.url, &meta.video_id, &audio_dir)?)
                } else {
                    None
                };
                let mut pkg = SongPackage::from_hit(
                    &meta.artist,
                    &meta.track,
                    meta.duration_s,
                    &hit,
                    &meta.video_id,
                    &meta.url,
                );
                if let Some(ref p) = audio {
                    pkg.audio_path = Some(p.display().to_string());
                }
                let _ = save_song_package(&cache_root, &pkg);
                (meta.artist, meta.track, meta.duration_s, hit, audio)
            }
        } else {
            let hit = fetch_lyrics(&meta.artist, &meta.track, meta.duration_s)?;
            let audio = if !no_audio {
                Some(download_audio(&meta.url, &meta.video_id, &audio_dir)?)
            } else {
                None
            };
            (meta.artist, meta.track, meta.duration_s, hit, audio)
        }
    } else {
        let track = track.ok_or_else(|| {
            anyhow::anyhow!("pass a YouTube URL, or artist with --track \"Song\"")
        })?;
        let hit = fetch_lyrics(url_or_artist, track, duration)?;
        (
            url_or_artist.to_string(),
            track.to_string(),
            duration,
            hit,
            None,
        )
    };

    println!("Now playing: {artist} — {track_name}");
    let cfg = ShowConfig {
        mode: mode_from(mode),
        volume,
        sync_offset_s: offset,
        play_audio: !no_audio && audio_path.is_some(),
        lights: !no_lights,
        karaoke_console: true,
        loop_play: false,
        ..ShowConfig::default()
    };
    run_show(
        &hit.plain,
        hit.synced_lrc.as_deref(),
        duration_s,
        audio_path.as_deref(),
        &cfg,
        stop,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn play_file(
    audio: &std::path::Path,
    lrc: Option<&std::path::Path>,
    lyrics: Option<&std::path::Path>,
    duration: f64,
    volume: f32,
    offset: f64,
    mode: ModeArg,
    no_lights: bool,
) -> Result<()> {
    let stop = install_ctrlc();
    let mut synced = None;
    let mut plain = String::new();
    if let Some(p) = lrc {
        let text = fs::read_to_string(p)?;
        synced = Some(text.clone());
        plain = razer_song_lights_rs::clean_lyrics(&text);
    }
    if let Some(p) = lyrics {
        plain = fs::read_to_string(p)?;
    }
    if plain.is_empty() {
        bail!("provide --lrc and/or --lyrics");
    }
    let cfg = ShowConfig {
        mode: mode_from(mode),
        volume,
        sync_offset_s: offset,
        play_audio: true,
        lights: !no_lights,
        karaoke_console: true,
        loop_play: false,
        ..ShowConfig::default()
    };
    run_show(
        &plain,
        synced.as_deref(),
        if duration > 0.0 { duration } else { 240.0 },
        Some(audio),
        &cfg,
        stop,
    )?;
    Ok(())
}

fn run_qa10(json_path: Option<PathBuf>) -> Result<()> {
    println!("10-SONG QA (Rust)");
    let mut reports = Vec::new();
    for (i, song) in SONG_CATALOG.iter().enumerate() {
        print!("[{}/10] {} — {} … ", i + 1, song.artist, song.track);
        let t0 = Instant::now();
        let mut errors = Vec::new();
        let mut title_ok = true;
        for title in song.youtube_titles {
            let (track, artist) = split_title(title, song.artist);
            let track = clean_track_name(&track);
            if !names_similar(&track, song.track) || names_similar(&track, &artist) {
                title_ok = false;
                errors.push(format!("title {title}"));
            }
        }
        let hit = match fetch_lyrics(song.artist, song.track, song.duration_s) {
            Ok(h) => h,
            Err(e) => {
                println!("FAIL");
                reports.push(serde_json::json!({"id": song.id, "ok": false, "err": e.to_string()}));
                continue;
            }
        };
        let mut identity_ok = true;
        for p in song.must_contain {
            if !contains_phrase(&hit.plain, p) {
                identity_ok = false;
                errors.push(format!("missing {p}"));
            }
        }
        for p in song.must_not_contain {
            if contains_phrase(&hit.plain, p) {
                identity_ok = false;
                errors.push(format!("wrong {p}"));
            }
        }
        let mut timing_ok = false;
        let mut sync_ok = false;
        let mut timed_lines = 0;
        if let Some(lrc) = &hit.synced_lrc {
            let lines = parse_lrc_lines(lrc);
            timed_lines = lines.len();
            if !lines.is_empty() {
                let cov = lines.last().unwrap().t / song.duration_s;
                timing_ok = timed_lines >= song.min_timed_lines && cov >= song.min_coverage;
                sync_ok = simulate_sync(&lines, 0.0).is_ok();
            }
        }
        let ok = title_ok && identity_ok && timing_ok && sync_ok;
        println!(
            "{} lines={timed_lines} src={}",
            if ok { "PASS" } else { "FAIL" },
            hit.source
        );
        reports.push(serde_json::json!({
            "id": song.id, "ok": ok, "identity_ok": identity_ok,
            "timing_ok": timing_ok, "sync_ok": sync_ok,
            "errors": errors, "elapsed": t0.elapsed().as_secs_f64(),
        }));
    }
    let passed = reports.iter().filter(|r| r["ok"] == true).count();
    println!("Result: {passed}/10");
    if let Some(p) = json_path {
        fs::write(p, serde_json::to_string_pretty(&reports)?)?;
    }
    if passed < 7 {
        bail!("QA failed");
    }
    Ok(())
}
