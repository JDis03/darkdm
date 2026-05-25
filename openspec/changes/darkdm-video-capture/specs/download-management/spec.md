## ADDED Requirements

### Requirement: The desktop app SHALL show active downloads with progress
The DarkDM desktop app (Tauri) SHALL display a list of active and completed downloads. Each download SHALL show: filename, progress percentage, download speed, estimated time remaining, source URL, and file size. Users SHALL be able to pause, resume, and cancel active downloads.

#### Scenario: Download starts
- **WHEN** a new download is initiated
- **THEN** it SHALL appear in the downloads list with a progress bar at 0%
- **AND** the status SHALL show "Descargando..."

#### Scenario: Download completes
- **WHEN** a download reaches 100%
- **THEN** the status SHALL change to "Completado"
- **AND** a notification SHALL be shown via the system notification daemon
- **AND** the file path SHALL be displayed

#### Scenario: Download paused and resumed
- **WHEN** the user clicks "Pause" on an active download
- **THEN** the download SHALL be paused via SIGSTOP to the download process (or via aria2c RPC)
- **WHEN** the user clicks "Resume"
- **THEN** the download SHALL continue with aria2c `--continue` flag

### Requirement: The desktop app SHALL organize downloads by source site
Downloaded files SHALL be organized into subdirectories based on the source website. The default output directory SHALL be `~/Descargas/DarkDM/{sitename}/`. The sitename SHALL be extracted from the URL's hostname.

#### Scenario: Download from YouTube
- **WHEN** a video is downloaded from `youtube.com`
- **THEN** the file SHALL be saved to `~/Descargas/DarkDM/youtube.com/filename.mp4`

#### Scenario: Download from Netflix
- **WHEN** a video is captured from `netflix.com`
- **THEN** the file SHALL be saved to `~/Descargas/DarkDM/netflix.com/darkdm_capture_{timestamp}.webm`

### Requirement: The popup SHALL show detected streams
The extension popup SHALL display a list of detected media streams for the current page. Each stream SHALL show its filename and type (HLS, DASH, MP4). Users SHALL be able to trigger downloads from the popup.

#### Scenario: Stream detected while popup is open
- **WHEN** a new stream URL is detected via network interception
- **AND** the popup is currently open
- **THEN** the popup SHALL dynamically add the stream to the list
- **AND** SHALL show the stream type badge (HLS/DASH/MP4)
