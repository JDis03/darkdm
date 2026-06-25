// Plugin registry — manages site extractors
//
// XDM reference: ExtractorRegistry pattern

use std::sync::Arc;
use url::Url;
use super::extractor::{SiteExtractor, DetectedLink, ExtractError};

pub struct ExtractorRegistry {
    extractors: Vec<Arc<dyn SiteExtractor>>,
}

impl ExtractorRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }
    
    /// Register an extractor
    pub fn register(&mut self, extractor: Arc<dyn SiteExtractor>) {
        self.extractors.push(extractor);
        // Sort by priority (highest first)
        self.extractors.sort_by(|a, b| b.priority().cmp(&a.priority()));
    }
    
    /// Find an extractor that can handle the URL
    pub fn find_extractor(&self, url: &Url) -> Option<Arc<dyn SiteExtractor>> {
        self.extractors
            .iter()
            .find(|e| e.can_handle(url))
            .cloned()
    }
    
    /// Try to extract download links from a URL
    pub async fn extract(&self, url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
        if let Some(extractor) = self.find_extractor(url) {
            extractor.extract(url).await
        } else {
            Err(ExtractError::not_found("No extractor found for this URL"))
        }
    }
    
    /// List all registered extractors
    pub fn list_extractors(&self) -> Vec<(&'static str, &'static str, u8)> {
        self.extractors
            .iter()
            .map(|e| (e.id(), e.name(), e.priority()))
            .collect()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    
    struct TestExtractor {
        id: &'static str,
        priority: u8,
    }
    
    #[async_trait]
    impl SiteExtractor for TestExtractor {
        fn id(&self) -> &'static str { self.id }
        fn name(&self) -> &'static str { "Test" }
        fn priority(&self) -> u8 { self.priority }
        fn can_handle(&self, url: &Url) -> bool {
            url.host_str().map(|h| h == "test.com").unwrap_or(false)
        }
        async fn extract(&self, _url: &Url) -> Result<Vec<DetectedLink>, ExtractError> {
            Ok(vec![DetectedLink::direct("http://test.com/file.zip")])
        }
    }
    
    #[test]
    fn test_registry_priority() {
        let mut registry = ExtractorRegistry::new();
        
        registry.register(Arc::new(TestExtractor { id: "low", priority: 10 }));
        registry.register(Arc::new(TestExtractor { id: "high", priority: 100 }));
        registry.register(Arc::new(TestExtractor { id: "mid", priority: 50 }));
        
        let list = registry.list_extractors();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, "high");
        assert_eq!(list[1].0, "mid");
        assert_eq!(list[2].0, "low");
    }
    
    #[tokio::test]
    async fn test_registry_extract() {
        let mut registry = ExtractorRegistry::new();
        registry.register(Arc::new(TestExtractor { id: "test", priority: 100 }));
        
        let url = Url::parse("http://test.com/page").unwrap();
        let links = registry.extract(&url).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "http://test.com/file.zip");
    }
}
