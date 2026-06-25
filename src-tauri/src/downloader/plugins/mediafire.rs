// MediaFire extractor — extract direct download link from MediaFire page
//
// Reference: darkdm-mediafire bash script

use async_trait::async_trait;
use url::Url;
use super::extractor::{SiteExtractor, DetectedLink, ExtractError};

pub struct MediaFireExtractor;

impl MediaFireExtractor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SiteExtractor for MediaFireExtractor {
    fn id(&self) -> &'static str {
        "mediafire"
    }
    
    fn name(&self) -> &'static str {
        "MediaFire"
    }
    
    fn priority(&self) -> u8 {
        100
    }
    
    fn can_handle(&self, url: &Url) -> bool {
        url.host_str()
            .map(|h| h.contains("mediafire.com"))
            .unwrap_or(false)
    }
    
    async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        // Fetch page with gzip decompression
        let client = reqwest::Client::builder()
            .gzip(true)
            .build()
            .map_err(|e| ExtractError::network(e))?;
        
        let html = client
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| ExtractError::network(e))?
            .text()
            .await
            .map_err(|e| ExtractError::network(e))?;
        
        // Parse HTML
        let doc = scraper::Html::parse_document(&html);
        
        // Pattern 1: #downloadButton
        let selector = scraper::Selector::parse("#downloadButton")
            .map_err(|e| ExtractError::parse(format!("Invalid selector: {:?}", e)))?;
        
        if let Some(btn) = doc.select(&selector).next() {
            if let Some(href) = btn.value().attr("href") {
                return Ok(vec![DetectedLink::direct(href)]);
            }
        }
        
        // Pattern 2: download*.mediafire.com links
        let selector = scraper::Selector::parse("a[href*='download']")
            .map_err(|e| ExtractError::parse(format!("Invalid selector: {:?}", e)))?;
        
        for link in doc.select(&selector) {
            if let Some(href) = link.value().attr("href") {
                if href.contains("download") && href.contains("mediafire.com") {
                    return Ok(vec![DetectedLink::direct(href)]);
                }
            }
        }
        
        Err(ExtractError::not_found("No download button found on MediaFire page"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_can_handle() {
        let extractor = MediaFireExtractor::new();
        
        let url = Url::parse("https://www.mediafire.com/file/abc123/test.zip").unwrap();
        assert!(extractor.can_handle(&url));
        
        let url = Url::parse("https://download1350.mediafire.com/xyz/test.zip").unwrap();
        assert!(extractor.can_handle(&url));
        
        let url = Url::parse("https://example.com/file.zip").unwrap();
        assert!(!extractor.can_handle(&url));
    }
    
    #[test]
    fn test_metadata() {
        assert_eq!(MediaFireExtractor::new().id(), "mediafire");
        assert_eq!(MediaFireExtractor::new().name(), "MediaFire");
        assert_eq!(MediaFireExtractor::new().priority(), 100);
    }
}
