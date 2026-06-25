// ProbeResult — metadata from initial HEAD/GET request
//
// XDM reference: HTTPDownloaderBase.cs (ProbeResult struct)



/// Result of probing a download URL
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Total resource size in bytes (from Content-Length)
    pub resource_size: Option<u64>,
    
    /// Whether server supports Range requests (Accept-Ranges: bytes)
    pub resumable: bool,
    
    /// Suggested filename (from Content-Disposition or URL)
    pub filename: Option<String>,
    
    /// Content type (from Content-Type header)
    pub content_type: Option<String>,
    
    /// Final URL after redirects
    pub final_url: String,
    
    /// Whether this is a text redirect (Content-Type: text/plain with URL in body)
    pub is_text_redirect: bool,
    
    /// Redirect target URL (if is_text_redirect)
    pub redirect_url: Option<String>,
}

impl ProbeResult {
    /// Parse ProbeResult from HTTP response headers
    pub fn from_headers(
        headers: &reqwest::header::HeaderMap,
        final_url: String,
    ) -> Self {
        let resource_size = headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        
        let resumable = headers
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains("bytes"))
            .unwrap_or(false);
        
        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        
        let filename = Self::extract_filename(headers, &final_url);
        
        let is_text_redirect = content_type
            .as_ref()
            .map(|ct| ct.starts_with("text/plain"))
            .unwrap_or(false);
        
        Self {
            resource_size,
            resumable,
            filename,
            content_type,
            final_url,
            is_text_redirect,
            redirect_url: None,
        }
    }
    
    /// Extract filename from Content-Disposition or URL
    fn extract_filename(
        headers: &reqwest::header::HeaderMap,
        url: &str,
    ) -> Option<String> {
        // Try Content-Disposition first
        if let Some(cd) = headers.get(reqwest::header::CONTENT_DISPOSITION) {
            if let Ok(cd_str) = cd.to_str() {
                // Parse: attachment; filename="file.mp4"
                // or: attachment; filename*=UTF-8''file.mp4
                for part in cd_str.split(';') {
                    let part = part.trim();
                    if part.starts_with("filename=") {
                        let filename = part
                            .trim_start_matches("filename=")
                            .trim_matches('"')
                            .to_string();
                        return Some(filename);
                    }
                    if part.starts_with("filename*=") {
                        // RFC 5987 encoded filename
                        let encoded = part.trim_start_matches("filename*=");
                        if let Some(filename) = encoded.split("''").nth(1) {
                            return Some(
                                urlencoding::decode(filename)
                                    .ok()?
                                    .to_string()
                            );
                        }
                    }
                }
            }
        }
        
        // Fallback: extract from URL path
        url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|segments| segments.last())
                    .filter(|s| !s.is_empty())
                    .and_then(|s| urlencoding::decode(s).ok().map(|d| d.to_string()))
            })
    }
    
    /// Get filename with fallback to "download"
    pub fn filename_or_default(&self) -> String {
        self.filename
            .clone()
            .unwrap_or_else(|| "download".to_string())
    }
    
    /// Check if this is a valid download (has size or is streamable)
    pub fn is_valid(&self) -> bool {
        self.resource_size.is_some() || !self.is_text_redirect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, ACCEPT_RANGES, CONTENT_DISPOSITION};
    
    #[test]
    fn test_probe_basic() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("1024"));
        headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        
        let probe = ProbeResult::from_headers(
            &headers,
            "https://example.com/file.mp4".to_string(),
        );
        
        assert_eq!(probe.resource_size, Some(1024));
        assert!(probe.resumable);
        assert_eq!(probe.filename, Some("file.mp4".to_string()));
    }
    
    #[test]
    fn test_probe_content_disposition() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"attachment; filename="video.mp4""#),
        );
        
        let probe = ProbeResult::from_headers(
            &headers,
            "https://example.com/download?id=123".to_string(),
        );
        
        assert_eq!(probe.filename, Some("video.mp4".to_string()));
    }
    
    #[test]
    fn test_probe_no_resume() {
        let headers = HeaderMap::new();
        
        let probe = ProbeResult::from_headers(
            &headers,
            "https://example.com/stream.m3u8".to_string(),
        );
        
        assert!(!probe.resumable);
        assert_eq!(probe.filename, Some("stream.m3u8".to_string()));
    }
}
