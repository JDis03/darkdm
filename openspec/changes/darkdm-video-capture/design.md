## Context

DarkDM is a new project — a video download manager for Linux inspired by Internet Download Manager (IDM). The system consists of three components:
1. **Browser Extension** (Chrome/Vivaldi MV3): Detects videos and captures streams
2. **Native Messaging Host** (Rust): Bridges extension ↔ desktop app
3. **Desktop App** (Tauri 2.0): Download management GUI

The primary use case is capturing video from streaming sites like Netflix, YouTube, Prime Video, etc. The extension uses a multi-layer capture strategy: direct URL download → `captureStream()` buffer capture → `chrome.tabCapture` recording.

This design covers the full architecture, component interfaces, and key technical decisions.

## Goals / Non-Goals

**Goals:**
- Detect `<video>` elements on any webpage and show a floating download overlay
- Capture video buffers via `HTMLMediaElement.captureStream()` + `MediaRecorder`
- Intercept network requests via `chrome.debugger` for HLS/DASH manifest detection
- Communicate between extension and desktop app via Chrome native messaging
- Provide a Tauri desktop app for download management with progress UI
- Handle DRM content gracefully (fallback from `captureStream()` to `tabCapture`)
- Support multi-threaded downloads via aria2c or custom Rust implementation

**Non-Goals:**
- DRM key extraction or circumvention (legal/technical boundary)
- Mobile app (Android/iOS) — desktop Linux only
- P2P download acceleration
- Built-in media player
- Support for Firefox (MV2 sunset) — Vivaldi/Chrome only initially

## Decisions

| Decision | Choice | Rationale | Alternatives Considered |
|----------|--------|-----------|------------------------|
| **Desktop framework** | Tauri 2.0 (Rust + Web) | Small binary, native perf, strong Linux support | Electron (too heavy), Python/GTK (distribution harder) |
| **Frontend** | Svelte | Lightest bundle, excellent Tauri integration, less boilerplate | React (heavier), Vue (middle ground) |
| **Native messaging** | Rust binary via `chrome.runtime.connectNative` | Type-safe, fast, easy to distribute | Python (needs interpreter), Node.js (heavy) |
| **Download engine** | aria2c (external) for MVP, custom Rust engine later | Zero dev time for multi-threaded downloads, battle-tested | Custom Rust downloader (more dev effort), wget/curl (single-threaded) |
| **Video capture API** | `captureStream()` first, `chrome.tabCapture` fallback | captureStream gives decoded frames directly from `<video>` element | `canvas` drawImage (lossy, slow), MediaSource buffer read (not exposed by Chrome) |
| **Network interception** | `chrome.debugger` API (with user consent) | Full access to response bodies and headers | `declarativeNetRequest` (can't read bodies), `webRequest` (limited in MV3) |
| **Stream detection** | PerformanceObserver + debugger.Network events | Real-time detection, minimal overhead | Polling DOM (misses streams), service worker (limited scope) |
| **Project language** | English (artifacts) + Spanish (communication) | Consistent with codebase, but user communicates in Spanish | — |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Vivaldi / Chrome                          │
│                                                                  │
│  ┌────────────────────────────────────┐   ┌──────────────────┐   │
│  │       Content Script (content.js)  │   │ Service Worker   │   │
│  │  ┌─────────────────────────────┐   │   │ (background.js)  │   │
│  │  │ Video Detection             │   │   │                  │   │
│  │  │ captureStream() + MediaRec  │──┼──▶│ ┌──────────────┐ │   │
│  │  │ Floating overlay button     │   │   │ │ Native       │ │   │
│  │  │ DRM detection (EME check)   │   │   │ │ Messaging    │ │   │
│  │  └─────────────────────────────┘   │   │ │ Bridge       │ │   │
│  │  ┌─────────────────────────────┐   │   │ └──────┬───────┘ │   │
│  │  │ Network Interception        │   │   │ ┌──────────────┐ │   │
│  │  │ (PerformanceObserver)       │──┼──▶│ │ chrome.      │ │   │
│  │  │ Fetch/XHR monkeypatches    │   │   │ │ debugger     │ │   │
│  │  └─────────────────────────────┘   │   │ └──────────────┘ │   │
│  └────────────────────────────────────┘   └──────────────────┘   │
│                      │                   │                       │
│                      │ chrome.runtime    │ chrome.runtime        │
│                      │ .sendMessage()    │ .connectNative()      │
└──────────────────────┼───────────────────┼───────────────────────┘
                       │                   │
               ┌───────▼───────────────────▼──────────┐
               │    Native Messaging Host (Rust)       │
               │    stdin/stdout JSON protocol         │
               │    ~/bin/darkdm-host                  │
               └───────┬───────────────────────────────┘
                       │
          ┌────────────┼────────────────┐
          │            │                │
          ▼            ▼                ▼
   ┌──────────┐  ┌──────────┐  ┌──────────────┐
   │ Tauri    │  │ aria2c   │  │ wget/curl    │
   │ Desktop  │  │ (multi-  │  │ (fallback)   │
   │ App      │  │ thread)  │  │              │
   └──────────┘  └──────────┘  └──────────────┘
```

## Data Flow

### Flow A: Direct Video Download (YouTube, Vimeo, non-DRM)
```
Content Script detects <video> with src URL
  → chrome.runtime.sendMessage({ type: 'DOWNLOAD_VIDEO', url })
  → Background Service Worker
    → Native Messaging Host (stdin)
    → Launch aria2c/wget with URL
    → Download saved to ~/Descargas/DarkDM/
```

### Flow B: captureStream (some DRM sites, no Widevine L1)
```
Content Script detects <video>
  → video.captureStream() → MediaStream
  → MediaRecorder.start()
  → On data available: Blob chunks
  → Send to background → Native app or fallback download
```

### Flow C: chrome.tabCapture (heavy DRM: Netflix, Prime)
```
Content Script detects DRM video
  → Background creates Offscreen Document
  → chrome.tabCapture.getMediaStreamId()
  → Offscreen: getUserMedia with streamId
  → MediaRecorder records tab audio+video
  → Blob → Download
```

### Flow D: chrome.debugger (network-level interception)
```
User clicks "Scan Page" or extension auto-attaches
  → chrome.debugger.attach({ tabId })
  → Network.responseReceived → Check Content-Type for media
  → Network.getResponseBody → Get media data
  → Send to Native Host for processing
```

## Risks / Trade-offs

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `captureStream()` blocked on DRM sites | High | Medium | Fallback to `chrome.tabCapture`; notify user about recording mode |
| `chrome.debugger` detaches on navigation | Medium | Medium | Re-attach on `page` events; check connection state before use |
| Widevine L3 → L1 migration locks resolutions | Medium | High | Future: explore PipeWire portal screen capture for L1 content |
| Native messaging host manifest paths differ per browser | Low | Low | Provide install script with all known paths |
| aria2c not installed | Medium | Low | Bundle static aria2c binary or implement basic Rust downloader |
| Extension permissions scare users | Medium | Low | Request `debugger` permission lazily with clear explanation popup |

## Open Questions

- Should we auto-attach `chrome.debugger` when a video is detected, or only on user action?
- What video formats/codecs should `MediaRecorder` prefer for best quality? (VP9 vs H264)
- Should the desktop app serve an HTTP API for the native host, or use Unix sockets?
- Bundle aria2c as a static binary or rely on system install?
