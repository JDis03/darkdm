# DarkDM

> IDM for Linux. Native Rust download manager with a Chrome extension and Tauri GUI.

**Status: In development.** Working scripts today, native CLI in progress.

---

## What it does

Pass any URL and DarkDM handles the rest — finds the real download link, downloads in parallel, resumes on interruption, and extracts archives.

```bash
darkdm descargar "https://www.mediafire.com/file/XXXX/video.rar/file" --password "abc"
darkdm descargar "https://www.youtube.com/watch?v=..."
darkdm descargar "https://cdn.example.com/video.mp4" --threads 8
darkdm info     "https://www.mediafire.com/file/XXXX/video.rar/file"
```

---

## Working today

| Component | What it does |
|-----------|-------------|
| `darkdm` CLI | Native Rust download manager with multi-threading, resume, ILoveCandy progress bar |
| `darkdm-mediafire` | Downloads from MediaFire — extracts link, downloads, extracts RAR/ZIP with password |
| `darkdm-host` | HTTP server on `:8765` — bridge between Chrome extension and the download engine |
| `darkdm-cli` | Downloads HLS streams (`.m3u8`) via ffmpeg |
| Chrome extension | Detects streams on any site, shows download overlay on `<video>` elements |
| `darkdm-capture` | TLS packet capture via `SSLKEYLOGFILE` + tcpdump |
| Tauri app | Desktop GUI that lists files in `~/Descargas/DarkDM/` |

---

## Install

```bash
git clone https://github.com/JDis03/darkdm.git && cd darkdm
./init.sh

# Scripts
cp scripts/darkdm-mediafire ~/.local/bin/

# Native host (for Chrome extension)
cd native-host && cargo build --release
cp target/release/darkdm-host ~/.local/bin/

# Chrome extension
# vivaldi://extensions → Load unpacked → select extension/
```

**Dependencies:**
```bash
sudo pacman -S curl unrar p7zip ffmpeg
pip install yt-dlp   # optional, for YouTube
```

---

## Usage

```bash
# MediaFire — auto-extracts direct link + downloads + extracts RAR
darkdm-mediafire "https://www.mediafire.com/file/XXXX/file.rar/file"
darkdm-mediafire "https://..." --password "pass"
darkdm-mediafire "https://..." --get-link   # print direct URL, no download

# HLS stream
darkdm-cli "https://cdn.example.com/stream.m3u8"

# Watch downloads
watch ~/scripts/watch-downloads.sh
```

---

## How it works

```
Browser Extension
  ↓  captures URL + headers from any site
darkdm engine
  ↓  resolves → extracts real link → downloads → extracts
  ├── Plugin registry: MediaFire, YouTube (yt-dlp), generic page analyzer
  ├── Dynamic piece-splitting: starts with 1 piece, splits the largest as workers finish
  ├── Range headers: parallel segments, auto-resume on interruption
  ├── Crash-safe state: TransactedIO (3-file rotation, atomic rename)
  └── Pipeline: Resolve → Extract → Download → Post-process
~/Descargas/DarkDM/
```

---

## Spec

Full architecture spec at [`openspec/changes/native-cli/design.md`](openspec/changes/native-cli/design.md) (2700+ lines).

Key algorithms ported from [XDM](https://github.com/subhra74/xdm):
- **Dynamic piece-splitting** — work-stealing, not static N-parts
- **TransactedIO** — crash-safe state with atomic `rename(2)`
- **ContinueAdjacentPiece** — reuse TCP connections between adjacent pieces
- **ProbeResult** — clean probe on first request (size, resumable, filename)
- `Accept-Encoding: identity` on all engine requests (critical for Range accuracy)

---

## Roadmap

```
✅ Bash scripts + Chrome extension
✅ Full spec (2700+ lines, 8 patterns, 12 XDM algorithms)
✅ Rust engine — reqwest, dynamic piece-splitting, TransactedIO
✅ CLI — clap, darkdm descargar/info, ILoveCandy progress bar
🔜 Multi-threaded download loop (currently single worker)
🔜 Plugins — MediaFire, YouTube, generic page analyzer
🔜 Tauri — shared engine, real-time progress events
```

---

## License

MIT
