## 1. Project Scaffolding & OpenSpec

- [x] 1.1 Initialize OpenSpec in the project
- [x] 1.2 Create change `darkdm-video-capture` with proposal, design, specs, tasks
- [x] 1.3 Create project directory structure (extension/, native-host/, src-tauri/)

## 2. Browser Extension — Content Script

- [x] 2.1 Create `manifest.json` with MV3 permissions (nativeMessaging, debugger, tabCapture, offscreen, scripting, host permissions)
- [x] 2.2 Implement `content.js` — detect `<video>` elements, show floating overlay, handle mouse hover
- [x] 2.3 Implement `content.js` — DRM detection via EME (mediaKeys, encrypted events)
- [x] 2.4 Implement `content.js` — `captureStream()` + `MediaRecorder` buffer capture
- [x] 2.5 Implement `content.css` — floating overlay styles (dark theme, animations, DRM badge, recording state)
- [x] 2.6 Implement network interception via PerformanceObserver + fetch monkeypatch
- [x] 2.7 Generate extension icons (16, 48, 128 PNG)

## 3. Browser Extension — Background Service Worker

- [x] 3.1 Implement `background.js` — `chrome.runtime.connectNative()` native messaging bridge
- [x] 3.2 Implement `background.js` — `chrome.debugger` attach/detach for network interception
- [x] 3.3 Implement `background.js` — Media stream detection via debugger events (Network.responseReceived, Network.requestWillBeSent)
- [x] 3.4 Implement `background.js` — Context menus (video, link, page contexts)
- [x] 3.5 Implement `background.js` — Command handler (keyboard shortcut)
- [x] 3.6 Implement `background.js` — Tab capture coordinator (offscreen document management)
- [x] 3.7 Implement `background.js` — Message routing (content ↔ native host ↔ offscreen)

## 4. Browser Extension — Offscreen Document & Popup

- [x] 4.1 Create `offscreen.html` + `offscreen.js` — tab capture via `getUserMedia` + `MediaRecorder`
- [x] 4.2 Create `popup/popup.html` — popup UI with scan button, stream list, connection status
- [x] 4.3 Create `popup/popup.css` — dark theme popup styles
- [x] 4.4 Create `popup/popup.js` — connection checks, scan trigger, debugger toggle, stream list updates

## 5. Native Messaging Host (Rust)

- [x] 5.1 Create `Cargo.toml` with dependencies (serde, serde_json, base64, ureq)
- [x] 5.2 Implement `main.rs` — stdin/stdout Chrome native messaging protocol reader/writer
- [x] 5.3 Implement message dispatch (VIDEO_DETECTED, STREAM_DETECTED, START_DOWNLOAD, PING, etc.)
- [x] 5.4 Implement download launcher (DarkDM app → aria2c → wget → curl fallback chain)
- [x] 5.5 Create `com.darkdm.manager.json` — native messaging host manifest
- [x] 5.6 Build and verify native host compiles successfully

## 6. Desktop App (Tauri 2.0 + Frontend)

- [ ] 6.1 Initialize Tauri 2.0 project with `npm create tauri-app` (Svelte frontend)
- [ ] 6.2 Configure `tauri.conf.json` (window title, size, permissions, app identifier)
- [ ] 6.3 Create main Rust backend with Tauri commands (start_download, pause, resume, list_downloads)
- [ ] 6.4 Implement download manager service in Rust (aria2c integration, progress tracking)
- [ ] 6.5 Create Svelte frontend — download list component with progress bars
- [ ] 6.6 Create Svelte frontend — add download dialog, settings panel
- [ ] 6.7 Create Svelte frontend — category/site organization sidebar

## 7. System Integration & Installation

- [ ] 7.1 Create install script (`install.sh`) that sets up native messaging manifest and binary
- [ ] 7.2 Create uninstall script (`uninstall.sh`)
- [ ] 7.3 Document extension ID configuration for `allowed_origins` in native host manifest
- [ ] 7.4 Add keyboard shortcuts for quick capture (configurable in manifest.json)

## 8. Testing & Verification

- [ ] 8.1 Load extension in Vivaldi/Chrome in developer mode and verify overlay appears on video pages
- [ ] 8.2 Test native messaging: send PING from extension and verify PONG response
- [ ] 8.3 Test buffer capture on non-DRM video (YouTube, Vimeo)
- [ ] 8.4 Test DRM detection and tab capture fallback on DRM site
- [ ] 8.5 Test download via aria2c (verify multi-threaded download works)
- [ ] 8.6 Test extension popup functionality (scan, stream list, connection status)
- [ ] 8.7 Validate all specs are implemented correctly
