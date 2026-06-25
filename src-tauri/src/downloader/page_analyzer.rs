// Page Analyzer — detect downloadable resources in HTML pages
//
// Reference: IDM page analysis (detects videos, audios, downloads in any page)

use scraper::{Html, Selector};
use url::Url;

/// Detected resource in a page
#[derive(Debug, Clone)]
pub struct DetectedResource {
    pub url: String,
    pub resource_type: ResourceType,
    pub size: Option<u64>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResourceType {
    Video,
    Audio,
    Document,
    Archive,
    Unknown,
}

impl DetectedResource {
    pub fn new(url: String, resource_type: ResourceType) -> Self {
        Self {
            url,
            resource_type,
            size: None,
            title: None,
        }
    }
}

/// Analyze HTML page and extract downloadable resources
pub fn analyze_page(html: &str, base_url: &Url) -> Vec<DetectedResource> {
    let doc = Html::parse_document(html);
    let mut resources = Vec::new();
    
    // 1. <video> tags
    if let Ok(selector) = Selector::parse("video") {
        for element in doc.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = base_url.join(src) {
                    resources.push(DetectedResource::new(
                        absolute_url.to_string(),
                        ResourceType::Video,
                    ));
                }
            }
        }
    }
    
    // 2. <source> tags (inside video/audio)
    if let Ok(selector) = Selector::parse("source") {
        for element in doc.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = base_url.join(src) {
                    let url_str = absolute_url.to_string();
                    let resource_type = detect_type_from_url(&url_str);
                    resources.push(DetectedResource::new(url_str, resource_type));
                }
            }
        }
    }
    
    // 3. <audio> tags
    if let Ok(selector) = Selector::parse("audio") {
        for element in doc.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                if let Ok(absolute_url) = base_url.join(src) {
                    resources.push(DetectedResource::new(
                        absolute_url.to_string(),
                        ResourceType::Audio,
                    ));
                }
            }
        }
    }
    
    // 4. <a> links with video/audio/archive extensions
    if let Ok(selector) = Selector::parse("a[href]") {
        for element in doc.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if is_downloadable_extension(href) {
                    if let Ok(absolute_url) = base_url.join(href) {
                        let url_str = absolute_url.to_string();
                        let resource_type = detect_type_from_url(&url_str);
                        let title = element.text().collect::<String>().trim().to_string();
                        
                        let mut resource = DetectedResource::new(url_str, resource_type);
                        if !title.is_empty() {
                            resource.title = Some(title);
                        }
                        resources.push(resource);
                    }
                }
            }
        }
    }
    
    // 5. Detect HLS/DASH in page
    let text = doc.root_element().text().collect::<String>();
    for line in text.lines() {
        // HLS (.m3u8)
        if line.contains(".m3u8") {
            if let Some(url) = extract_url_from_text(line, ".m3u8") {
                if let Ok(absolute_url) = base_url.join(&url) {
                    resources.push(DetectedResource::new(
                        absolute_url.to_string(),
                        ResourceType::Video,
                    ));
                }
            }
        }
        
        // DASH (.mpd)
        if line.contains(".mpd") {
            if let Some(url) = extract_url_from_text(line, ".mpd") {
                if let Ok(absolute_url) = base_url.join(&url) {
                    resources.push(DetectedResource::new(
                        absolute_url.to_string(),
                        ResourceType::Video,
                    ));
                }
            }
        }
    }
    
    // Deduplicate
    resources.sort_by(|a, b| a.url.cmp(&b.url));
    resources.dedup_by(|a, b| a.url == b.url);
    
    resources
}

