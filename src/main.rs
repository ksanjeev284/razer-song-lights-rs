//! Razer Song Lights (Rust) — full-featured CLI.

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
use razer_song_lights_rs::title::{
    clean_track_name, is_youtube_url, names_similar, split_title,
};
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
    about = "Razer Song Lights (Rust) — YouTube lyrics, audio, Chroma keyboard light show"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Clone, Debug, ValueEnum)]
enum ModeArg {
    FlashWord,
    FlashLine,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fetch synced lyrics for an artist / track
    Lyrics {
        artist: String,
        track: String,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
        #[arg(long)]
        lrc: bool,
    },
    /// Parse an LRC file and print timing summary
    ParseLrc {
        path: PathBuf,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
    },
    /// Parse a YouTube-style title into track + artist
    ParseTitle {
        title: String,
        #[arg(long, default_value = "")]
        artist_hint: String,
    },
    /// Load a YouTube URL → lyrics + audio + Chroma light show
    Play {
        /// YouTube URL, or artist name if --track is set
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
    /// Play a local audio file with LRC or plain lyrics file
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
    /// Run 10-song live accuracy + timing/sync QA
    Qa10 {
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Demo light flash (no YouTube)
    Demo {
        #[arg(long)]
        no_lights: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
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
            let text = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            let lines = parse_lrc_lines(&text);
            if lines.is_empty() {
                bail!("no timed lines in {}", path.display());
            }
            let q = lrc_quality_score(&text, duration);
            let last = lines.last().unwrap().t;
            let cov = if duration > 0.0 {
                last / duration
            } else {
                0.0
            };
            println!("lines: {}", lines.len());
            println!(
                "first: {:.2}s  last: {:.2}s  coverage: {:.0}%  quality: {:.1}",
                lines[0].t,
                last,
                cov * 100.0,
                q
            );
            for (i, ln) in lines.iter().take(8).enumerate() {
                println!("  [{i}] {:6.2}s  {}", ln.t, ln.text);
            }
            if lines.len() > 8 {
                println!("  … {} more", lines.len() - 8);
            }
            simulate_sync(&lines, 0.0).map_err(|e| anyhow::anyhow!(e))?;
            println!("sync simulation: OK");
        }
        Commands::ParseTitle { title, artist_hint } => {
            let (track, artist) = split_title(&title, &artist_hint);
            let track = clean_track_name(&track);
            println!("track:  {track}");
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
        } => {
            play_command(
                &url_or_artist,
                track.as_deref(),
                duration,
                volume,
                offset,
                mode,
                no_audio,
                no_lights,
                no_cache,
            )?;
        }
        Commands::PlayFile {
            audio,
            lrc,
            lyrics,
            duration,
            volume,
            offset,
            mode,
            no_lights,
        } => {
            play_file(
                &audio,
                lrc.as_deref(),
                lyrics.as_deref(),
                duration,
                volume,
                offset,
                mode,
                no_lights,
            )?;
        }
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
            };
            let sample = "hello world\nlights on stage\nsing with me";
            run_show(sample, None, 12.0, None, &cfg, stop)?;
        }
    }
    Ok(())
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

    let (artist, track_name, duration_s, url, video_id, hit, audio_path) =
        if is_youtube_url(url_or_artist) {
            println!("Fetching YouTube metadata…");
            let meta = extract_meta(url_or_artist)?;
            println!(
                "Resolved: {} — {} ({:.0}s)",
                meta.artist, meta.track, meta.duration_s
            );

            // Cache package
            if !no_cache {
                if let Some(pkg) = load_song_package(&cache_root, &meta.video_id) {
                    println!("Loaded lyrics from cache: {}", pkg.display_name());
                    let audio = if !no_audio {
                        if let Some(p) = &pkg.audio_path {
                            if PathBuf::from(p).is_file() {
                                Some(PathBuf::from(p))
                            } else {
                                println!("Downloading audio…");
                                Some(download_audio(&meta.url, &meta.video_id, &audio_dir)?)
                            }
                        } else {
                            println!("Downloading audio…");
                            Some(download_audio(&meta.url, &meta.video_id, &audio_dir)?)
                        }
                    } else {
                        None
                    };
                    let hit = razer_song_lights_rs::LyricsHit {
                        plain: pkg.lyrics.clone(),
                        synced_lrc: pkg.synced_lrc.clone(),
                        source: format!("{} (cache)", pkg.source),
                    };
                    (
                        pkg.artist,
                        pkg.track,
                        pkg.duration_s,
                        pkg.url,
                        pkg.video_id,
                        hit,
                        audio,
                    )
                } else {
                    println!("Searching lyrics…");
                    let hit = fetch_lyrics(&meta.artist, &meta.track, meta.duration_s)
                        .with_context(|| "lyrics fetch")?;
                    println!(
                        "Lyrics: {} ({})",
                        hit.source,
                        if hit.is_synced() {
                            "synced LRC"
                        } else {
                            "plain"
                        }
                    );
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
                    if !no_cache {
                        let _ = save_song_package(&cache_root, &pkg);
                    }
                    (
                        meta.artist,
                        meta.track,
                        meta.duration_s,
                        meta.url,
                        meta.video_id,
                        hit,
                        audio,
                    )
                }
            } else {
                println!("Searching lyrics…");
                let hit = fetch_lyrics(&meta.artist, &meta.track, meta.duration_s)?;
                let audio = if !no_audio {
                    println!("Downloading audio…");
                    Some(download_audio(&meta.url, &meta.video_id, &audio_dir)?)
                } else {
                    None
                };
                (
                    meta.artist,
                    meta.track,
                    meta.duration_s,
                    meta.url,
                    meta.video_id,
                    hit,
                    audio,
                )
            }
        } else {
            // artist + --track
            let track = track.ok_or_else(|| {
                anyhow::anyhow!("pass a YouTube URL, or artist with --track \"Song Name\"")
            })?;
            let artist = url_or_artist.to_string();
            println!("Searching lyrics for {artist} — {track}…");
            let hit = fetch_lyrics(&artist, track, duration)?;
            (
                artist,
                track.to_string(),
                duration,
                String::new(),
                String::new(),
                hit,
                None,
            )
        };

    let _ = (url, video_id);
    println!("Now playing: {artist} — {track_name}");
    let cfg = ShowConfig {
        mode: mode_from(mode),
        volume,
        sync_offset_s: offset,
        play_audio: !no_audio && audio_path.is_some(),
        lights: !no_lights,
        karaoke_console: true,
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
    let mut synced: Option<String> = None;
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
    let dur = if duration > 0.0 {
        duration
    } else {
        240.0
    };
    let cfg = ShowConfig {
        mode: mode_from(mode),
        volume,
        sync_offset_s: offset,
        play_audio: true,
        lights: !no_lights,
        karaoke_console: true,
    };
    run_show(
        &plain,
        synced.as_deref(),
        dur,
        Some(audio),
        &cfg,
        stop,
    )?;
    Ok(())
}

