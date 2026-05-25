## ADDED Requirements

### Requirement: The native messaging host SHALL communicate via Chrome native messaging protocol
The Rust binary SHALL read messages from stdin using the Chrome native messaging protocol: 4-byte little-endian unsigned integer length prefix followed by UTF-8 JSON. It SHALL write responses to stdout using the same format. Errors SHALL be logged to stderr.

#### Scenario: Message received from browser
- **WHEN** the native host reads a valid JSON message from stdin
- **THEN** it SHALL parse the message and dispatch it to the appropriate handler based on the `type` field
- **AND** SHALL write a JSON response to stdout

#### Scenario: Invalid message received
- **WHEN** the native host receives malformed JSON or an incomplete message
- **THEN** it SHALL write an error response with `success: false` and the error message

### Requirement: The native host SHALL support the following message types
The native messaging host SHALL handle these message types: `VIDEO_DETECTED`, `STREAM_DETECTED`, `MANIFEST_DETECTED`, `START_DOWNLOAD`, `MEDIA_BODY_CAPTURED`, `RECORDING_DONE`, `PING`.

#### Scenario: PING message
- **WHEN** the native host receives a message with `type: "PING"`
- **THEN** it SHALL respond with `{ type: "PONG", success: true }`

#### Scenario: START_DOWNLOAD with URL
- **WHEN** the native host receives `{ type: "START_DOWNLOAD", url: "https://..." }`
- **THEN** it SHALL attempt to launch the download
- **AND** if the DarkDM desktop app is installed, delegate the download to it
- **AND** if the desktop app is not available, use aria2c or wget as fallback
- **AND** SHALL respond with `{ type: "DOWNLOAD_STARTED", success: true }`

### Requirement: The native host SHALL provide download mechanism via system tools
The host SHALL detect available download tools in this priority order:
1. DarkDM desktop app (`/opt/darkdm/darkdm-app`)
2. aria2c (multi-threaded, 16 connections, 1MB chunks, resume support)
3. wget (single-threaded, resume with `-c`)
4. curl (single-threaded, fallback)

Downloads SHALL be saved to `~/Descargas/DarkDM/` by default.

#### Scenario: aria2c available
- **WHEN** the native host needs to download a file
- **AND** `aria2c` is found in PATH
- **THEN** it SHALL launch aria2c with: `-x 16 -s 16 -k 1M --continue --max-connection-per-server=16`
- **AND** SHALL not wait for the process to complete

#### Scenario: No download tool available
- **WHEN** no supported download tool is found
- **THEN** the native host SHALL respond with an error explaining the requirement

### Requirement: The native messaging host manifest SHALL be installable
The project SHALL provide the native messaging host manifest (`com.darkdm.manager.json`) at an installable path. The extension ID in `allowed_origins` SHALL be configurable. The binary path SHALL point to the compiled Rust executable.

#### Scenario: Default extension ID
- **WHEN** no custom extension ID is configured
- **THEN** the manifest SHALL use a placeholder ID `knldjmfmopnpolahpmmgbagdohdnhkik`
- **AND** SHALL document how to update it for the real extension ID
