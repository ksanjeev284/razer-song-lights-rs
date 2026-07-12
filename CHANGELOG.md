# Changelog

## [0.2.0] — 2026-07-12

### Full app features
- **YouTube play**: meta + audio download via `yt-dlp`, lyrics via lrclib
- **Audio**: rodio playback with seek / position clock
- **Chroma lights**: Windows `RzChromaSDK64.dll` word/line flash
- **Show engine**: audio-clock sync, karaoke console line, offset
- **CLI**: `play`, `play-file`, `demo`, plus existing tools
- Song package cache for instant re-play

### Tests
- 27 offline unit/integration tests
- Live 10-song QA still **10/10 PASS**

---

## [0.1.0] — 2026-07-12

### Added
- Core library: LRC parse, title parse, lyrics match, lrclib client, sync sim, cache, key map
- CLI: `lyrics`, `parse-lrc`, `parse-title`, `qa10`
- Unit tests + offline integration + 10-song live accuracy suite
- CI for Windows / Linux / macOS
