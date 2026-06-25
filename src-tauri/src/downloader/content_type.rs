// Content-Type detection — determine if URL is downloadable or needs extraction
//
// Reference: IDM content-type detection logic

/// Check if content-type indicates a downloadable file
pub fn is_downloadable(content_type: &str) -> bool {
    let ct = content_type.to_lowercase();
    
    // Video formats
    if ct.starts_with("video/") {
        return true;
    }
    
    // Audio formats
    if ct.starts_with("audio/") {
        return true;
    }
    
    // Application formats (archives, documents, binaries)
    if ct.starts_with("application/") {
        // Exclude HTML-like application types
        if ct.contains("html") || ct.contains("xhtml") || ct.contains("xml") {
            return false;
        }
        return true;
    }
    
    // Image formats
    if ct.starts_with("image/") {
        return true;
    }
    
    // Binary/octet-stream
    if ct.contains("octet-stream") {
        return true;
    }
    
    // Default: not downloadable (probably HTML/text)
    false
}

/// Check if content-type indicates a page that needs extraction
pub fn needs_extraction(content_type: &str) -> bool {
    let ct = content_type.to_lowercase();
    
    ct.starts_with("text/html") || 
    ct.contains("html") ||
    ct.starts_with("text/plain")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_downloadable() {
        // Video
        assert!(is_downloadable("video/mp4"));
        assert!(is_downloadable("video/x-matroska"));
        assert!(is_downloadable("video/webm"));
        
        // Audio
        assert!(is_downloadable("audio/mpeg"));
        assert!(is_downloadable("audio/mp4"));
        
        // Application
        assert!(is_downloadable("application/zip"));
        assert!(is_downloadable("application/pdf"));
        assert!(is_downloadable("application/octet-stream"));
        
        // NOT downloadable
        assert!(!is_downloadable("text/html"));
        assert!(!is_downloadable("text/plain"));
        assert!(!is_downloadable("application/xhtml+xml"));
    }
    
    #[test]
    fn test_needs_extraction() {
        assert!(needs_extraction("text/html"));
        assert!(needs_extraction("text/html; charset=utf-8"));
        assert!(needs_extraction("application/xhtml+xml"));
        
        assert!(!needs_extraction("video/mp4"));
        assert!(!needs_extraction("application/zip"));
    }
}
