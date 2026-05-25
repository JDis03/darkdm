# DarkDM — Video Download Manager for Linux

DarkDM is a video download manager for Linux inspired by Internet Download Manager (IDM). It captures video from streaming sites through a browser extension + native host + yt-dlp integration.

## Architecture

```
Extension (Vivaldi/Chrome MV3)
  ↓ chrome.debugger + captureStream + MediaSource
Native Host (Rust)
  ↓ native messaging
yt-dlp / ffmpeg / aria2c
  ↓
~/Downloads/DarkDM/
```

### 3 Capture Levels (like IDM)

1. **Network interception** — chrome.debugger detects HLS/DASH streams
2. **Native Messaging** — Rust host downloads via yt-dlp/ffmpeg
3. **Buffer capture** — captureStream grabs decoded video buffer

## Quick Start

```bash
# Install
cd extension/
# Load unpacked in Vivaldi: vivaldi://extensions → Load unpacked

# Dependencies
sudo pacman -S yt-dlp ffmpeg aria2c
```

## Project Structure

```
extension/        — Vivaldi/Chrome extension (MV3)
native-host/      — Rust native messaging host
openspec/         — Specifications (OpenSpec)
src-tauri/        — Desktop app (Tauri + Svelte)
```

## Status

| Feature | Status |
|---------|--------|
| YouTube download | ✅ Works |
| HLS stream detection | ✅ Works |
| captureStream (buffer) | ✅ Works |
| yt-dlp integration | ✅ Works |
| Site-specific extractors | 🔜 TODO |
| Desktop app (Tauri) | 🔜 TODO |
