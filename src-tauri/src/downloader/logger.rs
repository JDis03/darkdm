// Logging system — tracing with file rotation
//
// Usage:
//   logger::init();  // Call once at startup
//   tracing::info!("message");
//   tracing::debug!("debug info");
//   tracing::error!("error: {}", err);

use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use std::path::PathBuf;

/// Initialize logging system
/// 
/// Logs to:
/// - Console (colored, INFO level by default)
/// - File (~/.local/share/darkdm/darkdm.log, rotating daily, max 10MB)
/// 
/// Set RUST_LOG env var to control level:
///   RUST_LOG=debug darkdm descargar ...
///   RUST_LOG=darkdm::downloader=trace darkdm descargar ...
pub fn init() {
    init_with_level("info")
}

/// Initialize with custom log level
pub fn init_with_level(default_level: &str) {
    // Get log directory
    let log_dir = get_log_dir();
    std::fs::create_dir_all(&log_dir).ok();
    
    // File appender (rotating daily, keeps last 5 files)
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("darkdm")
        .filename_suffix("log")
        .max_log_files(5)
        .build(log_dir)
        .expect("Failed to create log file appender");
    
    // Console layer (colored, pretty)
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .with_ansi(true)
        .compact();
    
    // File layer (no colors, full details)
    let file_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_ansi(false)
        .with_writer(file_appender);
    
    // Environment filter (respects RUST_LOG)
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));
    
    // Combine layers
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();
    
    tracing::info!("DarkDM logging initialized");
    tracing::debug!("Log directory: {}", get_log_dir().display());
}

/// Get log directory path
fn get_log_dir() -> PathBuf {
    if let Ok(data_dir) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(data_dir).join("darkdm")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local/share/darkdm")
    } else {
        PathBuf::from("/tmp/darkdm")
    }
}

/// Get current log file path (today's log file)
pub fn get_log_file() -> PathBuf {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    get_log_dir().join(format!("darkdm.{}.log", today))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_log_dir() {
        let dir = get_log_dir();
        assert!(dir.to_str().unwrap().contains("darkdm"));
    }
    
    #[test]
    fn test_log_file() {
        let file = get_log_file();
        let filename = file.file_name().unwrap().to_str().unwrap();
        // Should be darkdm.YYYY-MM-DD.log
        assert!(filename.starts_with("darkdm."));
        assert!(filename.ends_with(".log"));
    }
}
