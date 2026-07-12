//! Integration: title parse regressions (offline).

use razer_song_lights_rs::song_catalog::SONG_CATALOG;
use razer_song_lights_rs::title::{clean_track_name, names_similar, split_title};

#[test]
fn all_catalog_youtube_titles_parse() {
    for song in SONG_CATALOG {
        for title in song.youtube_titles {
            let (track, artist) = split_title(title, song.artist);
            let track = clean_track_name(&track);
            assert!(
                names_similar(&track, song.track),
                "{}: {title:?} → track={track:?} artist={artist:?}",
                song.id
            );
            assert!(
                !names_similar(&track, &artist),
                "{}: track≈artist for {title:?}",
                song.id
            );
        }
    }
}

