# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — GUI + CLI + Chroma lights.

**v0.8.0** — Named playlists, ambient FX (wave/breath/ripple), snap seek, 1–9 jump %, top played, sleep fade.

https://github.com/ksanjeev284/razer-song-lights-rs

```bash
cargo run --release
```

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
