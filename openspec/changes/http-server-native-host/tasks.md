## 1. Native Host — HTTP Server

- [x] 1.1 Add `tiny_http` crate to `native-host/Cargo.toml`
- [x] 1.2 Create `native-host/src/server.rs` module with HTTP server struct
- [x] 1.3 Implement server startup on `127.0.0.1:8765` (configurable via `DARKDM_PORT` env var)
- [x] 1.4 Implement CORS headers (`Access-Control-Allow-Origin: *`) on all responses
- [x] 1.5 Implement OPTIONS preflight handler (return 204 with CORS headers)
- [x] 1.6 Implement `GET /health` endpoint returning `{"status": "ok", "version": "1.0.0"}`

## 2. Native Host — Download Endpoint

- [x] 2.1 Implement `POST /download` endpoint that parses JSON body
- [x] 2.2 Validate required fields (`manifest_url`) and return 400 on missing fields
- [x] 2.3 Return 400 on invalid JSON body
- [x] 2.4 Extract `user_agent` and `referer` from `headers` JSON or fallback to `page_url`
- [x] 2.5 Build ffmpeg command with `-user_agent`, `-referer`, `-i`, `-c copy`, `-movflags +faststart`
- [x] 2.6 Spawn ffmpeg as background process (`.spawn()`) and return 200 immediately
- [x] 2.7 Sanitize title for output filename and write to `~/Descargas/DarkDM/<title>.mp4`

## 3. Native Host — Remove Native Messaging

- [x] 3.1 Remove `read_message()` and `write_message()` functions from `main.rs`
- [x] 3.2 Remove stdin/stdout message loop from `main()`
- [x] 3.3 Replace main loop with HTTP server listen loop
- [x] 3.4 Remove `ChromeMessage` and `Response` structs (replace with HTTP request/response types)
- [x] 3.5 Keep `downloader.rs` module (HLS/DASH download logic) unchanged

## 4. Native Host — Systemd Service

- [x] 4.1 Create `systemd/darkdm-host.service` unit file with `Restart=on-failure`
- [ ] 4.2 Update `install.sh` to install systemd service to `~/.config/systemd/user/`
- [x] 4.3 Run `systemctl --user daemon-reload` and `systemctl --user enable darkdm-host`
- [ ] 4.4 Remove native messaging manifest installation from `install.sh`
- [ ] 4.5 Add `systemctl --user start darkdm-host` to install script

## 5. Extension — Popup HTTP Client

- [x] 5.1 Update `popup.js` to use `fetch('http://localhost:8765/download', {method: 'POST', body: JSON.stringify(msg)})` instead of `chrome.runtime.sendMessage`
- [x] 5.2 Handle fetch errors (connection refused) with alert: "DarkDM no está corriendo"
- [x] 5.3 Handle server error responses with alert showing error message
- [x] 5.4 Close popup on successful response (`success: true`)

## 6. Extension — Simplify Background

- [x] 6.1 Remove `sn()` function (native messaging) from `background.js`
- [x] 6.2 Remove `connectNative` port code from `background.js`
- [x] 6.3 Remove `DOWNLOAD_MEDIA` handler from `onMessage` listener
- [x] 6.4 Remove `setInterval` keepalive from `background.js`
- [x] 6.5 Keep `webRequest.onSendHeaders` listener for stream detection
- [x] 6.6 Keep `GET_CAPTURED_MEDIA` handler in `onMessage` listener

## 7. Extension — Manifest Update

- [x] 7.1 Remove `nativeMessaging` from `permissions` in `manifest.json`
- [x] 7.2 Add `http://localhost:8765/*` to `host_permissions` in `manifest.json`

## 8. Testing

- [x] 8.1 Test `GET /health` with curl: `curl http://localhost:8765/health`
- [ ] 8.2 Test `POST /download` with curl using real pelisjuanita URL
- [ ] 8.3 Verify ffmpeg produces correct output file (303 MB, no corruption)
- [ ] 8.4 Test popup download flow end-to-end in Vivaldi
- [ ] 8.5 Test error handling: server not running, invalid JSON, missing fields
- [x] 8.6 Test systemd service: start, stop, restart on crash
