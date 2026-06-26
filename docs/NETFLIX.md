# Netflix Downloads with DarkDM

> **⚠️ TESTING BRANCH:** `feat/netflix`. No mergeado a `main`.
> **⚠️ yt-dlp NO soporta Netflix** (no está entre sus 1872 extractores).

---

## How Netflix Streaming Really Works

Netflix no sirve archivos MP4 completos. Usa **byte-range requests** sobre archivos gigantes en sus CDNs.

### Two URL Formats Discovered

#### Type A: MP4 Segment (nflxso.net)
```
https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4
  ?v=1              ← API version
  &e=1782452026     ← Expiry (Unix timestamp)
  &t=8OjWXQoPBaR... ← HMAC token
```
- Archivos MP4 completos (~5-15 MB)
- Soporta Range requests (multi-threaded download)
- Usado para trailers, clips, o segmentos DASH

#### Type B: Byte-Range Chunk (nflxvideo.net) ← **The Real Deal**
```
https://ipv4-c011-scl001-mundopacifico-isp.1.oca.nflxvideo.net
  /range/480165810-480700689        ← Byte range embebido en path
    ?o=1
    &v=49
    &e=1782451739                    ← Expiry
    &t=cCIsqaTKklHGjMzNnGhxPoLt...  ← HMAC token (único por rango)
    &sc=Eq%27(...)                   ← Security context (binario)
```
- **Content-Type: application/octet-stream** (raw bytes, no MP4)
- **535 KB por request** típicamente
- **Individualmente firmado** — cada rango tiene su propio token
- **no-store** cache (seguridad)
- **freenginx** (Netflix custom nginx)
- **X-TCP-Info** header con telemetría interna del CDN

### Architecture Diagram

```
Single large video file on CDN (1-5 GB)
│
├── Byte 0-500000        → /range/0-500000?t=token1
├── Byte 480165810-...   → /range/480165810-480700689?t=token2
├── Byte 900000000-...   → /range/900000000-900500000?t=token3
│
└── Each /range/X-Y URL = ONE-TIME-USE
    Token t=... is cryptographically tied to exact X-Y range
    Can NOT modify range — different range = different token needed
```

When you press play, the browser makes hundreds of these range requests,
each ~500 KB, covering the video as it plays.

---

## Capturing a Full Video

### Method 1: Browser DevTools (recommended)

1. Open Netflix in Chrome/Firefox
2. F12 → Network tab
3. Filter: `nflxvideo.net` or `/range/`
4. **Press play and let the video play through**
5. (Optional) Seek through the video to trigger all ranges
6. Copy all URLs → save to `netflix-urls.txt`
7. Download with DarkDM:

```bash
darkdm batch netflix-urls.txt --concat --name "movie.mp4"
```

### Method 2: IDM on Windows

IDM captures these URLs automatically during playback. Export all captured URLs
and transfer to Linux for batch download.

### One URL ≠ Full Video

A single URL gives you only **one piece** (~500 KB). A full movie needs
**thousands** of these URLs. Always use `darkdm batch` for complete videos.

---

## Usage

### Single Segment (testing)
```bash
darkdm descargar "https://occ-0-*.nflxso.net/...mp4?v=1&e=*&t=*"
darkdm descargar "https://*.nflxvideo.net/range/X-Y?o=1&v=49&e=*&t=*"
```

### Batch Download (full video)
```bash
# Prepare URLs file
cat urls.txt
https://ipv4-*.nflxvideo.net/range/0-500000?o=1&v=49&e=...
https://ipv4-*.nflxvideo.net/range/500001-1000000?o=1&v=49&e=...
...

# Download all + concatenate
darkdm batch urls.txt --concat --name "movie.mp4"

# Download only (no concat)
darkdm batch urls.txt --threads 8
```

### Probe URL
```bash
darkdm info "https://*.nflxvideo.net/range/X-Y?o=1&v=49&e=*&t=*"
```

---

## Verified Performance

### Type A: MP4 Segment (nflxso.net)

| Metric | Value |
|--------|-------|
| File size | 9.17 MB (9,618,121 bytes) |
| Workers | 8 |
| Download time | ~9 seconds |
| Speed | 10.1 MiB/s |
| Resume | ✅ Supported |
| Format | video/mp4 |

### Type B: Byte-Range Chunk (nflxvideo.net)

| Metric | Value |
|--------|-------|
| Chunk size | 535 KB (534,880 bytes) |
| Workers | 1 (single-thread, range in URL) |
| Download time | < 1 second |
| Speed | Direct |
| Resume | N/A (tiny file) |
| Format | application/octet-stream |

---

## Token Expiry

The `e=` parameter is a Unix timestamp:

```bash
# Decode
$ date -d @1782451739
Thu Jul  9 2026

# Check if URL still valid
$ curl -sI "https://...nflxvideo.net/range/X-Y?o=1&v=49&e=1782451739&t=..."
```

Once expired → `403 Forbidden` or `404 Not Found`.
Tokens typically last **14-30 days**.

---

## CDN Hosts

| Host | Type | Purpose |
|------|------|---------|
| `*.nflxso.net` | Type A | MP4 segments, trailers |
| `*.nflxvideo.net` | Type B | Byte-range chunks (main streaming) |
| `*.nflxext.com` | Extras | Thumbnails, metadata |

Server: `freenginx` (Netflix custom nginx)

---

## Known Limitations

| Limitation | Impact |
|------------|--------|
| **Token expiry** | URLs last 14-30 days only |
| **Individual signing** | Each /range/X-Y needs unique token |
| **Capture required** | Must play video to get all URLs |
| **No yt-dlp support** | Can't use Netflix page URL directly |
| **No DRM bypass** | Widevine protected content not supported |
| **Many URLs** | Full movie = thousands of range requests |
| **Testing branch** | On `feat/netflix`, not in `main` |

---

## Batch Command Reference

```bash
darkdm batch --help

Usage: darkdm batch [OPTIONS] <FILE>

Arguments:
  <FILE>  File containing URLs (one per line)

Options:
  -o, --output <DIR>    Output directory
  -t, --threads <N>     Workers per file [default: 4]
  -c, --concat          Concat all files with ffmpeg
  -n, --name <NAME>     Output filename (requires --concat)
  -v, --verbose         Debug logging
```

---

## Full Capture Workflow

```bash
# 1. Capture URLs from browser DevTools
#    Filter: nflxvideo.net or /range/
#    Copy all → paste into urls.txt

# 2. Count them
wc -l urls.txt
# 2456 URLs for a typical movie

# 3. Download all
darkdm batch urls.txt \
  --output ~/Videos/Netflix \
  --threads 8 \
  --concat \
  --name "The.Movie.2026.mp4"

# 4. Clean up segments (optional)
rm ~/Videos/Netflix/range-*
```

---

## Testing

```bash
# On feat/netflix branch
git checkout feat/netflix
cd src-tauri

# Build
cargo build --release --bin darkdm

# Run tests
cargo test --lib

# Test single segment
./target/release/darkdm descargar "https://*.nflxvideo.net/range/X-Y?..."

# Test batch
./target/release/darkdm batch urls.txt --threads 4
```

---

## Future Improvements

- [ ] Auto-capture via Chrome extension (capture all /range/ requests)
- [ ] Parallel batch download (multi-file concurrent)
- [ ] Smart concatenation (detect gaps, re-order by range)
- [ ] Token expiry checker before batch
- [ ] Resume for large batches
- [ ] Merge to `main` after stable testing
