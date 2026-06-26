// Netflix CDN URL extractor
//
// ⚠️ CRITICAL yt-dlp does NOT support Netflix (not in its 1872 extractors).
//    Each segment/range has a UNIQUE token — can't guess sequential IDs.
//
// Architecture (reverse-engineered from live captures):
//
//   Two URL formats detected:
//
//   Type A — MP4 segment (small files, ~5-15 MB)
//     https://*.nflxso.net/so/soa7/<id>.mp4?v=1&e=<exp>&t=<hmac>
//     Content-Type: video/mp4
//     Full small files (trailers, clips)
//
//   Type B — Byte-range chunk (the real streaming mechanism)
//     https://*.nflxvideo.net/range/<start>-<end>?o=1&v=49&e=<exp>&t=<hmac>&sc=<ctx>
//     Content-Type: application/octet-stream
//     Each URL = specific byte range of a large video file (~500 KB each)
//     Player requests these as it needs them (seeking, buffering)
//
// CDN servers:
//   freenginx (Netflix custom nginx)
//   Cache-Control: no-store (security)
//   X-TCP-Info header with internal CDN telemetry
//
// Capture workflow (to get full video):
//   1. Play Netflix video in Chrome/Firefox
//   2. DevTools → Network tab → filter "nflxvideo.net" or "/range/"
//   3. Copy ALL range URLs as they load during playback
//   4. Save to file → darkdm batch urls.txt --concat --name movie.mp4
//
// Limitations:
//   - Each /range/ URL is ONE-TIME-USE (token tied to specific byte range)
//   - Each ~500 KB — a full movie needs hundreds of these
//   - No way to get full file without capturing all range URLs
//   - yt-dlp does NOT support Netflix (not a supported extractor)

use crate::downloader::plugins::{SiteExtractor, DetectedLink, ExtractError};
use async_trait::async_trait;
use url::Url;

/// Netflix CDN host patterns
///
/// Type A (.mp4):  *.nflxso.net
/// Type B (/range/): *.nflxvideo.net  (primary streaming, byte-range requests)
/// Extras:         *.nflxext.com
const NETFLIX_CDN_HOSTS: &[&str] = &[
    "nflxso.net",
    "nflxvideo.net",
    "nflxext.com",
];

/// NetflixExtractor — handles Netflix CDN URLs
///
/// Direct CDN URLs are passed through as-is (they work with the engine).
/// Page URLs (netflix.com/watch/*) are NOT supported — yt-dlp can't handle Netflix.
/// Use browser DevTools to capture segment URLs, save to file, and batch download.
pub struct NetflixExtractor;

impl NetflixExtractor {
    pub fn new() -> Self {
        Self
    }
    
    /// Check if URL is a Netflix CDN URL
    fn is_cdn_url(url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        NETFLIX_CDN_HOSTS.iter().any(|cdn| host.ends_with(*cdn))
    }
}

#[async_trait]
impl SiteExtractor for NetflixExtractor {
    fn id(&self) -> &'static str {
        "netflix"
    }
    
    fn name(&self) -> &'static str {
        "Netflix CDN"
    }
    
    fn priority(&self) -> u8 {
        90 // Below YouTube (95), above generic page analyzer (10)
    }
    
    fn can_handle(&self, url: &Url) -> bool {
        Self::is_cdn_url(url)
    }
    
    async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        tracing::info!("NetflixExtractor handling CDN URL: {}", url);
        
        // Extract filename from URL path
        let filename = url.path_segments()
            .and_then(|segments| segments.last())
            .map(|s| s.split('?').next().unwrap_or(s).to_string());
        
        Ok(vec![DetectedLink::with_metadata(
            url.as_str(),
            filename,
            None, // size unknown until probe
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_netflix_cdn() {
        let urls = vec![
            "https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4?v=1&e=1782452026&t=8OjWXQoPBaR5ja_X4WW92TNMRGk",
            "https://test.nflxvideo.net/video.mp4?e=1234567890&t=token",
            "https://cdn.nflxext.com/asset.mpd",
        ];
        
        for url_str in urls {
            let url = Url::parse(url_str).unwrap();
            assert!(NetflixExtractor::is_cdn_url(&url), "Should detect: {}", url_str);
        }
    }
    
    #[test]
    fn test_detect_netflix_page() {
        // Netflix page URLs are NOT handled by this extractor
        // yt-dlp doesn't support Netflix
        let url = Url::parse("https://www.netflix.com/watch/81280744").unwrap();
        assert!(!NetflixExtractor::is_cdn_url(&url), "Should NOT detect page URLs");
    }
    
    #[test]
    fn test_reject_non_netflix() {
        let urls = vec![
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://example.com/file.mp4",
            "https://mediafire.com/file/test.rar",
        ];
        
        for url_str in urls {
            let url = Url::parse(url_str).unwrap();
            assert!(!NetflixExtractor::is_cdn_url(&url), "Should reject: {}", url_str);
        }
    }
    
    #[test]
    fn test_filename_extraction() {
        let url = Url::parse("https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/1684335598036192513.mp4?v=1&e=1782452026&t=token").unwrap();
        
        let filename = url.path_segments()
            .and_then(|segments| segments.last())
            .map(|s| s.split('?').next().unwrap_or(s).to_string());
        
        assert_eq!(filename, Some("1684335598036192513.mp4".to_string()));
    }
    
    #[tokio::test]
    async fn test_cdn_url_returns_as_is() {
        let extractor = NetflixExtractor::new();
        let url = Url::parse("https://occ-0-3967-1740.1.nflxso.net/so/soa7/717/test.mp4?v=1&e=1782452026&t=token").unwrap();
        
        let result = extractor.extract(&url).await;
        assert!(result.is_ok());
        
        let links = result.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, url.as_str());
        assert_eq!(links[0].filename, Some("test.mp4".to_string()));
    }
}
