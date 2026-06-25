// Piece — download segment with atomic progress tracking
//
// XDM reference: PieceGrabber.cs, Piece.cs

use std::sync::atomic::{AtomicU64, Ordering};

/// A single download piece (segment) with atomic progress tracking
#[derive(Debug)]
pub struct Piece {
    /// Piece ID (0-indexed)
    pub id: usize,
    
    /// Start byte offset (inclusive)
    pub start: u64,
    
    /// End byte offset (inclusive)
    pub end: u64,
    
    /// Bytes downloaded so far (atomic for lock-free updates from worker threads)
    downloaded: AtomicU64,
    
    /// Whether this piece is complete
    pub complete: bool,
}

impl Piece {
    /// Create a new piece
    pub fn new(id: usize, start: u64, end: u64) -> Self {
        Self {
            id,
            start,
            end,
            downloaded: AtomicU64::new(0),
            complete: false,
        }
    }
    
    /// Total size of this piece in bytes
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }
    
    /// Bytes remaining to download
    pub fn remaining(&self) -> u64 {
        self.size().saturating_sub(self.downloaded())
    }
    
    /// Get current downloaded bytes (atomic read)
    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::Relaxed)
    }
    
    /// Add downloaded bytes (atomic increment)
    pub fn add_downloaded(&self, bytes: u64) {
        self.downloaded.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// Set downloaded bytes (atomic write)
    pub fn set_downloaded(&self, bytes: u64) {
        self.downloaded.store(bytes, Ordering::Relaxed);
    }
    
    /// Progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.size() == 0 {
            return 1.0;
        }
        self.downloaded() as f64 / self.size() as f64
    }
    
    /// Split this piece into two at the midpoint
    /// Returns None if piece is too small to split (< 512 KB)
    pub fn split(&self, min_size: u64) -> Option<Piece> {
        let remaining = self.remaining();
        if remaining < min_size * 2 {
            return None;
        }
        
        let current_pos = self.start + self.downloaded();
        let midpoint = current_pos + remaining / 2;
        
        Some(Piece::new(
            self.id + 1000, // Temporary ID, will be reassigned by PieceManager
            midpoint,
            self.end,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_piece_basic() {
        let piece = Piece::new(0, 0, 1023);
        assert_eq!(piece.size(), 1024);
        assert_eq!(piece.remaining(), 1024);
        assert_eq!(piece.downloaded(), 0);
    }
    
    #[test]
    fn test_piece_progress() {
        let piece = Piece::new(0, 0, 999);
        piece.add_downloaded(500);
        assert_eq!(piece.downloaded(), 500);
        assert_eq!(piece.remaining(), 500);
        assert!((piece.progress() - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_piece_split() {
        let piece = Piece::new(0, 0, 1024 * 1024 - 1); // 1 MB
        piece.add_downloaded(256 * 1024); // 256 KB downloaded
        
        let new_piece = piece.split(256 * 1024).unwrap();
        
        // New piece should start at midpoint of remaining bytes
        let remaining = piece.remaining();
        let expected_start = piece.start + piece.downloaded() + remaining / 2;
        assert_eq!(new_piece.start, expected_start);
        assert_eq!(new_piece.end, piece.end);
    }
    
    #[test]
    fn test_piece_split_too_small() {
        let piece = Piece::new(0, 0, 100 * 1024 - 1); // 100 KB
        assert!(piece.split(256 * 1024).is_none());
    }
}
