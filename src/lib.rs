//! Razer Song Lights — full Rust implementation.
//!
//! - LRC timing, title parse, lyrics identity, lrclib
//! - YouTube meta + audio via yt-dlp
//! - Local audio playback with seek (rodio)
//! - Razer Chroma keyboard lights (Windows SDK)
//! - Synced light show + karaoke console
//! - 10-song accuracy QA

pub mod audio;
pub mod cache;
pub mod chroma;
pub mod color;
pub mod gui;
pub mod history;
pub mod key_map;
pub mod settings;
pub mod lrc;
pub mod lrclib;
pub mod lyrics_match;
pub mod show;
pub mod song_catalog;
pub mod sync_sim;
pub mod title;
pub mod youtube;

pub use cache::{load_song_package, save_song_package, SongPackage};
pub use color::{rgb, unpack_rgb};
pub use lrc::{parse_lrc_lines, TimedLine, TimedWord};
pub use lrclib::fetch_lyrics;
pub use lyrics_match::{clean_lyrics, hit_matches_track, LyricsHit};
pub use title::{clean_track_name, is_youtube_url, names_similar, split_title, video_id_from_url};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
