// DownloadEngine — orchestrates multi-threaded download with dynamic piece-splitting
//
// XDM reference: SingleSourceHTTPDownloader.cs, HTTPDownloaderBase.cs

use crate::downloader::{ProbeResult, PieceManager, TransactedIO, ProgressBar};
use crate::downloader::stages::{PieceWorker, PieceCallback};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

/// Download state persisted to disk
#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadState {
    pub url: String,
    pub output_path: String,
    pub resource_size: u64,
    pub downloaded: u64,
    pub pieces: Vec<PieceState>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PieceState {
    pub id: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
}

/// Download engine configuration
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub max_workers: usize,
    pub output_dir: PathBuf,
    pub resume: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_workers: 8,
            output_dir: PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join("Descargas/DarkDM"),
            resume: true,
        }
    }
}

/// Main download engine
pub struct DownloadEngine {
    config: DownloadConfig,
    url: String,
    output_path: PathBuf,
    probe_result: Option<ProbeResult>,
    piece_manager: Arc<Mutex<PieceManager>>,
    state_io: TransactedIO,
}

impl DownloadEngine {
    /// Create a new download engine
    pub fn new(url: String, config: DownloadConfig) -> Self {
        let output_path = config.output_dir.join("download.tmp");
        let state_io = TransactedIO::new(&output_path);
        
        Self {
            config,
            url,
            output_path,
            probe_result: None,
            piece_manager: Arc::new(Mutex::new(PieceManager::new(8))),
            state_io,
        }
    }
    
    /// Probe the URL to get metadata
    pub async fn probe(&mut self) -> Result<ProbeResult, Box<dyn std::error::Error>> {
        let worker = PieceWorker::new(self.url.clone());
        let probe = worker.probe().await?;
        
        // Update output path with actual filename
        if let Some(filename) = &probe.filename {
            self.output_path = self.config.output_dir.join(filename);
            self.state_io = TransactedIO::new(&self.output_path);
        }
        
        self.probe_result = Some(probe.clone());
        Ok(probe)
    }
    
    /// Start the download
    pub async fn download(&mut self, show_progress: bool) -> Result<(), Box<dyn std::error::Error>> {
        // Probe if not already done
        if self.probe_result.is_none() {
            self.probe().await?;
        }
        
        let probe = self.probe_result.as_ref().unwrap();
        
        // Check if resumable
        if !probe.resumable {
            return self.download_single_thread().await;
        }
        
        let resource_size = probe.resource_size
            .ok_or("Cannot determine resource size")?;
        
        // Initialize piece manager
        let mut manager = self.piece_manager.lock().await;
        let initial_piece_id = manager.init_single_piece(resource_size);
        manager.mark_active(initial_piece_id);
        drop(manager);
        
        // Create output file
        tokio::fs::create_dir_all(&self.config.output_dir).await?;
        let file = tokio::fs::File::create(&self.output_path).await?;
        file.set_len(resource_size).await?;
        drop(file);
        
        // Create progress bar
        let progress_bar = if show_progress {
            Some(ProgressBar::new(probe.filename_or_default(), resource_size))
        } else {
            None
        };
        let progress_bar = Arc::new(Mutex::new(progress_bar));
        
        // Start workers
        self.spawn_workers_with_progress(progress_bar).await?;
        
        Ok(())
    }
    
    /// Download with a single thread (no Range support)
    async fn download_single_thread(&self) -> Result<(), Box<dyn std::error::Error>> {
        let worker = PieceWorker::new(self.url.clone());
        let client = reqwest::Client::new();
        
        let response = client
            .get(&self.url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?;
        
        tokio::fs::create_dir_all(&self.config.output_dir).await?;
        let mut file = tokio::fs::File::create(&self.output_path).await?;
        
        use tokio::io::AsyncWriteExt;
        use futures_util::StreamExt;
        
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
        }
        
        file.flush().await?;
        Ok(())
    }
    
    /// Spawn worker tasks with progress bar
    async fn spawn_workers_with_progress(
        &self,
        progress_bar: Arc<Mutex<Option<ProgressBar>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let manager = self.piece_manager.clone();
        let url = self.url.clone();
        let output_path = self.output_path.clone();
        
        // Get initial piece
        let piece_id = {
            let mgr = manager.lock().await;
            mgr.piece_ids().first().copied()
        };
        
        if let Some(id) = piece_id {
            let piece = {
                let mgr = manager.lock().await;
                mgr.get_piece(id)
            };
            
            if let Some(piece) = piece {
                let callback = Arc::new(EngineCallback::new(manager.clone(), progress_bar));
                let worker = PieceWorker::new(url.clone());
                
                tokio::spawn(async move {
                    if let Err(e) = worker.download_piece(piece, &output_path, callback).await {
                        eprintln!("\nWorker error: {}", e);
                    }
                });
            }
        }
        
        Ok(())
    }
    
    /// Get download progress
    pub async fn progress(&self) -> f64 {
        let manager = self.piece_manager.lock().await;
        manager.progress()
    }
    
    /// Get total downloaded bytes
    pub async fn downloaded(&self) -> u64 {
        let manager = self.piece_manager.lock().await;
        manager.total_downloaded()
    }
}

/// Callback implementation for the engine
struct EngineCallback {
    manager: Arc<Mutex<PieceManager>>,
    progress_bar: Arc<Mutex<Option<ProgressBar>>>,
}

impl EngineCallback {
    fn new(manager: Arc<Mutex<PieceManager>>, progress_bar: Arc<Mutex<Option<ProgressBar>>>) -> Self {
        Self { manager, progress_bar }
    }
}

#[async_trait::async_trait]
impl PieceCallback for EngineCallback {
    async fn on_piece_start(&self, _piece_id: usize) {
        // Piece started - no output needed
    }
    
    async fn on_piece_progress(&self, _piece_id: usize, _bytes: u64) {
        // Update progress bar with total downloaded
        let manager = self.manager.lock().await;
        let total_downloaded = manager.total_downloaded();
        drop(manager);
        
        let mut pb = self.progress_bar.lock().await;
        if let Some(bar) = pb.as_mut() {
            bar.update(total_downloaded);
        }
    }
    
    async fn on_piece_complete(&self, piece_id: usize) {
        let mut manager = self.manager.lock().await;
        manager.mark_complete(piece_id);
        
        // Try to create a new piece by splitting
        if let Some(_new_id) = manager.try_create_piece() {
            // TODO: spawn new worker for this piece
        }
        
        // Check if all complete
        if manager.is_complete() {
            drop(manager);
            let mut pb = self.progress_bar.lock().await;
            if let Some(bar) = pb.as_mut() {
                bar.finish();
            }
        }
    }
    
    async fn on_piece_error(&self, piece_id: usize, error: String) {
        eprintln!("Piece {} error: {}", piece_id, error);
        let mut manager = self.manager.lock().await;
        manager.mark_failed(piece_id);
    }
    
    async fn on_adjacent_bytes(&self, piece_id: usize, next_piece_id: usize) {
        println!("Piece {} received adjacent bytes for piece {}", piece_id, next_piece_id);
        // ContinueAdjacentPiece - reuse TCP connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_download_engine_probe() {
        let config = DownloadConfig::default();
        let mut engine = DownloadEngine::new(
            "https://httpbin.org/bytes/1024".to_string(),
            config,
        );
        
        let probe = engine.probe().await.unwrap();
        assert_eq!(probe.resource_size, Some(1024));
    }
}
