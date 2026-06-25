# DarkDM Logging System

DarkDM uses `tracing` for structured logging with file rotation and multiple output targets.

## Features

- **Console output**: Colored, pretty-printed logs
- **File output**: Rotating daily logs in `~/.local/share/darkdm/`
- **Log levels**: ERROR, WARN, INFO, DEBUG, TRACE
- **File rotation**: Daily rotation, keeps last 5 files
- **Environment control**: Use `RUST_LOG` to control verbosity

## Log Locations

```bash
# Default log directory
~/.local/share/darkdm/

# Today's log file
~/.local/share/darkdm/darkdm.2026-06-25.log

# Older logs (auto-rotated)
~/.local/share/darkdm/darkdm.2026-06-24.log
~/.local/share/darkdm/darkdm.2026-06-23.log
```

## Usage

### Basic Commands

```bash
# Download with default logging (INFO level)
darkdm descargar https://example.com/file.zip

# Download with verbose logging (DEBUG level)
darkdm descargar https://example.com/file.zip --verbose

# View recent logs
darkdm logs

# View last 20 lines
darkdm logs -n 20

# Follow logs in real-time (like tail -f)
darkdm logs --follow
```

### Environment Variables

```bash
# Set global log level
RUST_LOG=debug darkdm descargar https://example.com/file.zip

# Filter by module
RUST_LOG=darkdm::downloader=trace darkdm descargar https://example.com/file.zip

# Multiple filters
RUST_LOG=darkdm=debug,reqwest=warn darkdm descargar https://example.com/file.zip
```

## Log Levels

| Level | Description | Use Case |
|-------|-------------|----------|
| **ERROR** | Critical errors | Download failures, disk errors |
| **WARN** | Warnings | No resume support, auto-rename |
| **INFO** | General info | Download started, probe results |
| **DEBUG** | Detailed info | Piece progress, HTTP responses |
| **TRACE** | Very detailed | HTTP headers, chunk sizes |

## Log Format

### Console (colored)
```
[2026-06-25T21:16:35.589Z] INFO darkdm::downloader: Starting download
[2026-06-25T21:16:35.590Z] DEBUG darkdm::downloader::piece_worker:58: Starting piece 0: range 0-1048575
```

### File (plain text with thread IDs)
```
2026-06-25T21:16:35.589150Z  INFO ThreadId(01) darkdm::downloader: 68: Starting download
2026-06-25T21:16:35.590123Z DEBUG ThreadId(02) darkdm::downloader::piece_worker: 58: Starting piece 0
```

## Key Log Points

### Download Flow

1. **Probe**: `INFO` - Starting probe, result summary
2. **Extraction**: `INFO` - Extractor selection, links found
3. **Download Start**: `INFO` - File size, multi-threaded mode
4. **Piece Progress**: `DEBUG` - Each piece start/complete
5. **Errors**: `ERROR` - Worker failures, disk errors

### Example Session

```bash
$ darkdm descargar https://example.com/file.zip --verbose

[INFO] DarkDM logging initialized
[INFO] DarkDM CLI started
[INFO] Starting probe for URL: https://example.com/file.zip
[DEBUG] Probe result: filename=file.zip, size=Some(10485760), resumable=true
[INFO] Starting multi-threaded download: 10485760 bytes (10.00 MB)
[DEBUG] Starting piece 0: range 0-1310719 (1310720 bytes)
[DEBUG] Starting piece 1: range 1310720-2621439 (1310720 bytes)
...
[DEBUG] Piece 0 completed successfully
[DEBUG] Piece 1 completed successfully
...
```

## Debugging

### Common Issues

**No logs appearing?**
```bash
# Check log file exists
darkdm logs

# Try verbose mode
darkdm descargar <url> --verbose
```

**Too much output?**
```bash
# Reduce verbosity
RUST_LOG=warn darkdm descargar <url>
```

**Need more detail?**
```bash
# Enable trace for specific module
RUST_LOG=darkdm::downloader::piece_worker=trace darkdm descargar <url>
```

### Performance Impact

- **INFO level**: Minimal overhead (~1-2%)
- **DEBUG level**: Low overhead (~3-5%)
- **TRACE level**: Moderate overhead (~10-15%)

For production downloads, use INFO or WARN level.

## Integration

### Tauri App

The logging system is automatically initialized when the Tauri app starts:

```rust
use app_lib::downloader::logger;

fn main() {
    logger::init();
    // ... rest of app
}
```

### Custom Log Level

```rust
use app_lib::downloader::logger;

// Initialize with custom level
logger::init_with_level("debug");
```

### Adding Logs

```rust
use tracing::{info, debug, warn, error};

// Info level
tracing::info!("Download started: {}", url);

// Debug with context
tracing::debug!("Piece {} progress: {}/{} bytes", id, downloaded, total);

// Error with details
tracing::error!("Worker failed: {}", error);
```

## Maintenance

### Log Rotation

- **Automatic**: Daily rotation at midnight
- **Retention**: Last 5 days kept
- **Manual cleanup**: Delete old logs from `~/.local/share/darkdm/`

### Disk Usage

```bash
# Check log directory size
du -sh ~/.local/share/darkdm/

# Clean old logs (keeps today's)
find ~/.local/share/darkdm/ -name "darkdm.*.log" -mtime +7 -delete
```

## Future Enhancements

- [ ] JSON structured logging option
- [ ] Remote logging (syslog, journald)
- [ ] Log compression (gzip old files)
- [ ] Web UI for log viewing
- [ ] Log search/filter commands