/// Check if URL has a downloadable extension
fn is_downloadable_extension(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    
    // Video extensions
    let video_exts = [".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v", ".mpg", ".mpeg"];
    for ext in &video_exts {
        if url_lower.contains(ext) {
            return true;
        }
    }
    
    // Audio extensions
    let audio_exts = [".mp3", ".m4a", ".aac", ".flac", ".wav", ".ogg", ".opus", ".wma"];
    for ext in &audio_exts {
        if url_lower.contains(ext) {
            return true;
        }
    }
    
    // Archive extensions
    let archive_exts = [".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz"];
    for ext in &archive_exts {
        if url_lower.contains(ext) {
            return true;
        }
    }
    
    // Document extensions
    let doc_exts = [".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx"];
    for ext in &doc_exts {
        if url_lower.contains(ext) {
            return true;
        }
    }
    
    false
}

/// Detect resource type from URL
fn detect_type_from_url(url: &str) -> ResourceType {
    let url_lower = url.to_lowercase();
    
    if url_lower.contains(".mp4") || url_lower.contains(".mkv") || 
       url_lower.contains(".avi") || url_lower.contains(".webm") ||
       url_lower.contains(".m3u8") || url_lower.contains(".mpd") {
        return ResourceType::Video;
    }
    
    if url_lower.contains(".mp3") || url_lower.contains(".m4a") || 
       url_lower.contains(".flac") || url_lower.contains(".wav") {
        return ResourceType::Audio;
    }
    
    if url_lower.contains(".zip") || url_lower.contains(".rar") || 
       url_lower.contains(".7z") || url_lower.contains(".tar") {
        return ResourceType::Archive;
    }
    
    if url_lower.contains(".pdf") || url_lower.contains(".doc") {
        return ResourceType::Document;
    }
    
    ResourceType::Unknown
}

/// Extract URL from text containing a specific extension
fn extract_url_from_text(text: &str, extension: &str) -> Option<String> {
    if let Some(pos) = text.find(extension) {
        // Find start of URL (look backwards for http/https or /)
        let before = &text[..pos];
        let start = before.rfind("http")
            .or_else(|| before.rfind("//"))
            .or_else(|| before.rfind('/'))
            .unwrap_or(0);
        
        // Find end of URL (look forward for quote, space, or comma)
        let after_ext = pos + extension.len();
        let end = text[after_ext..]
            .find(|c: char| c == '"' || c == '\'' || c == ' ' || c == ',' || c == ')')
            .map(|i| after_ext + i)
            .unwrap_or(text.len());
        
        let url = text[start..end].trim();
        if !url.is_empty() {
            return Some(url.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_video_tag() {
        let html = r#"<video src="/video.mp4"></video>"#;
        let base = Url::parse("https://example.com").unwrap();
        let resources = analyze_page(html, &base);
        
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].url, "https://example.com/video.mp4");
        assert_eq!(resources[0].resource_type, ResourceType::Video);
    }
    
    #[test]
    fn test_detect_source_tag() {
        let html = r#"<video><source src="movie.mp4" type="video/mp4"></video>"#;
        let base = Url::parse("https://example.com/page/").unwrap();
        let resources = analyze_page(html, &base);
        
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].url, "https://example.com/page/movie.mp4");
    }
    
    #[test]
    fn test_detect_download_links() {
        let html = r#"<a href="/files/video.mp4">Download Video</a>"#;
        let base = Url::parse("https://example.com").unwrap();
        let resources = analyze_page(html, &base);
        
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].url, "https://example.com/files/video.mp4");
        assert_eq!(resources[0].title, Some("Download Video".to_string()));
    }
    
    #[test]
    fn test_detect_hls() {
        let html = r#"<script>var url = "https://cdn.example.com/stream.m3u8";</script>"#;
        let base = Url::parse("https://example.com").unwrap();
        let resources = analyze_page(html, &base);
        
        assert!(resources.iter().any(|r| r.url.contains("stream.m3u8")));
    }
    
    #[test]
    fn test_is_downloadable_extension() {
        assert!(is_downloadable_extension("video.mp4"));
        assert!(is_downloadable_extension("/path/to/file.mkv"));
        assert!(is_downloadable_extension("https://example.com/audio.mp3"));
        assert!(is_downloadable_extension("archive.zip"));
        
        assert!(!is_downloadable_extension("page.html"));
        assert!(!is_downloadable_extension("style.css"));
    }
}
