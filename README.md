# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — GUI + CLI + Chroma lights.

**v0.9.0** — YouTube search, lyric progress bar, queue tools, auto night dim, mirror FX.

https://github.com/ksanjeev284/razer-song-lights-rs

### Download

Pre-built binaries on the [Releases](https://github.com/ksanjeev284/razer-song-lights-rs/releases) page:

| Platform | Asset |
|----------|--------|
| Windows x64 | [`razer-song-lights-windows-x64.exe`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.9.0/razer-song-lights-windows-x64.exe) |
| Linux x64 | [`razer-song-lights-linux-x64`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.9.0/razer-song-lights-linux-x64) |
| macOS Apple Silicon | [`razer-song-lights-macos-arm64`](https://github.com/ksanjeev284/razer-song-lights-rs/releases/download/v0.9.0/razer-song-lights-macos-arm64) |

```bash
cargo run --release
```

**Notes:** Razer Chroma keyboard lights require Windows + Razer Synapse. Use **YT cookies → chrome/edge** if YouTube says “Please sign in”. Keep `yt-dlp` updated: `pip install -U yt-dlp`.

---

## Highlights (v0.9)

| Feature | |
|--------|--|
| **YouTube search** | Type a query → Find → click result to play |
| **Lyric progress bar** | Fill % through the current line |
| **Replay line** | `R` or button — jump to start of current lyric |
| **Seek ±30s** | Shift+←/→ or −30s/+30s buttons |
| **Auto night dim** | Dim lights 22:00–07:00 local |
| **Mirror FX** | Flip ambient light direction |
| **Shuffle queue / ↑ Top** | Library queue tools |
| **Export stats CSV** | Listen stats download |

Plus cookies fix (v0.8.5), playlists, ambient FX, A-B loop, karaoke F11, etc.

### Shortcuts

| Key | Action |
|-----|--------|
| Space | Pause / resume |
| ← → | Seek ±10s |
| Shift+←/→ | Seek ±30s |
| N / P | Next / prev |
| R | Replay current lyric line |
| S | Snap to nearest lyric |
| I | Skip intro |
| M | Mute |
| 1–9 | Jump to 10%–90% |
| [ / ] | Offset −0.1s / +0.1s |
| Ctrl+B | Bookmark |
| Ctrl+F | Favorite |
| F11 | Fullscreen karaoke |
| F1 | Help |

---

## Tests

```bash
cargo test   # 50 offline tests
```

---

MIT © 2026 ksanjeev284
