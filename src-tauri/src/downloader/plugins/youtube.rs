// YouTube extractor — uses yt-dlp to extract video URLs
//
// Reference: yt-dlp integration

use async_trait::async_trait;
use url::Url;
use super::extractor::{SiteExtractor, DetectedLink, ExtractError};
use std::process::Command;

pub struct YouTubeExtractor;

impl YouTubeExtractor {
    pub fn new() -> Self {
        Self
    }
    
    /// Check if yt-dlp is installed
    fn check_ytdlp() -> bool {
        Command::new("yt-dlp")
            .arg("--version")
            .output()
            .is_ok()
    }
}

#[async_trait]
impl SiteExtractor for YouTubeExtractor {
    fn id(&self) -> &'static str {
        "youtube"
    }
    
    fn name(&self) -> &'static str {
        "YouTube (yt-dlp)"
    }
    
    fn priority(&self) -> u8 {
        95
    }
    
    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        host.contains("youtube.com") || 
        host.contains("youtu.be") ||
        host.contains("vimeo.com") ||
        host.contains("tiktok.com") ||
        host.contains("twitter.com") ||
        host.contains("instagram.com")
    }
    
    async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        // Check if yt-dlp is installed
        if !Self::check_ytdlp() {
            return Err(ExtractError::not_found(
                "yt-dlp not found. Install with: pip install yt-dlp"
            ));
        }
        
        // Run yt-dlp to get direct URL
        let output = Command::new("yt-dlp")
            .arg("--get-url")
            .arg("--format")
            .arg("best")
            .arg(url.as_str())
            .output()
            .map_err(|e| ExtractError::network(format!("Failed to run yt-dlp: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ExtractError::parse(format!("yt-dlp failed: {}", stderr)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let direct_url = stdout.trim();
        
        if direct_url.is_empty() {
            return Err(ExtractError::not_found("No video URL found"));
        }
        
        // Get video title
        let title_output = Command::new("yt-dlp")
            .arg("--get-title")
            .arg(url.as_str())
            .output()
            .ok();
        
        let title = title_output
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());
        
        Ok(vec![DetectedLink::with_metadata(
            direct_url.to_string(),
            title,
            None,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_can_handle() {
        let extractor = YouTubeExtractor::new();
        
        let url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert!(extractor.can_handle(&url));
        
        let url = Url::parse("https://youtu.be/dQw4w9WgXcQ").unwrap();
        assert!(extractor.can_handle(&url));
        
        let url = Url::parse("https://vimeo.com/123456").unwrap();
        assert!(extractor.can_handle(&url));
        
        let url = Url::parse("https://example.com/video.mp4").unwrap();
        assert!(!extractor.can_handle(&url));
    }
    
    #[test]
    fn test_metadata() {
        assert_eq!(YouTubeExtractor::new().id(), "youtube");
        assert_eq!(YouTubeExtractor::new().name(), "YouTube (yt-dlp)");
        assert_eq!(YouTubeExtractor::new().priority(), 95);
    }
}
