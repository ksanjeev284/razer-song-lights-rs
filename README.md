# razer-song-lights-rs

[![CI](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ksanjeev284/razer-song-lights-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)

**Rust core** for [Razer Song Lights](https://github.com/ksanjeev284/razer-song-lights) — correct lyrics identity, LRC timing, YouTube title parsing, lrclib client, and full unit/integration QA.

> Companion to the **Python + Chroma SDK** desktop app (Windows keyboard lights).  
> This crate is **portable** (Windows / Linux / macOS) for logic + lyrics + sync QA.

---

## Features

| Module | Purpose |
|--------|---------|
| `lrc` | Parse LRC timestamps, quality score, word expansion, offsets |
| `title` | `Song - Artist` / `Artist - Song` YouTube title parse, URL helpers |
| `lyrics_match` | Clean lyrics, reject wrong-song / track≈artist hits |
| `lrclib` | Live client for [lrclib.net](https://lrclib.net) synced lyrics |
| `sync_sim` | Active line index vs song time (karaoke clock) |
| `cache` | JSON song package cache with invalidation |
| `key_map` | Char → Chroma 6×22 grid (preview / future native binding) |
| `song_catalog` | 10 reference tracks for accuracy QA |

---

## Quick start

```bash
git clone https://github.com/ksanjeev284/razer-song-lights-rs.git
cd razer-song-lights-rs
cargo build --release
```

### CLI

```bash
# Fetch lyrics
cargo run -- lyrics "Cigarettes After Sex" "Apocalypse" --duration 290

# Print LRC
cargo run -- lyrics "Queen" "Bohemian Rhapsody" --duration 355 --lrc

# Parse YouTube title
cargo run -- parse-title "Apocalypse - Cigarettes After Sex" --artist-hint "Cigarettes After Sex"

# Parse local LRC file
cargo run -- parse-lrc song.lrc --duration 290

# 10-song live accuracy + timing QA
cargo run -- qa10
cargo run -- qa10 --json report.json
```

---

## Testing

```bash
# Unit + offline integration (no network required for most)
cargo test

# Live 10-song suite (network)
cargo test --test integration_ten_songs -- --ignored --nocapture
# or
cargo run --release -- qa10
```

### What the 10-song QA checks

For each of 10 popular tracks:

1. **Title parse** — both YouTube layouts  
2. **Identity** — required phrases present; wrong-song phrases absent  
3. **Timing** — LRC line count, duration coverage, monotonic stamps  
4. **Sync sim** — active line index advances correctly with song time  

Target: **10/10 PASS** on lrclib (same bar as the Python suite).

### CI

GitHub Actions runs `cargo test` + clippy + fmt on **Windows, Ubuntu, macOS**.

---

## Relation to Python app

| | Python (`razer-song-lights`) | Rust (`razer-song-lights-rs`) |
|--|------------------------------|--------------------------------|
| Chroma keyboard lights | ✅ Native Windows DLL | Preview key map only |
| GUI | ✅ Tkinter | — |
| YouTube audio download | ✅ yt-dlp | — |
| LRC + title + identity | ✅ | ✅ (tested) |
| 10-song live QA | ✅ | ✅ |
| Platforms | Windows (Chroma) | All (logic/CLI) |

---

## License

[MIT](LICENSE) © 2026 [ksanjeev284](https://github.com/ksanjeev284)
