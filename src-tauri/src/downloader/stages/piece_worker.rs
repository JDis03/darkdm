// PieceWorker — downloads a single piece with Range headers
//
// XDM reference: PieceGrabber.cs

use crate::downloader::piece::Piece;
use crate::downloader::probe::ProbeResult;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// Callback trait for piece download events (Inversion of Control)
/// 
/// XDM reference: IPieceCallback.cs
#[async_trait::async_trait]
pub trait PieceCallback: Send + Sync {
    /// Called when piece download starts
    async fn on_piece_start(&self, piece_id: usize);
    
    /// Called on each chunk received
    async fn on_piece_progress(&self, piece_id: usize, bytes: u64);
    
    /// Called when piece completes successfully
    async fn on_piece_complete(&self, piece_id: usize);
    
    /// Called on piece error
    async fn on_piece_error(&self, piece_id: usize, error: String);
    
    /// Called when server sends adjacent bytes (ContinueAdjacentPiece)
    async fn on_adjacent_bytes(&self, piece_id: usize, next_piece_id: usize);
}

/// Downloads a single piece using Range headers
pub struct PieceWorker {
    client: reqwest::Client,
    url: String,
}

impl PieceWorker {
    /// Create a new PieceWorker
    pub fn new(url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 min timeout
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, url }
    }
    
    /// Download a piece to a file
    /// 
    /// CRITICAL: Always uses Accept-Encoding: identity to prevent compression
    /// (compressed Content-Length breaks Range calculations)
    pub async fn download_piece(
        &self,
        piece: Arc<Piece>,
        output_path: &std::path::Path,
        callback: Arc<dyn PieceCallback>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        callback.on_piece_start(piece.id).await;
        
        // Build Range header: "bytes=start-end"
        let range = format!("bytes={}-{}", piece.start, piece.end);
        
        let response = self.client
            .get(&self.url)
            .header(reqwest::header::RANGE, range)
            .header(reqwest::header::ACCEPT_ENCODING, "identity") // CRITICAL
            .send()
            .await?;
        
        // Check status
        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            let error = format!("HTTP {}: {}", status, response.text().await?);
            callback.on_piece_error(piece.id, error.clone()).await;
            return Err(error.into());
        }
        
        // Open file for writing at piece offset
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(output_path)
            .await?;
        
        // Seek to piece start
        use tokio::io::AsyncSeekExt;
        let mut file = file;
        file.seek(std::io::SeekFrom::Start(piece.start)).await?;
        
        // Stream response body
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let bytes = chunk.len() as u64;
            
            file.write_all(&chunk).await?;
            piece.add_downloaded(bytes);
            callback.on_piece_progress(piece.id, bytes).await;
        }
        
        file.flush().await?;
        callback.on_piece_complete(piece.id).await;
        
        Ok(())
    }
    
    /// Probe URL to get metadata (first request)
    /// 
    /// XDM reference: HTTPDownloaderBase.cs (Probe method)
    pub async fn probe(&self) -> Result<ProbeResult, Box<dyn std::error::Error>> {
        let response = self.client
            .head(&self.url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity") // CRITICAL
            .send()
            .await?;
        
        let final_url = response.url().to_string();
        let headers = response.headers();
        
        let mut probe = ProbeResult::from_headers(headers, final_url);
        
        // Check for text redirect (CDN returning URL in text/plain body)
        if probe.is_text_redirect {
            let body = self.client
                .get(&self.url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()
                .await?
                .text()
                .await?;
            
            // If body is a valid URL, it's a redirect
            if body.starts_with("http://") || body.starts_with("https://") {
                probe.redirect_url = Some(body.trim().to_string());
            }
        }
        
        Ok(probe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    struct TestCallback;
    
    #[async_trait::async_trait]
    impl PieceCallback for TestCallback {
        async fn on_piece_start(&self, piece_id: usize) {
            println!("Piece {} started", piece_id);
        }
        
        async fn on_piece_progress(&self, piece_id: usize, bytes: u64) {
            println!("Piece {} progress: {} bytes", piece_id, bytes);
        }
        
        async fn on_piece_complete(&self, piece_id: usize) {
            println!("Piece {} complete", piece_id);
        }
        
        async fn on_piece_error(&self, piece_id: usize, error: String) {
            eprintln!("Piece {} error: {}", piece_id, error);
        }
        
        async fn on_adjacent_bytes(&self, piece_id: usize, next_piece_id: usize) {
            println!("Piece {} received adjacent bytes for piece {}", piece_id, next_piece_id);
        }
    }
    
    #[tokio::test]
    async fn test_probe() {
        let worker = PieceWorker::new(
            "https://httpbin.org/bytes/1024".to_string()
        );
        
        let probe = worker.probe().await.unwrap();
        // httpbin may return different size, just check it's Some
        assert!(probe.resource_size.is_some());
        assert!(probe.resource_size.unwrap() > 0);
    }
}
