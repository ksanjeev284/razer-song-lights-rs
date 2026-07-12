# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — GUI + CLI + Chroma lights.

**v0.8.4** — Multi-platform binaries (Windows / macOS / Linux). CI + release lockfile fixed.

https://github.com/ksanjeev284/razer-song-lights-rs

### Download

Pre-built binaries on the [Releases](https://github.com/ksanjeev284/razer-song-lights-rs/releases) page:

| Platform | Asset |
|----------|--------|
| Windows x64 | [`razer-song-lights-windows-x64.exe`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.8.4/razer-song-lights-windows-x64.exe) |
| Linux x64 | [`razer-song-lights-linux-x64`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.8.4/razer-song-lights-linux-x64) |
| macOS Apple Silicon | [`razer-song-lights-macos-arm64`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.8.4/razer-song-lights-macos-arm64) |

```bash
# Or build from source
cargo run --release
```

**Notes:** Razer Chroma keyboard lights require Windows + Razer Synapse. Audio/lyrics work on all platforms. Linux needs `yt-dlp` on `PATH` for YouTube.

---

## Highlights (v0.8)

| Feature | |
|--------|--|
| **Named playlists** | Save / load queue as named lists in Library |
| **Ambient FX** | Pulse · Wave · Breath · Ripple · Off |
| **Snap to lyric** | `S` jumps to nearest timed line |
| **Jump %** | Keys `1`–`9` → 10%–90%; buttons 25/50/75% |
| **Offset nudge** | `[` / `]` ±0.1s for fine sync |
| **Top played** | Ranked by per-song play count |
| **Sleep fade** | Volume ramps down before sleep stop |
| **Paste URL** | One-click clipboard paste into YouTube field |
| **Export timed LRC** | Write current timed lines (synced or estimated) |
| **Clear queue** | Empty history/queue from Library |

Plus v0.7: resume position, remember offset, ambient pulse, night dim, crossfade next, M3U import/export, LRC quality badge, next-line countdown, F1 help, volume presets.

Plus v0.6: A-B loop, bookmarks, skip intro, fade stop, drag-drop, stats, prefetch, etc.

### Shortcuts

| Key | Action |
|-----|--------|
| Space | Pause / resume |
| ← → | Seek ±10s |
| N / P | Next / prev |
| I | Skip intro |
| M | Mute |
| S | Snap to nearest lyric |
| 1–9 | Jump to 10%–90% |
| [ / ] | Offset −0.1s / +0.1s |
| Ctrl+B | Bookmark |
| Ctrl+F | Favorite |
| Ctrl+O | Open file |
| F11 | Fullscreen karaoke |
| F1 | Help |

---

## Tests

```bash
cargo test   # 45 offline tests
```

---

MIT © 2026 ksanjeev284
