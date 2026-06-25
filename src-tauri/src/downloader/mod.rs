// DarkDM Download Engine
//
// Architecture ported from XDM (Xtreme Download Manager)
// Reference: https://github.com/subhra74/xdm

pub mod piece;
pub mod probe;
pub mod transacted_io;
pub mod piece_manager;
pub mod download_engine;
pub mod progress;
pub mod disk_space;
pub mod auto_rename;
pub mod content_type;
pub mod page_analyzer;
pub mod hls_handler;

pub mod stages;
pub mod plugins;

// Re-exports
pub use piece::Piece;
pub use probe::ProbeResult;
pub use transacted_io::TransactedIO;
pub use piece_manager::{PieceManager, PieceId};
pub use download_engine::{DownloadEngine, DownloadConfig, DownloadState};
pub use progress::ProgressBar;
