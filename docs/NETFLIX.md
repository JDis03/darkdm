# Netflix Downloads with DarkDM

> **⚠️ TESTING BRANCH:** This is on the `feat/netflix` branch. Netflix CDN URLs are time-limited and require browser session cookies to capture.

## How It Works

Netflix streams video via **token-authenticated CDN URLs**. These URLs contain:

```
https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4
  ?v=1              # API version
  &e=1782452026     # Expiry (Unix timestamp)
  &t=8OjWXQoPBaR... # HMAC auth token
```

**Key facts:**
- Direct MP4 files (not HLS/DASH for downloads)
- Support **Range requests** → multi-threaded download works
- Time-limited tokens (typically 14-30 days from issue)
- **No special headers needed** — URL alone is sufficient
- Server: `freenginx` (Netflix custom nginx)

## Capturing Netflix URLs

### Method 1: IDM (Internet Download Manager) — Windows
1. Play Netflix in browser
2. IDM detects the video automatically
3. Copy the CDN URL from IDM's download dialog
4. Send to Linux machine and use with DarkDM

### Method 2: Chrome Extension (coming soon)
1. Enable DarkDM Chrome extension
2. Play Netflix video
3. Extension captures `<video>` source URL
4. Sends to DarkDM native host for download

### Method 3: Browser Developer Tools
```javascript
// In browser console while Netflix is playing:
let video = document.querySelector('video');
console.log(video.src);
// Or check Network tab → filter by .mp4 or nflxso
```

## Usage

### Direct CDN URL (captured from browser)
```bash
darkdm descargar "https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4?v=1&e=1782452026&t=8OjWXQoPBaR5ja_X4WW92TNMRGk"
```

### Netflix Page URL (requires cookies)
```bash
# Netflix page URL — needs cookies.txt from browser
darkdm descargar "https://www.netflix.com/watch/81280744"
```

To use page URLs, export cookies from your browser:
1. Install a cookies.txt export extension
2. Log into Netflix
3. Export cookies to `netflix_cookies.txt`
4. Pass to DarkDM (coming soon: `--cookies` flag)

### Probe URL
```bash
darkdm info "https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4?v=1&e=1782452026&t=8OjWXQoPBaR5ja_X4WW92TNMRGk"
```

## Verified Performance

Tested with a 9.17 MB Netflix clip:

| Metric | Value |
|--------|-------|
| File size | 9.17 MB (9,618,121 bytes) |
| Workers | 8 |
| Download time | ~9 seconds |
| Speed | 10.1 MiB/s |
| Resume | ✅ Supported |
| Format | video/mp4 |
| Server | freenginx (Netflix) |

## Token Expiry

The `e=` parameter in the URL is a Unix timestamp. Once expired:
- Server returns `403 Forbidden` or `404 Not Found`
- Must capture a fresh URL from a Netflix browser session
- Tokens typically last **14-30 days**

Check expiry:
```bash
# Decode the Unix timestamp
$ date -d @1782452026
Thu Jul  9 2026
```

## CDN Hosts

Netflix uses multiple CDN hosts — all supported:

| Host | Purpose |
|------|---------|
| `*.nflxso.net` | Main streaming CDN |
| `*.nflxvideo.net` | Video content CDN |
| `*.nflxext.com` | Extras (thumbnails, metadata) |

## Limitations

- ⚠️ **Token expiry** — URLs are time-limited
- ⚠️ **Session required** — Need active Netflix session to capture URLs
- ⚠️ **DRM content** — Some content may be DRM-protected (Widevine)
- ⚠️ **No automatic extraction** — Can't extract from page URL without cookies
- ⚠️ **Testing branch** — On `feat/netflix`, not merged to `main`

## Testing

```bash
# Switch to netflix branch
git checkout feat/netflix

# Build
cd src-tauri && cargo build --release --bin darkdm

# Test with captured CDN URL
darkdm descargar "https://occ-0-*-nflxso.net/*.mp4?v=1&e=*&t=*"

# Run tests
cargo test --lib
```

## Future Improvements

- [ ] `--cookies` flag for page URL extraction
- [ ] Automatic token refresh via browser extension
- [ ] Multi-episode batch download
- [ ] DRM license handling (Widevine)
- [ ] Merge to `main` after testing
