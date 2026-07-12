# Changelog

## [0.10.0] — unreleased

### Features
- **Fade-in** on play start
- **Auto skip intro** to first lyric
- **Hide past lyrics** in list
- **Local audio open** (mp3/m4a/wav/…)
- **Song notes + ★ rating** (per video id)
- **Queue ▲/▼** reorder current
- **Random favorite** + export favorites M3U
- **Demo catalog** → YouTube search
- **Chroma hardware test** sweep
- **Search history** dropdown
- **Time remaining** on status bar

### Notes
- Not published as a GitHub release yet (local / master only)

---

## [0.9.0] — 2026-07-13

### Features
- **YouTube search** via yt-dlp (`ytsearchN:`) with click-to-play results
- **Lyric line progress bar** (0–100% through current line)
- **Replay line** (`R` + button)
- **Seek ±30s** (Shift+arrows + buttons)
- **Auto night dim** (22:00–07:00 local)
- **Mirror ambient FX** (left↔right)
- **Queue shuffle** + move current **to top**
- **Export listen stats CSV**

### Tests
- 50 offline unit/integration tests

---

## [0.8.5] — 2026-07-12

### Fixes
- **YouTube “Please sign in”**: yt-dlp cookie support
  - GUI: **YT cookies** browser (chrome/edge/firefox/…) + cookies.txt picker
  - Auto-try browser cookies + `player_client=android,web`
  - Env: `YTDLP_COOKIES`, `YTDLP_COOKIES_FROM_BROWSER`
  - Default file: cache `youtube_cookies.txt`
  - Clearer error text when bot-check still fails

## [0.8.4] — 2026-07-12

### Fixes
- **Release builds**: refresh `Cargo.lock` so `cargo build --locked` succeeds on CI
- Release workflow falls back without `--locked` and publishes whatever platforms succeed

## [0.8.3] — 2026-07-12

### Fixes
- **macOS compile**: audio worker thread owns rodio `OutputStream` (not `Send` on CoreAudio); public `AudioPlayer` is fully `Send`/`Sync`
- Windows + Ubuntu + macOS CI and release binaries should all go green

## [0.8.2] — 2026-07-12

### Fixes
- Fade/crossfade deferred drop stays on the GUI thread
- Remaining Clippy nits for CI green on Win/Linux

## [0.8.1] — 2026-07-12

### CI / Release
- Fix Clippy `-D warnings` failures that broke CI on all platforms
- Install Linux audio/GUI system packages in CI
- **Multi-platform release binaries** on tag push:
  - `razer-song-lights-windows-x64.exe`
  - `razer-song-lights-linux-x64`
  - `razer-song-lights-macos-arm64` (Apple Silicon)
  - `razer-song-lights-macos-x64` (Intel)

---

## [0.8.0] — 2026-07-12

### Features
- **Named playlists** — save/load queue lists in Library panel
- **Ambient FX** — Pulse, Wave, Breath, Ripple, Off (live switchable)
- **Snap to lyric** (`S`) — jump to nearest timed line
- **Jump %** — keys `1`–`9` (10%–90%) + 25/50/75% buttons
- **Offset nudge** — `[` / `]` and UI ±0.1s
- **Top played** ranking from per-song play counts
- **Sleep fade** — volume ramp before sleep stop
- **Paste URL** from clipboard
- **Export timed LRC** from current timed lines
- **Clear queue** from Library

### From 0.7 (shipped together)
- Per-song **resume position** + **remember offset**
- Ambient keyboard pulse + **night dim**
- Soft **crossfade next** (fade on track change)
- M3U import/export, history filter, remove from queue
- LRC quality badge, next-line countdown, F1 help, volume presets

### Tests
- 45 offline unit/integration tests

---

## [0.6.0] — 2026-07-12

### Features
- **A-B loop** (engine-enforced, live set/clear)
- **Skip intro** to first lyric timestamp
- **Bookmarks** with jump buttons
- **Fade-out on stop** (optional)
- **Drag-drop** YouTube URL / LRC / TXT
- **Prefetch** next track audio ~35s before end
- **Listen stats** (plays, hours, last song)
- **Lyric font size** slider
- **Export plain TXT** + share clipboard
- Shortcuts: **N/P**, **I**, **Ctrl+B**
- Replay **−5s** button

### Tests
- 40 offline unit/integration tests

---

## [0.5.0] — themes, F11 karaoke, sleep timer, shuffle/repeat
## [0.4.0] — pause/mute/speed, favorites, optimizations
## [0.3.0] — full GUI
## [0.2.0] — CLI play
## [0.1.0] — core library
