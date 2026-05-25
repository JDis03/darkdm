## ADDED Requirements

### Requirement: The extension SHALL capture video buffers via `captureStream()`
When a user clicks the download overlay on a `<video>` element, the extension SHALL first attempt to call `HTMLMediaElement.captureStream()` to obtain a `MediaStream` of the decoded video frames. The stream SHALL be recorded using the `MediaRecorder` API with the best available codec (prefer VP9, fall back to VP8, then H264, then webm).

#### Scenario: captureStream succeeds
- **WHEN** `videoElement.captureStream()` returns a valid MediaStream
- **THEN** the extension SHALL create a `MediaRecorder` with the stream
- **AND** SHALL record the video with 5 Mbps video bitrate and 192 Kbps audio bitrate
- **AND** SHALL save chunks every 5000ms

#### Scenario: captureStream is not available
- **WHEN** the browser does not support `captureStream()` or the method is undefined
- **THEN** the extension SHALL fall back to `chrome.tabCapture` for full tab capture
- **AND** SHALL notify the user that direct buffer capture is unavailable

#### Scenario: captureStream throws an error
- **WHEN** `captureStream()` throws an error (e.g., due to DRM restrictions)
- **THEN** the extension SHALL catch the error
- **AND** SHALL attempt to use `chrome.tabCapture` as a fallback
- **AND** SHALL log the error for debugging

### Requirement: The MediaRecorder SHALL produce downloadable video files
The recorded MediaStream SHALL be encoded into a video file using the best available MIME type. The resulting Blob SHALL be sent to the background service worker, which SHALL either forward it to the native app for saving or trigger a browser download as fallback.

#### Scenario: Successful recording completion
- **WHEN** the MediaRecorder stops after successful recording
- **AND** recorded chunks contain data
- **THEN** a Blob SHALL be created from all chunks
- **AND** the filename SHALL follow the pattern `darkdm_{page_title}_{resolution}_{timestamp}.webm`
- **AND** the blob SHALL be sent to the background via `chrome.runtime.sendMessage`

#### Scenario: Recording stopped by user
- **WHEN** the user clicks the overlay button again while recording is active
- **THEN** the MediaRecorder SHALL stop
- **AND** the recorded data SHALL be finalized and delivered
- **AND** the overlay SHALL return to its idle "download" state
