# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — GUI + CLI + Chroma lights.

**v0.6.0** — A-B loop, bookmarks, skip intro, fade stop, drag-drop, stats, prefetch, more shortcuts.

https://github.com/ksanjeev284/razer-song-lights-rs

```bash
cargo run --release
```

---

## Highlights (v0.6)

| Feature | |
|--------|--|
| **A-B loop** | Click A⟷B twice to set loop range; third clears |
| **Skip intro** | Jump to first LRC line (`I`) |
| **Bookmarks** | Ctrl+B / 📌, click to jump back |
| **Fade stop** | Smooth volume fade on stop |
| **Drag & drop** | Drop YouTube URL or `.lrc`/`.txt` onto window |
| **Prefetch next** | Downloads next song audio near track end |
| **Listen stats** | Songs played + hours listened |
| **Lyric font size** | Adjustable |
| **Export TXT / Share** | Plain lyrics + share clipboard |
| **N / P** | Next / previous song |

Plus all earlier features: themes, brightness, F11 karaoke, shuffle/repeat, sleep timer, favorites, cache tools, shortcuts, etc.

### Shortcuts

| Key | Action |
|-----|--------|
| Space | Pause / resume |
| ← → | Seek ±10s |
| N / P | Next / prev |
| I | Skip intro |
| M | Mute |
| Ctrl+B | Bookmark |
| Ctrl+F | Favorite |
| Ctrl+O | Open file |
| F11 | Fullscreen karaoke |

---

## Tests

```bash
cargo test   # 40 offline tests
```

---

MIT © 2026 ksanjeev284
