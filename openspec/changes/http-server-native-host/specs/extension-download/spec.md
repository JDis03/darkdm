## ADDED Requirements

### Requirement: Popup sends download via HTTP fetch
The popup SHALL send download requests to the native host via HTTP POST to `http://localhost:8765/download` instead of using `chrome.runtime.sendNativeMessage`.

#### Scenario: Successful download request
- **WHEN** the user clicks the "Descargar" button on a detected stream
- **THEN** the popup SHALL send a POST request to `http://localhost:8765/download` with JSON body containing `manifest_url`, `title`, `page_url`, `headers`, and `cookies`

#### Scenario: Download started feedback
- **WHEN** the server responds with `{"success": true}`
- **THEN** the popup SHALL close automatically

#### Scenario: Server not running
- **WHEN** the fetch request fails (connection refused, timeout)
- **THEN** the popup SHALL display an alert: "DarkDM no está corriendo. Ejecuta: systemctl --user start darkdm-host"

#### Scenario: Server returns error
- **WHEN** the server responds with `{"success": false, "error": "<message>"}`
- **THEN** the popup SHALL display an alert with the error message

### Requirement: Extension no longer requires nativeMessaging permission
The extension manifest SHALL NOT include the `nativeMessaging` permission. The extension SHALL include `http://localhost:8765/*` in `host_permissions`.

#### Scenario: Manifest permissions check
- **WHEN** the extension is loaded in Vivaldi
- **THEN** the manifest SHALL NOT contain `nativeMessaging` in permissions
- **THEN** the manifest SHALL contain `http://localhost:8765/*` in host_permissions

### Requirement: Background service worker simplified
The background service worker SHALL NOT contain native messaging code (`sendNativeMessage`, `connectNative`). The background SHALL only handle stream detection via `webRequest` and respond to `GET_CAPTURED_MEDIA` messages from the popup.

#### Scenario: Background handles stream detection
- **WHEN** a `.m3u8` URL is detected via `webRequest.onSendHeaders`
- **THEN** the background SHALL store the stream info (URL, headers, referer from `details.initiator`) in `capturedMedia`

#### Scenario: Background responds to popup queries
- **WHEN** the popup sends `GET_CAPTURED_MEDIA` message
- **THEN** the background SHALL respond with the captured media array for the current tab

### Requirement: No service worker keepalive needed
The background service worker SHALL NOT use `setInterval` or port-based keepalive mechanisms. The `fetch()` API works reliably even when the service worker is dormant.

#### Scenario: Worker dormant but fetch works
- **WHEN** the service worker has been dormant for more than 30 seconds
- **THEN** the popup's `fetch()` call to `http://localhost:8765/download` SHALL still succeed
