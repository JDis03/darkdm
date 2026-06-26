// Netflix CDN URL extractor
//
// Netflix uses token-authenticated CDN URLs (nflxso.net, nflxvideo.net).
// Direct CDN URLs work as-is — no special headers needed.
// For page URLs, uses yt-dlp with Netflix cookies.
//
// Capturing CDN URLs:
//   Chrome extension intercepts <video> src on netflix.com
//   IDM on Windows captures nflxso.net URLs via browser integration
//   URL format: https://occ-*.nflxso.net/so/soa7/*.mp4?v=1&e=<expiry>&t=<token>
//
// CDN URL anatomy:
//   e=1782452026  — expiry Unix timestamp
//   t=...         — HMAC token (time-limited)
//   v=1           — API version
//   .mp4          — container (may also be .isml for HLS)

use crate::downloader::plugins::{SiteExtractor, DetectedLink, ExtractError};
use async_trait::async_trait;
use url::Url;

/// Netflix CDN host patterns
const NETFLIX_CDN_HOSTS: &[&str] = &[
    "nflxso.net",
    "nflxvideo.net",
    "nflxext.com",
];

/// NetflixExtractor — handles Netflix CDN URLs and page URLs
pub struct NetflixExtractor {
    /// Path to Netflix cookies.txt (optional)
    /// Export from browser extension: cookies.txt
    cookies_path: Option<String>,
}

impl NetflixExtractor {
    pub fn new() -> Self {
        Self { cookies_path: None }
    }
    
    /// Set cookies file path for page URL extraction
    pub fn with_cookies(mut self, path: impl Into<String>) -> Self {
        self.cookies_path = Some(path.into());
        self
    }
    
    /// Check if URL is a Netflix CDN URL
    fn is_cdn_url(url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        NETFLIX_CDN_HOSTS.iter().any(|cdn| host.ends_with(*cdn))
    }
    
    /// Check if URL is a Netflix page URL
    fn is_page_url(url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        host.ends_with("netflix.com")
    }
    
    /// Extract CDN URLs using yt-dlp with cookies
    async fn extract_with_ytdlp(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        // Build yt-dlp command
        let mut cmd = tokio::process::Command::new("yt-dlp");
        cmd.arg("--get-url")
            .arg("--format")
            .arg("best")
            .arg(url.as_str());
        
        // Add cookies if available
        if let Some(cookies) = &self.cookies_path {
            cmd.arg("--cookies").arg(cookies);
        }
        
        tracing::info!("Running yt-dlp for Netflix URL: {}", url);
        tracing::debug!("yt-dlp command: yt-dlp --get-url --format best {}", url);
        
        let output = cmd.output().await
            .map_err(|e| ExtractError::network(format!("yt-dlp not found: {}. Install with: pip install yt-dlp", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("yt-dlp failed: {}", stderr);
            return Err(ExtractError::network(format!(
                "yt-dlp extraction failed. Ensure Netflix cookies are valid.\n{}", 
                stderr
            )));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut links = Vec::new();
        
        for line in stdout.lines() {
            let line = line.trim();
            if !line.is_empty() {
                links.push(DetectedLink::direct(line));
            }
        }
        
        if links.is_empty() {
            return Err(ExtractError::not_found("No CDN URLs found from Netflix page. Ensure you have valid Netflix cookies."));
        }
        
        tracing::info!("yt-dlp found {} CDN URL(s)", links.len());
        Ok(links)
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
        90 // Below YouTube (95), above generic (10)
    }
    
    fn can_handle(&self, url: &Url) -> bool {
        Self::is_cdn_url(url) || Self::is_page_url(url)
    }
    
    async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        tracing::info!("NetflixExtractor handling: {}", url);
        
        // Direct CDN URL — pass through as-is
        if Self::is_cdn_url(url) {
            tracing::info!("Direct Netflix CDN URL detected — using as-is");
            
            // Extract filename from URL
            let filename = url.path_segments()
                .and_then(|segments| segments.last())
                .map(|s| {
                    // Remove query params from filename
                    s.split('?').next().unwrap_or(s).to_string()
                });
            
            return Ok(vec![DetectedLink::with_metadata(
                url.as_str(),
                filename,
                None, // size unknown until probe
            )]);
        }
        
        // Netflix page URL — use yt-dlp with cookies
        if Self::is_page_url(url) {
            tracing::info!("Netflix page URL detected — using yt-dlp extraction");
            return self.extract_with_ytdlp(url).await;
        }
        
        Err(ExtractError::not_found("Unknown Netflix URL format"))
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
        let urls = vec![
            "https://www.netflix.com/watch/81280744",
            "https://netflix.com/title/81280744",
        ];
        
        for url_str in urls {
            let url = Url::parse(url_str).unwrap();
            assert!(NetflixExtractor::is_page_url(&url), "Should detect: {}", url_str);
        }
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
            assert!(!NetflixExtractor::is_page_url(&url), "Should reject: {}", url_str);
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
