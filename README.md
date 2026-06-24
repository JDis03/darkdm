# DarkDM — Modern Download Manager for Linux

DarkDM is a **download manager for Linux** inspired by the architecture of [Internet Download Manager (IDM)](https://www.internetdownloadmanager.com/) for Windows. It's being built as a native Rust application with a Tauri frontend, designed from the ground up to be fast, extensible, and reliable.

**⚠️ This project is in active development.** The core architecture is defined in a [detailed specification](openspec/changes/native-cli/). A working bash-based MediaFire downloader and a Chrome extension for HLS stream capture are available today.

---

## Philosophy

IDM on Windows sets the standard: a browser extension captures download URLs, passes them to a native download engine, and that engine handles the rest — multi-threaded downloads, resume support, queue management, and post-processing (archive extraction, video conversion).

DarkDM follows the same model on Linux:

```
Browser Extension              Native Engine (Rust)
     │                               │
     │  Captures URL + Headers       │
     ├──────────────────────────────►│
     │                               │
     │                               ├── Pipeline: Resolve → Extract → Download → Process
     │                               ├── Multi-threaded (Range headers, segmented)
     │                               ├── Auto-resume on interruption
     │                               ├── Plugin registry for site-specific extractors
     │                               └── Event system → terminal UI + GUI
     │                               │
     │                          ┌────┴────┐
     │                          │  Tauri  │
     │                          │  (Svelte│
     │                          │   GUI)  │
     │                          └─────────┘
```

The difference from IDM: DarkDM is **open-source**, **Rust-native** (no C++), and designed with **modern software patterns** — Pipeline, State Machine, Plugin Registry, Event System — making it maintainable and extensible.

---

## What It Does

### Today (working)

| Feature | How | Status |
|---------|-----|--------|
| **MediaFire downloads** | `darkdm-mediafire` (bash) — extracts direct link, downloads, extracts RAR/ZIP/7z | ✅ |
| **HLS stream capture** | Chrome extension detects `.m3u8` + HTTP server downloads via ffmpeg | ✅ |
| **Chrome extension** | MV3, detects streams via `webRequest`, shows overlay on `<video>` elements | ✅ |
| **TLS packet capture** | `darkdm-capture` (bash) — SSLKEYLOGFILE + tcpdump + pyshark | ✅ |
| **Tauri desktop app** | Lists downloaded files in `~/Descargas/DarkDM/` | ✅ |

### Tomorrow (spec'd, not yet implemented)

| Feature | How | Status |
|---------|-----|--------|
| **Unified CLI** | `darkdm descargar <url>` — one command for everything | 🔜 |
| **Multi-threaded downloads** | Segmented download with `Range` headers (like IDM) | 🔜 Spec'd |
| **Auto-resume** | Detect partial files, resume with `Range: bytes=X-` | 🔜 Spec'd |
| **YouTube & 1000+ sites** | Plugin system calling `yt-dlp` | 🔜 Spec'd |
| **Generic page analyzer** | Extract video/download links from any HTML page | 🔜 Spec'd |
| **Archive extraction** | RAR/ZIP/7z/tar.gz/tar.xz — password support | 🔜 Spec'd |
| **Download queue** | Concurrent downloads with priority | 🔜 Spec'd |
| **Tauri integration** | Shared engine → real-time progress in GUI | 🔜 Spec'd |

---

## Architecture

### Current (transitional)

```
extension/              Chrome MV3 extension
  ├── background.js     Captures .m3u8 streams via webRequest
  ├── content.js        Hover overlay on <video> elements
  └── popup/            Stream list + download button

native-host/            Rust binaries (legacy)
  ├── darkdm-host       HTTP server on :8765 (serves extension)
  ├── darkdm-cli        HLS downloader (calls ffmpeg)
  ├── darkdm-proxy      Forward proxy for video capture
  └── darkdm-proxy-mitm Async CONNECT proxy

scripts/                Bash utilities (working)
  ├── darkdm-mediafire  MediaFire downloader + extractor
  ├── darkdm-capture    TLS key logging + pcap extraction
  └── watch-downloads.sh Terminal download monitor
```

### Target (native Rust CLI + Tauri)

```
src-tauri/src/
├── lib.rs                    Shared engine (used by CLI + Tauri GUI)
├── bin/cli.rs               `darkdm` CLI binary (clap)
│
└── downloader/               Core engine
    ├── pipeline.rs           Pipeline orchestrator
    ├── state.rs              State machine (12 states)
    ├── events.rs             Event broadcast channel
    ├── queue.rs              Concurrent download queue
    ├── retry.rs              Exponential backoff
    │
    ├── stages/               Pipeline stages
    │   ├── url_resolver.rs   HEAD request → detect type
    │   ├── link_extractor.rs Plugin registry dispatch
    │   ├── download_engine.rs
    │   │   ├── direct.rs     Single-thread download
    │   │   ├── segmented.rs  Multi-thread with Range
    │   │   └── resume.rs     Partial file recovery
    │   ├── hls.rs            HLS manifest parser
    │   ├── dash.rs           DASH manifest parser
    │   └── post_processor.rs
    │       ├── extract.rs    Archive extraction
    │       └── organizer.rs  File organization
    │
    └── plugins/              Site extractors
        ├── mod.rs            Registry + SiteExtractor trait
        ├── mediafire.rs      MediaFire page scraper
        ├── youtube.rs        yt-dlp wrapper
        ├── mega.rs           Mega.nz (future)
        ├── googledrive.rs    Google Drive (future)
        ├── hls.rs            HLS URL detector
        ├── dash.rs           DASH URL detector
        └── generic_page.rs   Fallback HTML parser

extension/                    Chrome MV3 extension (unchanged)

native-host/                  Legacy (will be deprecated)
```

---

## Design Patterns

The engine is built on **8 standard patterns** used by IDM, aria2, yt-dlp, and JDownloader:

| Pattern | Used By | Role |
|---------|---------|------|
| **Pipeline** | IDM, aria2 | Each download passes through independent stages (Resolve → Extract → Download → Process) |
| **State Machine** | aria2, JDownloader | 12 explicit states with validated transitions — no invalid states possible |
| **Plugin Registry** | yt-dlp (1000+ extractors) | `SiteExtractor` trait with priority-based auto-detection per URL |
| **Event System** | aria2 (RPC), IDM (COM) | `broadcast::channel` — terminal (indicatif) and GUI (Tauri) as consumers |
| **Retry Backoff** | aria2, curl | 5 retries with exponential backoff 1s→16s + random jitter |
| **Segmented Download** | IDM (core feature) | Parallel `Range` requests → assemble on completion |
| **Queue Manager** | IDM, JDownloader | FIFO queue with configurable concurrency limit |
| **Observer** | All major DM | Reactive UI updates via event streaming |

The full spec (2013 lines) is at [`openspec/changes/native-cli/design.md`](openspec/changes/native-cli/design.md).

---

## Quick Start

### Prerequisites

```bash
# Required for current scripts
sudo pacman -S curl unrar p7zip ffmpeg
```

### Clone and build

```bash
git clone https://github.com/JDis03/darkdm.git
cd darkdm

# Verify environment
./init.sh

# Build native host (for Chrome extension support)
cd native-host && cargo build --release
cp target/release/darkdm-host ~/.local/bin/
```

### Chrome Extension

1. Open `vivaldi://extensions` (or `chrome://extensions`)
2. Enable **Developer Mode**
3. Click **Load unpacked** → select the `extension/` directory
4. Visit any streaming site — detected streams appear in the popup

### MediaFire Downloads (bash)

```bash
# Make scripts available
export PATH="$PATH:$PWD/scripts"
cp scripts/darkdm-mediafire ~/.local/bin/

# Download with auto-extraction
darkdm-mediafire "https://www.mediafire.com/file/XXXX/archivo.rar/file"

# With password for protected archives
darkdm-mediafire "https://..." --password "mypass"

# Extract direct link only (no download)
darkdm-mediafire "https://..." --get-link

# Custom output directory
darkdm-mediafire "https://..." ~/Movies --password "mypass"
```

---

## Chrome Extension

```
extension/
├── manifest.json          MV3
├── background.js          webRequest → detects .m3u8 streams
├── content.js             Floating "⬇️ Download" overlay on <video>
├── hook.js                Monkey-patches fetch/XHR for manifest interception
├── popup/
│   ├── popup.html         Stream list UI
│   ├── popup.js           Download button → POST to localhost:8765
│   └── popup.css
└── icons/
```

The extension uses `http://localhost:8765` instead of `chrome.nativeMessaging` because MV3 service workers go to sleep after ~30s, causing `sendNativeMessage` to fail silently. HTTP `fetch()` works reliably regardless of worker lifecycle.

---

## Project Map

```
├── src-tauri/                  Tauri desktop app (Svelte + Rust)
│   ├── src/lib.rs              Download listing commands
│   └── src/main.rs             Entry point
│
├── native-host/                Rust binaries (legacy, will merge)
│   ├── src/server.rs           HTTP server on :8765
│   ├── src/downloader.rs       HLS/DASH download engine
│   ├── src/bin/cli.rs          HLS CLI
│   └── src/bin/proxy*.rs       Network proxies
│
├── extension/                  Chrome MV3 extension
│
├── scripts/                    Working bash utilities
│   ├── darkdm-mediafire        MediaFire downloader
│   ├── darkdm-capture          TLS packet capture
│   ├── darkdm-debug            Real-time log viewer
│   ├── darkdm-vivaldi          Launch Vivaldi with SSLKEYLOGFILE
│   └── watch-downloads.sh      Terminal download monitor
│
├── openspec/                   Spec-driven development
│   └── changes/native-cli/     Full native CLI spec (2013 lines)
│
├── init.sh                     Project verification
├── install.sh                  Extension + host installer
├── feature_list.json           Feature tracking
└── progress.md                 Session log
```

---

## Why Rust?

| Concern | Bash | Rust (native) |
|---------|------|---------------|
| **HTTP** | `curl` subprocess | `reqwest` — full control, streaming, Range |
| **HTML parsing** | `grep`/regex (brittle) | `scraper` — CSS selectors (robust) |
| **Multi-thread** | Not possible | Native `tokio` + `Range` segmentation |
| **Resume** | `curl -C -` (fragile) | Programmatic `Range` headers + verification |
| **Progress** | curl's bar (not capturable) | Callbacks → terminal (`indicatif`) + GUI |
| **Dependencies** | curl, unrar, ffmpeg, 7z, python | Static binary (~10MB, no deps) |
| **Error handling** | Exit codes | Typed errors, retry, state machine |
| **Testing** | Manual | Unit + integration tests |

---

## Roadmap

```
Phase 1  [✅] Working bash scripts + Chrome extension
Phase 2  [✅] Complete native CLI spec (2013 lines, 8 patterns)
Phase 3  [  ] Universal download engine in Rust (reqwest, segmented, resume)
Phase 4  [  ] CLI with clap (subcommands, --json, --interactive)
Phase 5  [  ] Page analyzer + plugins (MediaFire, YouTube, Mega)
Phase 6  [  ] Tauri integration (shared engine, progress events)
Phase 7  [  ] Deprecate bash scripts — all functionality in native CLI
```

---

## License

MIT
