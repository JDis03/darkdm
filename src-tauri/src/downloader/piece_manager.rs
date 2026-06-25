// PieceManager — orchestrates dynamic piece-splitting (work-stealing)
//
// XDM reference: PieceGrabber.cs, HTTPDownloaderBase.cs

use crate::downloader::piece::Piece;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type PieceId = usize;

/// Manages the pool of download pieces with dynamic splitting
pub struct PieceManager {
    pieces: HashMap<PieceId, Arc<Piece>>,
    active: HashSet<PieceId>,
    failed: HashSet<PieceId>,
    max_active: usize,
    next_id: PieceId,
    min_split_size: u64,
}

impl PieceManager {
    /// Create a new PieceManager
    pub fn new(max_active: usize) -> Self {
        Self {
            pieces: HashMap::new(),
            active: HashSet::new(),
            failed: HashSet::new(),
            max_active,
            next_id: 0,
            min_split_size: 256 * 1024, // 256 KB minimum
        }
    }
    
    /// Initialize with a single piece covering the entire resource
    pub fn init_single_piece(&mut self, resource_size: u64) -> PieceId {
        let piece = Arc::new(Piece::new(0, 0, resource_size - 1));
        self.pieces.insert(0, piece);
        self.next_id = 1;
        0
    }
    
    /// Try to create a new piece by splitting the largest active piece
    /// 
    /// Returns Some(piece_id) if a new piece was created, None otherwise
    pub fn try_create_piece(&mut self) -> Option<PieceId> {
        // Can't create more pieces if we're at max capacity
        if self.active.len() >= self.max_active {
            return None;
        }
        
        // 1. Retry failed pieces first
        if let Some(id) = self.retry_failed() {
            self.active.insert(id);
            return Some(id);
        }
        
        // 2. Find the largest active piece that can be split
        let split_target = self.pieces.iter()
            .filter(|(id, piece)| {
                self.active.contains(id) && piece.remaining() >= self.min_split_size * 2
            })
            .max_by_key(|(_, piece)| piece.remaining())
            .map(|(id, _)| *id);
        
        if let Some(target_id) = split_target {
            // Split the piece
            let target_piece = self.pieces.get(&target_id).unwrap();
            if let Some(new_piece) = target_piece.split(self.min_split_size) {
                let new_id = self.next_id;
                self.next_id += 1;
                
                let new_piece = Arc::new(Piece::new(new_id, new_piece.start, new_piece.end));
                self.pieces.insert(new_id, new_piece);
                self.active.insert(new_id);
                
                return Some(new_id);
            }
        }
        
        None
    }
    
    /// Retry a failed piece
    fn retry_failed(&mut self) -> Option<PieceId> {
        if let Some(&id) = self.failed.iter().next() {
            self.failed.remove(&id);
            return Some(id);
        }
        None
    }
    
    /// Mark a piece as active
    pub fn mark_active(&mut self, id: PieceId) {
        self.active.insert(id);
    }
    
    /// Mark a piece as complete
    pub fn mark_complete(&mut self, id: PieceId) {
        self.active.remove(&id);
        if let Some(piece) = self.pieces.get_mut(&id) {
            // Mark as complete (need to make Piece.complete mutable)
            // For now, we just remove from active set
        }
    }
    
    /// Mark a piece as failed
    pub fn mark_failed(&mut self, id: PieceId) {
        self.active.remove(&id);
        self.failed.insert(id);
    }
    
    /// Get a piece by ID
    pub fn get_piece(&self, id: PieceId) -> Option<Arc<Piece>> {
        self.pieces.get(&id).cloned()
    }
    
    /// Get total downloaded bytes across all pieces
    pub fn total_downloaded(&self) -> u64 {
        self.pieces.values().map(|p| p.downloaded()).sum()
    }
    
    /// Get total size across all pieces
    pub fn total_size(&self) -> u64 {
        self.pieces.values().map(|p| p.size()).sum()
    }
    
    /// Check if all pieces are complete
    pub fn is_complete(&self) -> bool {
        self.active.is_empty() && 
        self.failed.is_empty() &&
        self.pieces.values().all(|p| p.remaining() == 0)
    }
    
    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        let total = self.total_size();
        if total == 0 {
            return 1.0;
        }
        self.total_downloaded() as f64 / total as f64
    }
    
    /// Get number of active workers
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
    
    /// Get all piece IDs
    pub fn piece_ids(&self) -> Vec<PieceId> {
        self.pieces.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_piece_manager_init() {
        let mut manager = PieceManager::new(8);
        let id = manager.init_single_piece(1024 * 1024); // 1 MB
        
        assert_eq!(id, 0);
        assert_eq!(manager.total_size(), 1024 * 1024);
        assert_eq!(manager.total_downloaded(), 0);
    }
    
    #[test]
    fn test_piece_manager_split() {
        let mut manager = PieceManager::new(8);
        let id = manager.init_single_piece(1024 * 1024); // 1 MB
        
        manager.mark_active(id);
        
        // Simulate some download progress
        let piece = manager.get_piece(id).unwrap();
        piece.add_downloaded(256 * 1024); // 256 KB downloaded
        
        // Try to create a new piece by splitting
        let new_id = manager.try_create_piece();
        assert!(new_id.is_some());
        assert_eq!(new_id.unwrap(), 1);
        
        // Should have 2 pieces now
        assert_eq!(manager.pieces.len(), 2);
        assert_eq!(manager.active_count(), 2);
    }
    
    #[test]
    fn test_piece_manager_no_split_too_small() {
        let mut manager = PieceManager::new(8);
        let id = manager.init_single_piece(100 * 1024); // 100 KB (too small)
        
        manager.mark_active(id);
        
        // Try to split - should fail because piece is too small
        let new_id = manager.try_create_piece();
        assert!(new_id.is_none());
    }
    
    #[test]
    fn test_piece_manager_max_active() {
        let mut manager = PieceManager::new(2); // Max 2 active
        let id = manager.init_single_piece(10 * 1024 * 1024); // 10 MB
        
        manager.mark_active(id);
        
        // Create first split
        let id1 = manager.try_create_piece();
        assert!(id1.is_some());
        assert_eq!(manager.active_count(), 2);
        
        // Try to create another - should fail (at max)
        let id2 = manager.try_create_piece();
        assert!(id2.is_none());
    }
    
    #[test]
    fn test_piece_manager_retry_failed() {
        let mut manager = PieceManager::new(8);
        let id = manager.init_single_piece(1024 * 1024);
        
        manager.mark_active(id);
        manager.mark_failed(id);
        
        assert_eq!(manager.active_count(), 0);
        assert!(manager.failed.contains(&id));
        
        // Try to create piece - should retry the failed one
        let retry_id = manager.try_create_piece();
        assert_eq!(retry_id, Some(id));
        assert!(!manager.failed.contains(&id));
        assert!(manager.active.contains(&id));
    }
}
