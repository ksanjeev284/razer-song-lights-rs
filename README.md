# razer-song-lights-rs

[![CI](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)

**Full Razer Song Lights app in Rust** — desktop GUI + CLI.

YouTube → lyrics + audio → **synced Chroma keyboard lights** + karaoke display.

**Version 0.3.0** — all major features implemented.

Companion: [Python app](https://github.com/ksanjeev284/razer-song-lights)

---

## Features

| Feature | Status |
|--------|--------|
| **Desktop GUI** (egui, dark Razer theme) | ✅ |
| YouTube Load + Play | ✅ |
| Audio play / stop / ±10s seek / scrub | ✅ |
| Prev / Next play history | ✅ |
| Karaoke scrolling lyrics | ✅ |
| Cache played music | ✅ |
| Sync offset + word/line modes | ✅ |
| Chroma keyboard lights (Windows) | ✅ |
| CLI play / play-file / qa10 / demo | ✅ |
| 10-song accuracy QA | ✅ 10/10 |

---

## Quick start

```bash
git clone https://github.com/ksanjeev284/razer-song-lights-rs.git
cd razer-song-lights-rs
cargo run --release
```

Opens the **GUI** by default.

Needs:

- **yt-dlp** on PATH (`pip install -U yt-dlp`)
- **ffmpeg** (recommended)
- **Windows + Razer Synapse** for keyboard lights

### GUI flow

1. Paste YouTube URL → **Load + Play**  
2. Lyrics scroll with audio; keyboard flashes words  
3. Use **Prev / −10s / Play / +10s / Next / Stop**  
4. Tune **Offset** if lights are early/late  

### CLI

```bash
cargo run --release -- play "https://www.youtube.com/watch?v=VIDEO_ID"
cargo run --release -- play-file track.mp3 --lrc song.lrc
cargo run --release -- qa10
cargo run --release -- demo
cargo run --release -- gui          # explicit GUI
cargo run --release -- --console lyrics "Adele" "Hello" --duration 295
```

---

## Testing

```bash
cargo test
cargo run --release -- qa10
```

Offline: **31 tests passed**. Live 10-song QA: **10/10 PASS**.

---

## License

[MIT](LICENSE) © 2026 [ksanjeev284](https://github.com/ksanjeev284)
