## ADDED Requirements

### Requirement: The extension SHALL detect DRM on video elements
The content script SHALL detect whether a `<video>` element uses Encrypted Media Extensions (EME) / Widevine DRM. Detection SHALL check:
1. `video.mediaKeys` — if set, DRM is active
2. `video.getAttribute('onencrypted')` — encrypted event handler present
3. Source elements with `keySystem` attribute
4. `encrypted` event listener presence (via wrapped `addEventListener`)

#### Scenario: Video with Widevine DRM
- **WHEN** a `<video>` element has `mediaKeys` property set to a valid MediaKeys object
- **THEN** the extension SHALL mark the video as DRM-protected
- **AND** SHALL display the overlay with "🎬 Capturar con DarkDM" instead of "⬇️ Descargar con DarkDM"
- **AND** SHALL show a "DRM" badge on the overlay

#### Scenario: Non-DRM video
- **WHEN** a `<video>` element has no EME-related properties or events
- **THEN** the extension SHALL consider it non-DRM
- **AND** SHALL prioritize direct source URL detection and download

### Requirement: The extension SHALL use layered fallback for DRM content
When the user clicks the download overlay on a DRM-protected video, the extension SHALL attempt strategies in this order:
1. **`captureStream()` + MediaRecorder**: Try to capture the decoded video buffer (may be blocked by some DRM implementations)
2. **`chrome.tabCapture`**: If captureStream fails, capture the entire tab as a media stream

#### Scenario: captureStream works on DRM video
- **WHEN** the user clicks the overlay on a DRM video
- **AND** `videoElement.captureStream()` returns a valid MediaStream
- **THEN** the extension SHALL record the stream via MediaRecorder
- **AND** SHALL process it as a normal buffer capture (see buffer-capture spec)
- **AND** SHALL log that captureStream succeeded despite DRM

#### Scenario: captureStream blocked by DRM
- **WHEN** the user clicks the overlay on a DRM video
- **AND** `videoElement.captureStream()` throws an error or returns empty
- **THEN** the extension SHALL catch the error
- **AND** SHALL request `chrome.tabCapture.getMediaStreamId()` for the tab
- **AND** SHALL create an offscreen document to handle the recording
- **AND** the overlay SHALL show "Capturando video..." with a recording indicator

### Requirement: The extension SHALL create an offscreen document for tab capture
When `chrome.tabCapture` is needed, the extension SHALL create an offscreen document that receives the stream ID and starts recording via `getUserMedia({ chromeMediaSource: 'tab' })`. The offscreen document SHALL record with `video/webm` codec at 8 Mbps and SHALL send the recorded Blob to the background when complete.

#### Scenario: Offscreen document created
- **WHEN** tab capture is initiated
- **AND** no offscreen document exists
- **THEN** `chrome.offscreen.createDocument()` SHALL be called with `reasons: ['USER_MEDIA']`
- **AND** the offscreen document SHALL be used exclusively for the recording session

#### Scenario: Recording complete via tab capture
- **WHEN** the user stops the recording or the tab is closed
- **THEN** the offscreen document SHALL finalize the Blob
- **AND** SHALL send it to the background via `chrome.runtime.sendMessage`
- **AND** SHALL stop all media tracks
