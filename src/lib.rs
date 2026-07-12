//! Razer Song Lights — Rust core library.
//!
//! Ports the logic that keeps lyrics **correct** and **timed**:
//! - LRC parse + timing quality
//! - YouTube-style title parse (`Song - Artist` / `Artist - Song`)
//! - Lyrics identity checks
//! - lrclib.net client for synced lyrics
//! - Clock-sync line selection
//! - Song package cache helpers
//!
//! Full keyboard Chroma control remains Windows-native (see Python app);
//! this crate focuses on portable, well-tested sync logic + CLI tooling.

pub mod cache;
pub mod color;
pub mod key_map;
pub mod lrc;
pub mod lrclib;
pub mod lyrics_match;
pub mod song_catalog;
pub mod sync_sim;
pub mod title;

pub use cache::{load_song_package, save_song_package, SongPackage};
pub use color::{rgb, unpack_rgb};
pub use lrc::{parse_lrc_lines, TimedLine, TimedWord};
pub use lrclib::fetch_lyrics;
pub use lyrics_match::{clean_lyrics, hit_matches_track, LyricsHit};
pub use title::{clean_track_name, is_youtube_url, names_similar, split_title, video_id_from_url};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

