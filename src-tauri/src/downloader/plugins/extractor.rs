// Site extractor trait — plugins for site-specific URL extraction
//
// XDM reference: IExtractor interface (plugin system)

use async_trait::async_trait;
use url::Url;

/// Detected download link from a page
#[derive(Debug, Clone)]
pub struct DetectedLink {
    pub url: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
}

impl DetectedLink {
    /// Create a direct download link
    pub fn direct(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            filename: None,
            size: None,
        }
    }
    
    /// Create a link with metadata
    pub fn with_metadata(url: impl Into<String>, filename: Option<String>, size: Option<u64>) -> Self {
        Self {
            url: url.into(),
            filename,
            size,
        }
    }
}

/// Extraction error
#[derive(Debug)]
pub enum ExtractError {
    Network(String),
    Parse(String),
    NotFound(String),
}

impl ExtractError {
    pub fn network(e: impl std::fmt::Display) -> Self {
        Self::Network(e.to_string())
    }
    
    pub fn parse(e: impl std::fmt::Display) -> Self {
        Self::Parse(e.to_string())
    }
    
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "Network error: {}", e),
            Self::Parse(e) => write!(f, "Parse error: {}", e),
            Self::NotFound(e) => write!(f, "Not found: {}", e),
        }
    }
}

impl std::error::Error for ExtractError {}

/// Site extractor trait
#[async_trait]
pub trait SiteExtractor: Send + Sync {
    /// Plugin ID (unique identifier)
    fn id(&self) -> &'static str;
    
    /// Plugin name (human-readable)
    fn name(&self) -> &'static str;
    
    /// Priority (higher = checked first)
    fn priority(&self) -> u8;
    
    /// Check if this extractor can handle the URL
    fn can_handle(&self, url: &Url) -> bool;
    
    /// Extract download links from the page
    async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError>;
}
