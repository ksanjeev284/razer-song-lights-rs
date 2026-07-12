//! Razer Song Lights (Rust) — CLI for lyrics fetch, LRC parse, and 10-song QA.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use razer_song_lights_rs::lrc::{lrc_quality_score, parse_lrc_lines};
use razer_song_lights_rs::lrclib::fetch_lyrics;
use razer_song_lights_rs::lyrics_match::contains_phrase;
use razer_song_lights_rs::song_catalog::SONG_CATALOG;
use razer_song_lights_rs::sync_sim::simulate_sync;
use razer_song_lights_rs::title::{clean_track_name, names_similar, split_title};
use razer_song_lights_rs::VERSION;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "razer-song-lights",
    version = VERSION,
    about = "Rust core for Razer Song Lights — LRC timing, title parse, lyrics identity, lrclib"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fetch synced lyrics for an artist / track
    Lyrics {
        artist: String,
        track: String,
        #[arg(long, default_value_t = 0.0)]
        duration: f64,
        /// Print raw LRC instead of plain text
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
    /// Run 10-song live accuracy + timing/sync QA
    Qa10 {
        #[arg(long)]
        json: Option<PathBuf>,
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
            println!("first: {:.2}s  last: {:.2}s  coverage: {:.0}%  quality: {:.1}", lines[0].t, last, cov * 100.0, q);
            for (i, ln) in lines.iter().take(8).enumerate() {
                println!("  [{i}] {:6.2}s  {}", ln.t, ln.text);
            }
            if lines.len() > 8 {
                println!("  … {} more", lines.len() - 8);
            }
            if let Err(e) = simulate_sync(&lines, 0.0) {
                bail!("sync simulation failed: {e}");
            }
            println!("sync simulation: OK");
        }
        Commands::ParseTitle { title, artist_hint } => {
            let (track, artist) = split_title(&title, &artist_hint);
            let track = clean_track_name(&track);
            println!("track:  {track}");
            println!("artist: {artist}");
        }
        Commands::Qa10 { json } => {
            run_qa10(json)?;
        }
    }
    Ok(())
}

fn run_qa10(json_path: Option<PathBuf>) -> Result<()> {
    println!("========================================================================");
    println!("10-SONG LYRICS ACCURACY + TIMING/SYNC QA (Rust)");
    println!("========================================================================");

    let mut reports = Vec::new();
    for (i, song) in SONG_CATALOG.iter().enumerate() {
        print!(
            "[{}/10] {} — {} … ",
            i + 1,
            song.artist,
            song.track
        );
        let t0 = Instant::now();
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // Title parse
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

        // Fetch
        let hit = match fetch_lyrics(song.artist, song.track, song.duration_s) {
            Ok(h) => h,
            Err(e) => {
                errors.push(format!("fetch: {e}"));
                println!("FAIL");
                reports.push(serde_json::json!({
                    "id": song.id,
                    "ok": false,
                    "errors": errors,
                }));
                continue;
            }
        };

        // Identity
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

        // Timing
        let mut timing_ok = false;
        let mut sync_ok = false;
        let mut timed_lines = 0usize;
        let mut first_t = -1.0;
        let mut last_t = -1.0;
        let mut coverage = 0.0;
        let mut quality = -1.0;

        if let Some(lrc) = &hit.synced_lrc {
            let lines = parse_lrc_lines(lrc);
            timed_lines = lines.len();
            quality = lrc_quality_score(lrc, song.duration_s);
            if !lines.is_empty() {
                first_t = lines[0].t;
                last_t = lines.last().unwrap().t;
                coverage = last_t / song.duration_s;
                for i in 0..lines.len().saturating_sub(1) {
                    if lines[i].t > lines[i + 1].t + 0.001 {
                        errors.push(format!("non-monotonic at {i}"));
                    }
                }
                if timed_lines < song.min_timed_lines {
                    errors.push(format!(
                        "too few lines: {timed_lines} < {}",
                        song.min_timed_lines
                    ));
                }
                if coverage < song.min_coverage {
                    errors.push(format!(
                        "weak coverage: {:.0}% < {:.0}%",
                        coverage * 100.0,
                        song.min_coverage * 100.0
                    ));
                }
                timing_ok = timed_lines >= song.min_timed_lines
                    && coverage >= song.min_coverage
                    && errors.iter().all(|e| !e.starts_with("non-monotonic") && !e.starts_with("too few") && !e.starts_with("weak"));
                match simulate_sync(&lines, 0.0) {
                    Ok(_) => sync_ok = true,
                    Err(e) => errors.push(format!("sync sim: {e}")),
                }
            } else {
                errors.push("empty LRC parse".into());
            }
        } else {
            warnings.push("no synced LRC".into());
            errors.push("missing synced LRC".into());
        }

        let ok = title_ok && identity_ok && timing_ok && sync_ok && errors.iter().all(|e| {
            !e.starts_with("missing phrase")
                && !e.starts_with("wrong-song")
                && !e.starts_with("title parse")
                && !e.starts_with("too few")
                && !e.starts_with("weak coverage")
                && !e.starts_with("sync sim")
                && !e.starts_with("missing synced")
                && !e.starts_with("non-monotonic")
        });

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
            "first_t": first_t,
            "last_t": last_t,
            "coverage": coverage,
            "quality": quality,
            "errors": errors,
            "warnings": warnings,
            "preview": hit.plain.lines().next().unwrap_or(""),
            "elapsed_s": t0.elapsed().as_secs_f64(),
        }));
    }

    let passed = reports.iter().filter(|r| r["ok"].as_bool() == Some(true)).count();
    println!();
    println!("Result: {passed}/{} songs fully PASS", reports.len());
    println!("========================================================================");

    if let Some(path) = json_path {
        fs::write(&path, serde_json::to_string_pretty(&reports)?)?;
        println!("JSON → {}", path.display());
    }

    if passed < 7 {
        bail!("QA failed: only {passed}/10 passed");
    }
    Ok(())
}

