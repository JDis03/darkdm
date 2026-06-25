// Plugins — site-specific extractors (MediaFire, YouTube, etc.)
//
// XDM reference: IExtractor interface

pub mod extractor;
pub mod registry;
pub mod mediafire;
pub mod youtube;

// Re-exports
pub use extractor::{SiteExtractor, DetectedLink, ExtractError};
pub use registry::ExtractorRegistry;
pub use mediafire::MediaFireExtractor;
pub use youtube::YouTubeExtractor;

// TODO: Implement more extractors
// - Mega
// - Google Drive
