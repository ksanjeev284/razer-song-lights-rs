# Changelog

## [0.4.0] — 2026-07-12

### Features
- Pause / Resume, Mute, playback **speed** (0.5–1.5×)
- **Auto-next** when a track ends
- **Favorites** + Library side panel (history + stars)
- **Keyboard shortcuts** (Space, arrows, M, Ctrl+F, Ctrl+O)
- **Settings persistence** (volume, offset, mode, toggles)
- Lyric **filter** + copy current line
- Save prefs button

### Optimizations
- Binary search for active LRC line/word (`O(log n)`)
- Adaptive sleep in light engine (lower idle CPU)
- Atomic sync offset on hot path
- Pause freezes song clock correctly; speed scales position
- Engine ends cleanly for auto-next

### Tests
- 35 offline unit/integration tests

---

## [0.3.0] — GUI + full parity
## [0.2.0] — CLI play pipeline
## [0.1.0] — Core library
