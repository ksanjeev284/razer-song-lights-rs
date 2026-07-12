//! Live 10-song accuracy + timing (network).
//!
//!   cargo test --test integration_ten_songs -- --ignored --nocapture

use razer_song_lights_rs::lrc::{lrc_quality_score, parse_lrc_lines};
use razer_song_lights_rs::lrclib::fetch_lyrics;
use razer_song_lights_rs::lyrics_match::contains_phrase;
use razer_song_lights_rs::song_catalog::SONG_CATALOG;
use razer_song_lights_rs::sync_sim::simulate_sync;
use razer_song_lights_rs::title::{clean_track_name, names_similar, split_title};

#[test]
#[ignore = "network — run with --ignored"]
fn ten_songs_identity_and_sync() {
    let mut passed = 0;
    for song in SONG_CATALOG {
        eprintln!(">>> {} — {}", song.artist, song.track);

        for title in song.youtube_titles {
            let (track, _) = split_title(title, song.artist);
            let track = clean_track_name(&track);
            assert!(
                names_similar(&track, song.track),
                "title parse failed: {title}"
            );
        }

        let hit = fetch_lyrics(song.artist, song.track, song.duration_s)
            .unwrap_or_else(|e| panic!("fetch {}: {e}", song.id));

        for p in song.must_contain {
            assert!(
                contains_phrase(&hit.plain, p),
                "{} missing {p:?} preview={:?}",
                song.id,
                hit.plain.lines().next()
            );
        }
        for p in song.must_not_contain {
            assert!(
                !contains_phrase(&hit.plain, p),
                "{} has wrong-song phrase {p:?}",
                song.id
            );
        }

        let lrc = hit
            .synced_lrc
            .as_deref()
            .expect("synced LRC required for timing QA");
        let lines = parse_lrc_lines(lrc);
        assert!(
            lines.len() >= song.min_timed_lines,
            "{} lines {} < {}",
            song.id,
            lines.len(),
            song.min_timed_lines
        );
        let coverage = lines.last().unwrap().t / song.duration_s;
        assert!(
            coverage >= song.min_coverage,
            "{} coverage {coverage:.2} < {}",
            song.id,
            song.min_coverage
        );
        simulate_sync(&lines, 0.0).expect("sync sim");
        let q = lrc_quality_score(lrc, song.duration_s);
        eprintln!(
            "    OK lines={} coverage={:.0}% quality={q:.1} src={}",
            lines.len(),
            coverage * 100.0,
            hit.source
        );
        passed += 1;
    }
    assert_eq!(passed, 10);
}

