// DownloadEngine — orchestrates multi-threaded download with dynamic piece-splitting
//
// XDM reference: SingleSourceHTTPDownloader.cs, HTTPDownloaderBase.cs

use crate::downloader::{ProbeResult, PieceManager, TransactedIO, ProgressBar};
use crate::downloader::stages::{PieceWorker, PieceCallback};
use crate::downloader::{disk_space, auto_rename};
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
        
        // Auto-rename if file exists (do this BEFORE resumable check)
        let original_path = self.output_path.clone();
        self.output_path = auto_rename::auto_rename(&self.output_path);
        if original_path != self.output_path {
            tracing::info!("Auto-renamed: {} → {}", 
                original_path.display(), self.output_path.display());
            eprintln!("⚠️  File exists, renamed: {} → {}", 
                original_path.display(), self.output_path.display());
        }
        
        // Check if resumable
        if !probe.resumable {
            tracing::warn!("Server does not support Range requests, falling back to single-threaded download");
            return self.download_single_thread().await;
        }
        
        let resource_size = probe.resource_size
            .ok_or("Cannot determine resource size")?;
        
        tracing::info!("Starting multi-threaded download: {} bytes ({:.2} MB)", 
            resource_size, resource_size as f64 / 1024.0 / 1024.0);
        
        // Check disk space before starting
        disk_space::check_disk_space(&self.output_path, resource_size)
            .map_err(|e| {
                tracing::error!("Disk space check failed: {}", e);
                format!("Disk space check failed: {}", e)
            })?;
        
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
        
        // Start download loop with workers
        self.download_loop(progress_bar).await?;
        
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
    
    /// Download loop — spawns workers and waits for completion
    async fn download_loop(
        &self,
        progress_bar: Arc<Mutex<Option<ProgressBar>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::time::{sleep, Duration};
        
        let manager = self.piece_manager.clone();
        let url = self.url.clone();
        let output_path = self.output_path.clone();
        
        // Spawn initial worker
        let initial_piece_id = {
            let mgr = manager.lock().await;
            mgr.piece_ids().first().copied()
        };
        
        if let Some(id) = initial_piece_id {
            self.spawn_worker(id, manager.clone(), url.clone(), output_path.clone(), progress_bar.clone()).await;
        }
        
        // Wait for all workers to complete
        loop {
            sleep(Duration::from_millis(100)).await;
            
            let mgr = manager.lock().await;
            if mgr.is_complete() {
                break;
            }
            drop(mgr);
        }
        
        Ok(())
    }
    
    /// Spawn a single worker for a piece
    async fn spawn_worker(
        &self,
        piece_id: usize,
        manager: Arc<Mutex<PieceManager>>,
        url: String,
        output_path: PathBuf,
        progress_bar: Arc<Mutex<Option<ProgressBar>>>,
    ) {
        let piece = {
            let mgr = manager.lock().await;
            mgr.get_piece(piece_id)
        };
        
        if let Some(piece) = piece {
            let callback = Arc::new(EngineCallback::new(
                manager.clone(),
                progress_bar.clone(),
                url.clone(),
                output_path.clone(),
            ));
            let worker = PieceWorker::new(url.clone());
            let piece_id = piece.id;
            
            tokio::spawn(async move {
                if let Err(e) = worker.download_piece(piece, &output_path, callback).await {
                    tracing::error!("Worker error for piece {}: {}", piece_id, e);
                    eprintln!("\nWorker error: {}", e);
                }
            });
        }
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
    url: String,
    output_path: PathBuf,
}

impl EngineCallback {
    fn new(
        manager: Arc<Mutex<PieceManager>>,
        progress_bar: Arc<Mutex<Option<ProgressBar>>>,
        url: String,
        output_path: PathBuf,
    ) -> Self {
        Self { manager, progress_bar, url, output_path }
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
        if let Some(new_id) = manager.try_create_piece() {
            // Spawn new worker for the new piece
            let piece = manager.get_piece(new_id);
            drop(manager);
            
            if let Some(piece) = piece {
                let callback = Arc::new(EngineCallback::new(
                    self.manager.clone(),
                    self.progress_bar.clone(),
                    self.url.clone(),
                    self.output_path.clone(),
                ));
                let worker = PieceWorker::new(self.url.clone());
                let output_path = self.output_path.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = worker.download_piece(piece, &output_path, callback).await {
                        eprintln!("\nWorker error: {}", e);
                    }
                });
            }
        } else {
            drop(manager);
        }
        
        // Check if all complete
        let manager = self.manager.lock().await;
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
        // httpbin may return different size, just check it's Some
        assert!(probe.resource_size.is_some());
        assert!(probe.resource_size.unwrap() > 0);
    }
}
