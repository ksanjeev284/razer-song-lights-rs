# razer-song-lights-rs

[![CI](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)

**Full Razer Song Lights stack in Rust** — YouTube → lyrics + audio → **synced Chroma keyboard lights** + karaoke console.

Companion to the [Python GUI app](https://github.com/ksanjeev284/razer-song-lights).

**Current version: 0.2.0**

---

## Features (all working)

| Feature | Status |
|--------|--------|
| YouTube metadata (`yt-dlp`) | ✅ |
| Audio download + cache | ✅ |
| Synced LRC lyrics (lrclib) | ✅ |
| Correct title parse (`Song - Artist` / `Artist - Song`) | ✅ |
| Local audio play + seek (rodio) | ✅ |
| Razer Chroma keyboard lights (Windows SDK) | ✅ |
| Word / line flash modes | ✅ |
| Karaoke console line display | ✅ |
| Sync offset | ✅ |
| Song package cache | ✅ |
| 10-song accuracy + timing QA | ✅ **10/10** |
| Desktop GUI (Tk) | ❌ use Python app |
| Prev/Next history playlist UI | 🔜 CLI-focused |

---

## Requirements

- **Rust 1.74+**
- **yt-dlp** on PATH (`pip install -U yt-dlp`) for YouTube
- **ffmpeg** recommended for MP3 convert
- **Windows** + **Razer Synapse** for keyboard lights  
  (Linux/macOS: lyrics + audio + QA still work; lights skip gracefully)

---

## Install & run

```bash
git clone https://github.com/ksanjeev284/razer-song-lights-rs.git
cd razer-song-lights-rs
cargo build --release
```

### Play a YouTube song (lights + audio + lyrics)

```bash
cargo run --release -- play "https://www.youtube.com/watch?v=VIDEO_ID"

# options
cargo run --release -- play "URL" --volume 0.9 --offset 1.5 --mode flash-line
cargo run --release -- play "URL" --no-audio          # lights only
cargo run --release -- play "URL" --no-lights         # audio + karaoke text
```

### Play local audio + LRC

```bash
cargo run --release -- play-file track.mp3 --lrc song.lrc --duration 290
```

### Lyrics / tools

```bash
cargo run -- lyrics "Cigarettes After Sex" "Apocalypse" --duration 290
cargo run -- parse-title "Apocalypse - Cigarettes After Sex" --artist-hint "Cigarettes After Sex"
cargo run -- parse-lrc song.lrc --duration 290
cargo run -- demo
cargo run --release -- qa10
```

---

## Testing

```bash
# Offline unit + integration
cargo test

# Live 10-song identity + LRC timing (network)
cargo run --release -- qa10
cargo test --test integration_ten_songs -- --ignored --nocapture
```

Latest live QA: **10/10 PASS** (same catalog as Python: Apocalypse, Blinding Lights, bad guy, Shape of You, Hello, Bohemian Rhapsody, Never Gonna Give You Up, Believer, Levitating, Viva La Vida).

CI: Windows / Ubuntu / macOS (`cargo test`, clippy, fmt).

---

## Architecture

```text
YouTube URL
   │
   ├─ yt-dlp meta ──► artist / track / duration
   ├─ lrclib ────────► plain + LRC timestamps
   └─ yt-dlp -x ─────► cached audio file
                          │
                          ▼
              AudioPlayer (rodio, seekable)
              position_s ──► sync clock
                          │
              ┌───────────┴───────────┐
              ▼                       ▼
     ChromaKeyboard (Win DLL)    Karaoke console
     word/line flash             current LRC line
```

---

## Python vs Rust

| | Python | Rust |
|--|--------|------|
| GUI | ✅ Tkinter | CLI (+ demo) |
| Chroma lights | ✅ | ✅ Windows |
| YouTube + LRC | ✅ | ✅ |
| 10-song QA | ✅ | ✅ |
| Portable binary | PyInstaller | `cargo build --release` |

---

## License

[MIT](LICENSE) © 2026 [ksanjeev284](https://github.com/ksanjeev284)