fn run_qa10(json_path: Option<PathBuf>) -> Result<()> {
    println!("========================================================================");
    println!("10-SONG LYRICS ACCURACY + TIMING/SYNC QA (Rust)");
    println!("========================================================================");

    let mut reports = Vec::new();
    for (i, song) in SONG_CATALOG.iter().enumerate() {
        print!("[{}/10] {} — {} … ", i + 1, song.artist, song.track);
        let t0 = Instant::now();
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        let mut title_ok = true;
        for title in song.youtube_titles {
            let (track, artist) = split_title(title, song.artist);
            let track = clean_track_name(&track);
            if !names_similar(&track, song.track) {
                title_ok = false;
                errors.push(format!("title parse {title:?} → {track}"));
            }
            if names_similar(&track, &artist) {
                title_ok = false;
                errors.push(format!("track≈artist for {title:?}"));
            }
        }

        let hit = match fetch_lyrics(song.artist, song.track, song.duration_s) {
            Ok(h) => h,
            Err(e) => {
                errors.push(format!("fetch: {e}"));
                println!("FAIL");
                reports.push(serde_json::json!({
                    "id": song.id, "ok": false, "errors": errors,
                }));
                continue;
            }
        };

        let mut identity_ok = true;
        for p in song.must_contain {
            if !contains_phrase(&hit.plain, p) {
                identity_ok = false;
                errors.push(format!("missing phrase: {p}"));
            }
        }
        for p in song.must_not_contain {
            if contains_phrase(&hit.plain, p) {
                identity_ok = false;
                errors.push(format!("wrong-song phrase: {p}"));
            }
        }

        let mut timing_ok = false;
        let mut sync_ok = false;
        let mut timed_lines = 0usize;
        let mut coverage = 0.0;

        if let Some(lrc) = &hit.synced_lrc {
            let lines = parse_lrc_lines(lrc);
            timed_lines = lines.len();
            if !lines.is_empty() {
                coverage = lines.last().unwrap().t / song.duration_s;
                if timed_lines >= song.min_timed_lines && coverage >= song.min_coverage {
                    timing_ok = true;
                } else {
                    errors.push(format!(
                        "timing lines={timed_lines} coverage={:.0}%",
                        coverage * 100.0
                    ));
                }
                match simulate_sync(&lines, 0.0) {
                    Ok(_) => sync_ok = true,
                    Err(e) => errors.push(format!("sync sim: {e}")),
                }
            }
        } else {
            warnings.push("no synced LRC".into());
            errors.push("missing synced LRC".into());
        }

        let ok = title_ok && identity_ok && timing_ok && sync_ok;
        println!(
            "{}  identity={identity_ok} timing={timing_ok} sync={sync_ok} lines={timed_lines} src={}",
            if ok { "PASS" } else { "FAIL" },
            hit.source
        );
        reports.push(serde_json::json!({
            "id": song.id,
            "artist": song.artist,
            "track": song.track,
            "ok": ok,
            "title_parse_ok": title_ok,
            "identity_ok": identity_ok,
            "timing_ok": timing_ok,
            "sync_sim_ok": sync_ok,
            "source": hit.source,
            "timed_lines": timed_lines,
            "coverage": coverage,
            "errors": errors,
            "warnings": warnings,
            "elapsed_s": t0.elapsed().as_secs_f64(),
        }));
    }

    let passed = reports
        .iter()
        .filter(|r| r["ok"].as_bool() == Some(true))
        .count();
    println!();
    println!("Result: {passed}/{} songs fully PASS", reports.len());
    if let Some(path) = json_path {
        fs::write(&path, serde_json::to_string_pretty(&reports)?)?;
        println!("JSON → {}", path.display());
    }
    if passed < 7 {
        bail!("QA failed: only {passed}/10 passed");
    }
    Ok(())
}
