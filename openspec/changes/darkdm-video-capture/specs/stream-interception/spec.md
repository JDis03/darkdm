## ADDED Requirements

### Requirement: The extension SHALL intercept network requests to detect media streams
The extension SHALL use two parallel mechanisms to detect media streams:
1. **PerformanceObserver**: Monitor `resource` entries for URLs matching `.m3u8`, `.mpd`, `.m4s`, `.ts` patterns
2. **chrome.debugger API**: Attach a debugger to the tab and listen for `Network.responseReceived` events where the response Content-Type matches video/audio MIME types

#### Scenario: PerformanceObserver detects media URL
- **WHEN** a resource entry with URL matching `/\.(m3u8|mpd|m4s|ts)(\?|$)/i` is observed
- **THEN** the content script SHALL send a `MEDIA_STREAM_DETECTED` message to the background with the URL and page info

#### Scenario: Debugger detects media response
- **WHEN** `chrome.debugger` is attached to a tab
- **AND** a `Network.responseReceived` event has Content-Type containing `video/`, `audio/`, `application/vnd.apple.mpegurl`, or `application/dash+xml`
- **THEN** the background SHALL call `Network.getResponseBody` to retrieve the response body
- **AND** SHALL forward the body to the native messaging host

### Requirement: The extension SHALL monkeypatch `fetch` to detect manifest requests
The content script SHALL wrap `window.fetch` to inspect URLs for HLS/DASH manifest patterns. When a manifest URL is detected, the extension SHALL notify the background regardless of whether the fetch succeeds or fails.

#### Scenario: Fetch to m3u8 URL
- **WHEN** a page script calls `fetch()` with a URL containing `.m3u8` or `.mpd`
- **THEN** the monkeypatched fetch SHALL send a `FETCH_MEDIA_DETECTED` message to the background
- **AND** SHALL continue the original fetch without modification

### Requirement: The debugger SHALL auto-attach on video detection
When the content script detects a video element and the user has not disabled automatic interception, the background SHALL attempt to attach `chrome.debugger` to the tab. If the user has not granted debugger permission, the extension SHALL prompt via the popup.

#### Scenario: Debugger attaches successfully
- **WHEN** `chrome.debugger.attach()` succeeds
- **AND** `Network.enable` is called
- **THEN** the debugger SHALL listen for `Network.responseReceived` and `Network.requestWillBeSent` events

#### Scenario: Debugger detaches on navigation
- **WHEN** the user navigates to a new page
- **THEN** the background SHALL detect the detach event
- **AND** SHALL clean up the debugger target entry
- **AND** MAY re-attach if the new page also contains video content
