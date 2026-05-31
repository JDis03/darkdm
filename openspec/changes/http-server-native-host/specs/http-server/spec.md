## ADDED Requirements

### Requirement: HTTP server listens on localhost
The native host SHALL start an HTTP server listening on `127.0.0.1:8765` at startup. The port SHALL be configurable via the `DARKDM_PORT` environment variable.

#### Scenario: Server starts on default port
- **WHEN** the native host process starts without `DARKDM_PORT` set
- **THEN** the HTTP server SHALL listen on `127.0.0.1:8765`

#### Scenario: Server starts on custom port
- **WHEN** the native host process starts with `DARKDM_PORT=9000`
- **THEN** the HTTP server SHALL listen on `127.0.0.1:9000`

#### Scenario: Port already in use
- **WHEN** the native host starts and port 8765 is already bound
- **THEN** the process SHALL log an error message and exit with non-zero status

### Requirement: POST /download endpoint accepts stream requests
The server SHALL expose a `POST /download` endpoint that accepts a JSON body with stream download parameters.

#### Scenario: Valid download request
- **WHEN** a POST request is received at `/download` with valid JSON containing `manifest_url`, `title`, `page_url`, and `headers`
- **THEN** the server SHALL respond with `200 OK` and JSON `{"success": true, "message": "Download started", "output_path": "<path>"}`

#### Scenario: Missing manifest_url
- **WHEN** a POST request is received at `/download` without `manifest_url` in the body
- **THEN** the server SHALL respond with `400 Bad Request` and JSON `{"success": false, "error": "manifest_url is required"}`

#### Scenario: Invalid JSON body
- **WHEN** a POST request is received at `/download` with malformed JSON
- **THEN** the server SHALL respond with `400 Bad Request` and JSON `{"success": false, "error": "Invalid JSON"}`

### Requirement: CORS headers for extension access
The server SHALL include CORS headers in all responses to allow requests from Chrome/Vivaldi extensions.

#### Scenario: Preflight OPTIONS request
- **WHEN** an OPTIONS request is received at any endpoint
- **THEN** the server SHALL respond with `204 No Content` and headers `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: POST, OPTIONS`, `Access-Control-Allow-Headers: Content-Type`

#### Scenario: POST response includes CORS headers
- **WHEN** a POST request is processed
- **THEN** the response SHALL include `Access-Control-Allow-Origin: *` header

### Requirement: ffmpeg execution matches bash script behavior
The server SHALL execute ffmpeg with the same flags and behavior as the working bash script (`/tmp/darkdm_ffmpeg_debug.sh`).

#### Scenario: ffmpeg launched with correct flags
- **WHEN** a valid download request is received with `page_url` as referer
- **THEN** the server SHALL execute: `ffmpeg -y -hide_banner -loglevel error -user_agent '<UA>' -referer '<page_url>' -i '<manifest_url>' -c copy -movflags +faststart '<output_path>'`

#### Scenario: ffmpeg runs in background
- **WHEN** ffmpeg is launched for a download
- **THEN** the server SHALL spawn ffmpeg as a background process (`.spawn()`) and return the HTTP response immediately without waiting for ffmpeg to complete

#### Scenario: Output file path
- **WHEN** a download request includes `title: "Jack Ryan"`
- **THEN** the output file SHALL be created at `~/Descargas/DarkDM/Jack Ryan.mp4`

### Requirement: Health check endpoint
The server SHALL expose a `GET /health` endpoint for monitoring.

#### Scenario: Health check returns OK
- **WHEN** a GET request is received at `/health`
- **THEN** the server SHALL respond with `200 OK` and JSON `{"status": "ok", "version": "1.0.0"}`

### Requirement: Systemd user service installation
The native host SHALL be installable as a systemd user service that starts automatically on login.

#### Scenario: Service starts on login
- **WHEN** the user logs in and the systemd service is enabled
- **THEN** the native host process SHALL start automatically and listen on the configured port

#### Scenario: Service restarts on crash
- **WHEN** the native host process crashes unexpectedly
- **THEN** systemd SHALL restart the process automatically (`Restart=on-failure`)
