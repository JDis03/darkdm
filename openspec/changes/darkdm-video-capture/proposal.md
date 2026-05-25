## Why

Users of Linux lack a native video download manager comparable to Internet Download Manager (IDM) on Windows — a tool that detects video streams on any website (including DRM-protected ones like Netflix), captures the buffer in real-time, and downloads videos with multi-threaded acceleration. Current Linux solutions are either command-line only (yt-dlp, aria2c) or lack browser integration. DarkDM fills this gap by offering a full-stack solution: a browser extension with video detection + a desktop download manager with a native GUI.

## What Changes

- **Browser Extension (Vivaldi/Chrome MV3)**: Content script that detects `<video>` elements, uses `captureStream()` to capture video buffers, and intercepts network requests for HLS/DASH manifests. Shows a floating overlay button for one-click download or capture.
- **Native Messaging Host**: Rust binary that bridges the browser extension with the desktop app via Chrome's native messaging protocol (stdin/stdout JSON).
- **Desktop App (Tauri/Rust + Web Frontend)**: GUI for managing downloads, showing progress, scheduling, and organizing captured videos by category.
- **Automatic Stream Detection**: Uses `chrome.debugger` API (with user consent) to intercept network traffic and detect MPD/M3U8 manifests and media segments — similar to how IDM captures video from Netflix, YouTube, and other streaming platforms.
- **Multi-Layer Capture Strategy**:
  1. Network interception (direct URLs, best quality, non-DRM)
  2. `HTMLMediaElement.captureStream()` (decoded buffer, works with some DRM)
  3. `chrome.tabCapture` (full tab capture, works with heavy DRM like Widevine)

## Capabilities

### New Capabilities
- `video-detection`: Detect `<video>` elements on any webpage, identify their source URLs, DRM status, and resolution. Show a floating overlay button for download actions.
- `buffer-capture`: Use `captureStream()` + `MediaRecorder` to capture the decoded video buffer from a `<video>` element in real-time. Record in configurable quality (bitrate, resolution, codec).
- `stream-interception`: Use `chrome.debugger` API to intercept network requests and detect HLS (m3u8), MPEG-DASH (mpd), and direct media file URLs. Capture response bodies for download.
- `native-messaging-host`: Rust binary that reads JSON messages from the browser extension via stdin/stdout, dispatches downloads to desktop app or system tools (aria2c, wget), and reports progress back.
- `download-management`: Desktop app with queued multi-threaded downloads, progress tracking, pause/resume, speed limiting, and organization by category/site.
- `drm-handling`: Detect Widevine/EME DRM on video elements. Fall back from `captureStream()` to `chrome.tabCapture` when DRM blocks direct buffer access. Notify users about DRM limitations.

### Modified Capabilities
*(None — this is a new project with no existing specs.)*

## Impact

- **New code**: Browser extension (~500 LOC JS), native messaging host (~300 LOC Rust), Tauri desktop app (~800 LOC Rust + ~500 LOC frontend)
- **Dependencies**: Rust toolchain, Tauri 2.0, Node.js for frontend build, aria2c (optional, for multi-threaded fallback), chrome.debugger API permissions
- **System integration**: Native messaging host manifest must be installed at `~/.config/vivaldi/NativeMessagingHosts/com.darkdm.manager.json` (or equivalent Chromium path)
- **Permissions**: Extension requires `nativeMessaging`, `debugger`, `tabCapture`, `offscreen`, `storage`, `downloads`, `scripting`, and host permissions for `http://*/*` `https://*/*`
