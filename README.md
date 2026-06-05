# DarkDM — Video Download Manager for Linux

DarkDM is a video download manager for Linux inspired by IDM. It detects HLS streams automatically from any streaming site and downloads them via ffmpeg — no DRM bypassing, just capturing what the browser already plays.

## Architecture

```
Vivaldi/Chrome (MV3 Extension)
  ↓ webRequest.onSendHeaders detects .m3u8 URLs
  ↓ Popup shows detected streams
  ↓ fetch('http://localhost:8765/download')
Native Host (Rust HTTP Server)
  ↓ Downloads manifest with curl
  ↓ Filters ad URLs injected by the site
  ↓ Resolves relative segment URLs to absolute
  ↓ ffmpeg -c copy (no re-encode, multiplexed stream)
  ↓
~/Descargas/DarkDM/video.mp4
```

### Why HTTP server instead of native messaging?

Chrome MV3 service workers go to sleep after ~30s of inactivity. When the user clicks download, the worker wakes up but `chrome.runtime.sendNativeMessage` fails silently — the host never launches. Video DownloadHelper solved this with `chrome.offscreen` API, but that only works for browser-native downloads. Since DarkDM needs to run `ffmpeg` (an external binary), it uses an HTTP server on `localhost:8765` instead. `fetch()` works reliably even when the service worker is dormant.

## Quick Start

### 1. Build and install native host

```bash
cd native-host/
cargo build --release
cp target/release/darkdm-host ~/.local/bin/

# Install as systemd user service (auto-start on login)
cp ../systemd/darkdm-host.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now darkdm-host

# Verify
curl http://localhost:8765/health
# → {"status":"ok","version":"1.0.0"}
```

### 2. Load extension in Vivaldi/Chrome

```
vivaldi://extensions → Enable developer mode → Load unpacked → select extension/
```

### 3. Download a video

1. Open any streaming site (pelisjuanita.com, etc.)
2. Play the video so it loads
3. Click the **DarkDM** extension icon — detected streams appear
4. Click **⬇️ Descargar**
5. Check `~/Descargas/DarkDM/`

## Dependencies

```bash
sudo pacman -S ffmpeg curl
```

## Project Structure

```
extension/        — Chrome/Vivaldi MV3 extension
  background.js   — webRequest stream detection
  popup/          — Stream list + download button
native-host/      — Rust HTTP server (localhost:8765)
  src/server.rs   — HTTP endpoints + ffmpeg launcher
  src/downloader.rs — HLS/DASH download utilities
openspec/         — Spec-driven change tracking
scripts/          — Helper scripts
```

## HTTP API

The native host exposes a simple REST API:

```
GET  /health          → {"status":"ok","version":"1.0.0"}
POST /download        → Launch ffmpeg in background
OPTIONS /download     → CORS preflight
```

### POST /download

```json
{
  "manifest_url": "https://cdn.example.com/stream.m3u8",
  "title": "Movie Title",
  "page_url": "https://pelisjuanita.com/...",
  "headers": {
    "user-agent": "Mozilla/5.0 ...",
    "referer": "https://pelisjuanita.com/..."
  }
}
```

Response:
```json
{"success": true, "message": "Download started", "output_path": "/home/dark/Descargas/DarkDM/Movie Title.mp4"}
```

## Known Limitations

- Sites that inject TikTok/ad URLs into HLS manifests may fail (the host tries to filter them but not all cases are covered)
- DRM-protected streams (Netflix, Disney+, Prime) are not supported — DarkDM only downloads streams the browser can play without DRM
- The native host must be running before clicking download (`systemctl --user status darkdm-host`)

## Status

| Feature | Status |
|---------|--------|
| Auto-detect HLS streams via webRequest | ✅ Works |
| HTTP server (replaces native messaging) | ✅ Works |
| ffmpeg download (-c copy, no re-encode) | ✅ Works |
| Ad URL filtering in manifests | 🔧 Partial (known sites) |
| Multi-stream popup (per tab) | ✅ Works |
| Stream dedup on page reload | ✅ Works |
| Systemd auto-start | ✅ Works |
| DASH streams | 🔜 TODO |
| Download progress in popup | 🔜 TODO |
