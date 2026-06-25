# DarkDM CLI

Native Rust download manager with multi-threaded downloads, resume support, and dynamic piece-splitting.

## Build

```bash
cargo build --release --bin darkdm
cp target/release/darkdm ~/.local/bin/
```

## Usage

### Download a file

```bash
darkdm descargar "https://example.com/file.mp4"
```

With options:

```bash
# Custom output directory
darkdm descargar "https://example.com/file.mp4" --output ~/Videos

# More workers (default: 8)
darkdm descargar "https://example.com/file.mp4" --threads 16

# Disable resume
darkdm descargar "https://example.com/file.mp4" --no-resume
```

### Probe a URL (no download)

```bash
darkdm info "https://example.com/file.mp4"
```

Output:
```
📡 DarkDM — Probing URL
URL: https://example.com/file.mp4

✓ Probe Result:
  Filename: file.mp4
  Size: 10485760 bytes (10.00 MB)
  Resumable: yes
  Content-Type: video/mp4
  Final URL: https://cdn.example.com/file.mp4
```

## Features

### Implemented

- ✅ Multi-threaded download with Range headers
- ✅ Dynamic piece-splitting (work-stealing)
- ✅ Probe URL metadata (filename, size, resumable)
- ✅ Single-thread fallback (no Range support)
- ✅ Accept-Encoding: identity (critical for Range accuracy)
- ✅ Content-Disposition filename parsing
- ✅ Text redirect detection

### In Progress

- 🔜 Progress bars (indicatif)
- 🔜 Resume from partial downloads (TransactedIO state)
- 🔜 Retry with exponential backoff
- 🔜 Disk space check before download
- 🔜 Auto-rename on filename conflicts

### Planned

- 🔜 Plugins: MediaFire, YouTube (yt-dlp), generic page analyzer
- 🔜 Archive extraction (RAR, ZIP, 7z, tar.gz)
- 🔜 Download queue
- 🔜 Speed limiter
- 🔜 HLS/DASH stream support

## Architecture

```
darkdm descargar <url>
  ↓
DownloadEngine::probe()
  ↓ ProbeResult (size, resumable, filename)
  ↓
DownloadEngine::download()
  ↓
PieceManager::init_single_piece(size)
  ↓
spawn PieceWorker (Range: bytes=0-N)
  ↓
PieceCallback::on_piece_complete()
  ↓
PieceManager::try_create_piece()
  ↓ split largest active piece
  ↓
spawn new PieceWorker (Range: bytes=M-N)
  ↓
repeat until all pieces complete
```

## Tests

```bash
cargo test --lib
```

16 tests covering:
- Piece: split, progress, atomic updates
- ProbeResult: header parsing, Content-Disposition
- TransactedIO: crash-safe state, 3-file rotation
- PieceManager: dynamic splitting, retry failed, max active
- DownloadEngine: probe
- PieceWorker: probe

## Examples

### Probe httpbin.org

```bash
$ darkdm info "https://httpbin.org/bytes/1024"
📡 DarkDM — Probing URL
URL: https://httpbin.org/bytes/1024

✓ Probe Result:
  Filename: 1024
  Size: 1024 bytes (0.00 MB)
  Resumable: no
  Content-Type: application/octet-stream
  Final URL: https://httpbin.org/bytes/1024
```

### Download with 16 workers

```bash
darkdm descargar "https://cdn.example.com/large-file.zip" --threads 16
```

## Comparison with bash scripts

| Feature | darkdm-mediafire (bash) | darkdm (Rust) |
|---------|------------------------|---------------|
| Multi-thread | ❌ | ✅ (8 workers default) |
| Resume | ❌ (curl -C fragile) | ✅ (TransactedIO) |
| Progress | ❌ (curl bar only) | ✅ (indicatif) |
| Dynamic splitting | ❌ | ✅ (work-stealing) |
| Crash-safe state | ❌ | ✅ (3-file rotation) |
| Dependencies | curl, unrar, 7z | None (static binary) |
| Testing | Manual | 16 unit tests |

## License

MIT
