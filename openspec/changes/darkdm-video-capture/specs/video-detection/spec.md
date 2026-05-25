## ADDED Requirements

### Requirement: The extension SHALL detect `<video>` elements on all web pages
The content script SHALL scan the DOM for `<video>` elements and identify their attributes (src, currentSrc, videoWidth, videoHeight, duration, paused state). The detection SHALL run on page load and SHALL use a MutationObserver to detect dynamically added video elements. The extension SHALL prefer the largest visible video element on the page when multiple videos exist.

#### Scenario: Single video on page
- **WHEN** a page contains exactly one `<video>` element that is visible (bounding rect area > 5000px²)
- **THEN** the extension SHALL identify it as the primary video target and SHALL prepare the floating overlay

#### Scenario: Multiple videos on page
- **WHEN** a page contains multiple `<video>` elements
- **THEN** the extension SHALL select the one with the largest visible bounding rect area
- **AND** SHALL prefer videos that are actively playing (`paused === false`) or have loaded data (`readyState >= 2`)

#### Scenario: Dynamically added video
- **WHEN** a `<video>` element is added to the DOM after page load (e.g., JS-loaded player)
- **THEN** the MutationObserver SHALL detect it within 1000ms and re-evaluate the primary video target

### Requirement: The extension SHALL provide a floating overlay button over detected videos
When the user's mouse hovers over a detected video element, the extension SHALL show a floating overlay button at the top-right corner of the video. The overlay SHALL have a dark semi-transparent background with a download icon, the text "Descargar con DarkDM" (or "Capturar con DarkDM" for DRM content), and a DRM badge when applicable. The overlay SHALL disappear 3 seconds after the mouse leaves the video area.

#### Scenario: Mouse enters video area
- **WHEN** the user moves their cursor over a detected video element
- **THEN** the floating overlay SHALL appear with animation (fade in + slide down, 300ms)
- **AND** SHALL be positioned at the top-right corner of the video element

#### Scenario: Mouse leaves video area
- **WHEN** the user moves their cursor out of the video element
- **THEN** the overlay SHALL remain visible for 3 seconds
- **AND** SHALL then fade out and hide

#### Scenario: Overlay click action
- **WHEN** the user clicks the floating overlay button
- **THEN** the extension SHALL attempt to capture or download the video using the appropriate strategy (see buffer-capture and drm-handling specs)

### Requirement: The extension SHALL provide a context menu for video download
The extension SHALL register Chrome context menu items for video, audio, link, and page contexts. The context menu SHALL provide options to download individual videos, download linked media, and scan all videos on the current page.

#### Scenario: Right-click on video element
- **WHEN** the user right-clicks on a `<video>` or `<audio>` element
- **THEN** the context menu SHALL show "⬇️ Descargar video con DarkDM"
- **AND** clicking it SHALL trigger the download/capture flow for that element

#### Scenario: Right-click on page
- **WHEN** the user right-clicks on a page (not on a specific video element)
- **THEN** the context menu SHALL show "⬇️ DarkDM - Detectar videos en esta página"
- **AND** clicking it SHALL scan the page for videos and activate network interception
