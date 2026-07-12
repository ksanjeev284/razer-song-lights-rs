# razer-song-lights-rs

**Full Razer Song Lights app in Rust** — GUI + CLI + Chroma lights.

**v0.5.0** — more player features (themes, karaoke fullscreen, sleep timer, cache tools, …)

https://github.com/ksanjeev284/razer-song-lights-rs

---

## Run

```bash
cargo run --release
```

Needs **yt-dlp**, **ffmpeg** (recommended), **Razer Synapse** (Windows lights).

---

## Features

| Area | Features |
|------|----------|
| **Playback** | Play / Pause / Stop / Mute / Speed 0.5–1.5× / Scrub / ±10s |
| **Queue** | Prev / Next / History / Favorites / Shuffle / Repeat Off·One·All / Auto-next |
| **YouTube** | Load + Play, cache, recent URLs dropdown |
| **Karaoke** | Scrolling lyrics, filter, click-line-to-seek, F11 fullscreen, export LRC |
| **Lights** | Word/Line modes, 8 color themes, brightness, live updates |
| **Extras** | Sleep timer 15/30/60m, always-on-top, mini player, clear cache, saved prefs |
| **Shortcuts** | Space · ←/→ · M · Ctrl+F · Ctrl+O · F11 |

### Light themes
Rainbow · Razer Green · Fire · Ice · Purple · Gold · Pink · White

---

## Tests

```bash
cargo test   # 38 offline tests
```

---

## License

MIT © 2026 ksanjeev284
