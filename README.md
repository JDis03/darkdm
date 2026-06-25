# DarkDM

> **Better than yt-dlp.** Multi-threaded download manager for Linux with Chrome extension, native Rust engine, and Tauri GUI.

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Status:** ✅ Native CLI ready · 🔜 Tauri GUI in progress

---

## Quick Start

```bash
# Download anything
darkdm descargar "https://example.com/file.zip"

# YouTube, MediaFire, any site
darkdm descargar "https://www.youtube.com/watch?v=..."
darkdm descargar "https://www.mediafire.com/file/XXXX/video.rar"

# Multi-threaded (8 workers)
darkdm descargar "https://cdn.example.com/large.iso" --threads 8

# Probe without downloading
darkdm info "https://example.com/file.zip"

# View logs
darkdm logs
darkdm logs --follow
```

---

## Features

### ✅ Production Ready

- **Multi-threaded downloads** — Dynamic piece-splitting (XDM algorithm), 8 workers default
- **Smart resume** — Crash-safe state with atomic writes, resume from any interruption
- **Site extractors** — YouTube (yt-dlp), MediaFire, generic HTML analyzer
- **HLS/DASH support** — Automatic ffmpeg integration for streaming protocols
- **Progress tracking** — ILoveCandy Pac-Man progress bar
- **Logging system** — Structured logs (console + rotating files), `darkdm logs` command
- **Auto-rename** — Never overwrites files (`file.mp4` → `file (1).mp4`)
- **Disk space check** — Fails fast if insufficient space

### 🔜 Coming Soon

- **Tauri GUI** — Desktop app with real-time progress
- **Queue manager** — Multiple concurrent downloads
- **Browser integration** — Chrome extension auto-capture

---

## Installation

### Prerequisites

```bash
# Arch Linux
sudo pacman -S rust cargo ffmpeg

# Ubuntu/Debian
sudo apt install rustc cargo ffmpeg

# YouTube support (optional)
pip install yt-dlp
```

### Build from source

```bash
git clone https://github.com/JDis03/darkdm.git
cd darkdm
./init.sh

# Build CLI
cd src-tauri
cargo build --release --bin darkdm

# Install
sudo cp target/release/darkdm /usr/local/bin/
```

### Verify

```bash
darkdm --help
darkdm info "https://httpbin.org/bytes/1024"
```

---

## Usage

### Basic Downloads

```bash
# Download any file
darkdm descargar "https://example.com/file.zip"

# Custom output directory
darkdm descargar "https://example.com/file.zip" --output ~/Downloads

# More workers (faster for large files)
darkdm descargar "https://example.com/file.zip" --threads 16

# Disable resume
darkdm descargar "https://example.com/file.zip" --no-resume
```

### Site-Specific

```bash
# YouTube (uses yt-dlp)
darkdm descargar "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# MediaFire (auto-extracts direct link)
darkdm descargar "https://www.mediafire.com/file/XXXX/file.rar"

# HLS streams (uses ffmpeg)
darkdm descargar "https://cdn.example.com/stream.m3u8"
```

### Debugging

```bash
# Verbose logging (DEBUG level)
darkdm descargar "https://example.com/file.zip" --verbose

# Custom log level
RUST_LOG=trace darkdm descargar "https://example.com/file.zip"

# View logs
darkdm logs -n 50
darkdm logs --follow

# Probe URL (no download)
darkdm info "https://example.com/file.zip"
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      darkdm CLI                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. Probe URL                                               │
│     ├─ HEAD request → size, resumable, content-type        │
│     └─ Detect HTML → trigger extraction                    │
│                                                             │
│  2. Extract (if needed)                                     │
│     ├─ YouTube → yt-dlp (HLS manifest)                      │
│     ├─ MediaFire → scrape HTML                             │
│     └─ Generic → <video>, <audio>, <a href> tags           │
│                                                             │
│  3. Download                                                │
│     ├─ Multi-threaded (8 workers, dynamic piece-split)     │
│     ├─ Range requests (HTTP 206 Partial Content)           │
│     ├─ TransactedIO (crash-safe state, atomic writes)      │
│     ├─ Progress bar (ILoveCandy Pac-Man)                   │
│     └─ Auto-resume on interruption                         │
│                                                             │
│  4. Post-process                                            │
│     ├─ HLS/DASH → ffmpeg merge                             │
│     └─ Auto-rename if file exists                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
                           ↓
              ~/Descargas/DarkDM/file.mp4
```

### Key Algorithms (ported from XDM)

- **Dynamic piece-splitting** — Work-stealing, not static N-parts
- **TransactedIO** — 3-file rotation, atomic `rename(2)` for crash safety
- **ContinueAdjacentPiece** — Reuse TCP connections between pieces
- **Accept-Encoding: identity** — Critical for accurate Range calculations

---

## Documentation

- **[Architecture Spec](openspec/changes/native-cli/design.md)** — 2700+ lines, 8 design patterns, XDM algorithms
- **[Logging Guide](docs/LOGGING.md)** — Structured logging, debugging, log rotation

---

## Project Status

| Feature | Status | Notes |
|---------|--------|-------|
| Multi-threaded download | ✅ Done | 8 workers, dynamic piece-splitting |
| Resume support | ✅ Done | Crash-safe TransactedIO |
| Site extractors | ✅ Done | YouTube, MediaFire, generic HTML |
| HLS/DASH support | ✅ Done | ffmpeg integration |
| Progress bar | ✅ Done | ILoveCandy Pac-Man |
| Logging system | ✅ Done | tracing + rotating files |
| CLI interface | ✅ Done | clap, descargar/info/logs |
| **Tauri GUI** | 🔜 Next | Desktop app with real-time progress |
| **Queue manager** | 🔜 Next | Multiple concurrent downloads |
| **Browser extension** | 🔜 Next | Auto-capture from Chrome |

**Tests:** 38/38 passing · **Build:** `./init.sh` passes

---

## Tech Stack

- **Rust** — Core engine (reqwest, tokio, async-trait)
- **tracing** — Structured logging with file rotation
- **clap** — CLI argument parsing
- **scraper** — HTML parsing for site extractors
- **yt-dlp** — YouTube extraction (external)
- **ffmpeg** — HLS/DASH merging (external)
- **Tauri** — Desktop GUI (coming soon)

---

## Contributing

1. Fork the repo
2. Create a feature branch (`git checkout -b feat/amazing`)
3. Run tests (`cargo test --lib`)
4. Commit (`git commit -m 'feat: add amazing feature'`)
5. Push (`git push origin feat/amazing`)
6. Open a Pull Request

**Development:**
```bash
# Run tests
cargo test --lib

# Build CLI
cargo build --release --bin darkdm

# Verify
./init.sh
```

---

## License

MIT — See [LICENSE](LICENSE) for details.

---

## Acknowledgments

- **[XDM](https://github.com/subhra74/xdm)** — Algorithms for dynamic piece-splitting and crash-safe state
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)** — YouTube extraction backend

---

<div align="center">
  <sub>Built with ❤️ for Linux power users</sub>
</div>
