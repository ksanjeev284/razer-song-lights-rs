# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — optimized GUI + CLI.

**v0.4.0** — more features, lower CPU, seek-safe binary-search sync.

Repo: https://github.com/ksanjeev284/razer-song-lights-rs

---

## Features

| Feature | |
|--------|--|
| Desktop GUI (dark Razer theme) | ✅ |
| YouTube Load + Play + cache | ✅ |
| Audio: play / pause / stop / mute / speed 0.5–1.5× | ✅ |
| Seek ±10s + scrub bar | ✅ |
| Prev / Next history + **Library panel** | ✅ |
| **Favorites** (Ctrl+F / ★) | ✅ |
| Karaoke scroll + lyric filter + copy | ✅ |
| Auto-next when track ends | ✅ |
| Settings saved between sessions | ✅ |
| Keyboard shortcuts | ✅ |
| Chroma lights (Windows) | ✅ |
| CLI + 10-song QA | ✅ |

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| **Space** | Pause / Resume (or Play) |
| **← / →** | Seek −10s / +10s |
| **M** | Mute toggle |
| **Ctrl+F** | Favorite current song |
| **Ctrl+O** | Open lyrics file |

---

## Run

```bash
cargo run --release
```

Requires: **yt-dlp**, **ffmpeg** (recommended), **Razer Synapse** on Windows for lights.

---

## Optimizations (0.4)

- **O(log n)** binary search for LRC line/word index (long songs)
- **Adaptive engine sleep** — lower CPU when idle between lyric events
- **Atomic offset** (no mutex on hot path)
- **Pause-aware** audio clock (no position drift)
- **Seek-safe** light index (works after scrub / reverse seek)
- Fewer redundant Chroma flashes (only on line/word change)

---

## Tests

```bash
cargo test          # 35 offline tests
cargo run -- qa10   # live 10-song accuracy
```

---

## License

MIT © 2026 ksanjeev284
