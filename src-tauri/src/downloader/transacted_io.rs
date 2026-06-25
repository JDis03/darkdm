// TransactedIO — crash-safe state persistence with 3-file rotation
//
// XDM reference: TransactedIO.cs
//
// Algorithm:
// 1. Write to state.tmp
// 2. Write END marker
// 3. Atomic rename state.tmp → state.2
// 4. Atomic rename state.1 → state.2 (if exists)
// 5. Atomic rename state.2 → state.1
//
// On crash: read state.1, fallback to state.2 if corrupted
// rename(2) is atomic on Linux → no partial writes visible

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

const END_MARKER: &[u8] = b"END\n";

/// Crash-safe state persistence
pub struct TransactedIO {
    base_path: PathBuf,
}

impl TransactedIO {
    /// Create a new TransactedIO for the given base path
    /// 
    /// Example: base_path = "~/Downloads/DarkDM/video.mp4.state"
    /// Creates: video.mp4.state.1, video.mp4.state.2, video.mp4.state.tmp
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }
    
    fn state_1(&self) -> PathBuf {
        self.base_path.with_extension("state.1")
    }
    
    fn state_2(&self) -> PathBuf {
        self.base_path.with_extension("state.2")
    }
    
    fn state_tmp(&self) -> PathBuf {
        self.base_path.with_extension("state.tmp")
    }
    
    /// Write state atomically
    pub fn write<T: Serialize>(&self, state: &T) -> io::Result<()> {
        let tmp_path = self.state_tmp();
        let state_1 = self.state_1();
        let state_2 = self.state_2();
        
        // 1. Write to tmp file
        let json = serde_json::to_string_pretty(state)?;
        let mut file = File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.write_all(END_MARKER)?;
        file.sync_all()?;
        drop(file);
        
        // 2. Rotate: preserve old state.1 as state.2 backup before overwriting
        // This ensures we always have at least one valid state file
        
        // If state.1 exists, move it to state.2 as backup (atomic)
        if state_1.exists() {
            // Remove old state.2 if it exists
            let _ = fs::remove_file(&state_2);
            fs::rename(&state_1, &state_2)?;
        }
        
        // tmp → state.1 (atomic)
        fs::rename(&tmp_path, &state_1)?;
        
        Ok(())
    }
    
    /// Read state with fallback
    /// 
    /// Tries state.1 first, falls back to state.2 if corrupted
    pub fn read<T: for<'de> Deserialize<'de>>(&self) -> io::Result<T> {
        let state_1 = self.state_1();
        let state_2 = self.state_2();
        
        // Try state.1 first
        if state_1.exists() {
            match self.read_file(&state_1) {
                Ok(state) => return Ok(state),
                Err(e) => {
                    eprintln!("Warning: state.1 corrupted ({}), trying state.2", e);
                }
            }
        }
        
        // Fallback to state.2
        if state_2.exists() {
            return self.read_file(&state_2);
        }
        
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No valid state file found",
        ))
    }
    
    /// Read and validate a single state file
    fn read_file<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> io::Result<T> {
        let mut file = File::open(path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        
        // Validate END marker
        if !contents.ends_with(END_MARKER) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing END marker (incomplete write)",
            ));
        }
        
        // Remove END marker before parsing
        contents.truncate(contents.len() - END_MARKER.len());
        
        serde_json::from_slice(&contents).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, e)
        })
    }
    
    /// Check if state exists
    pub fn exists(&self) -> bool {
        self.state_1().exists() || self.state_2().exists()
    }
    
    /// Delete all state files
    pub fn delete(&self) -> io::Result<()> {
        let _ = fs::remove_file(self.state_1());
        let _ = fs::remove_file(self.state_2());
        let _ = fs::remove_file(self.state_tmp());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::env;
    
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestState {
        downloaded: u64,
        pieces: Vec<String>,
    }
    
    #[test]
    fn test_transacted_io_write_read() {
        let temp_dir = env::temp_dir();
        let base_path = temp_dir.join("test_state");
        let io = TransactedIO::new(&base_path);
        
        let state = TestState {
            downloaded: 1024,
            pieces: vec!["piece1".to_string(), "piece2".to_string()],
        };
        
        io.write(&state).unwrap();
        let read_state: TestState = io.read().unwrap();
        
        assert_eq!(state, read_state);
        
        // Cleanup
        io.delete().unwrap();
    }
    
    #[test]
    fn test_transacted_io_crash_recovery() {
        let temp_dir = env::temp_dir();
        let base_path = temp_dir.join("test_crash_state");
        let io = TransactedIO::new(&base_path);
        
        // Write initial state
        let state1 = TestState {
            downloaded: 1024,
            pieces: vec!["piece1".to_string()],
        };
        io.write(&state1).unwrap();
        
        // Write second state (this moves state1 to state.2 as backup)
        let state2 = TestState {
            downloaded: 2048,
            pieces: vec!["piece1".to_string(), "piece2".to_string()],
        };
        io.write(&state2).unwrap();
        
        // Simulate crash: corrupt state.1 by removing END marker
        let state_1_path = io.state_1();
        let mut contents = fs::read(&state_1_path).unwrap();
        contents.truncate(contents.len() - END_MARKER.len());
        fs::write(&state_1_path, contents).unwrap();
        
        // Should fallback to state.2 (which has state1, the previous valid state)
        let read_state: TestState = io.read().unwrap();
        assert_eq!(state1, read_state);
        
        // Cleanup
        io.delete().unwrap();
    }
}
