// Auto-rename — avoid filename conflicts
//
// XDM reference: HTTPDownloaderBase.cs (auto-rename logic)

use std::path::{Path, PathBuf};

/// Auto-rename file if it already exists
/// 
/// Examples:
/// - file.mp4 → file (1).mp4
/// - file (1).mp4 → file (2).mp4
/// - file.tar.gz → file (1).tar.gz
pub fn auto_rename(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path.file_name().unwrap().to_str().unwrap();
    
    // Split into name and extension(s)
    let (name, ext) = split_name_ext(filename);
    
    // Try (1), (2), (3), ... until we find a free name
    for i in 1..1000 {
        let new_name = if ext.is_empty() {
            format!("{} ({})", name, i)
        } else {
            format!("{} ({}){}", name, i, ext)
        };
        
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return new_path;
        }
    }
    
    // Fallback: append timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let new_name = if ext.is_empty() {
        format!("{} ({})", name, timestamp)
    } else {
        format!("{} ({}){}", name, timestamp, ext)
    };
    
    parent.join(new_name)
}

/// Split filename into (name, extension)
/// 
/// Handles multi-part extensions like .tar.gz
fn split_name_ext(filename: &str) -> (String, String) {
    // Handle multi-part extensions
    let multi_ext = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"];
    
    for ext in &multi_ext {
        if filename.ends_with(ext) {
            let name = filename[..filename.len() - ext.len()].to_string();
            return (name, ext.to_string());
        }
    }
    
    // Single extension
    if let Some(dot_pos) = filename.rfind('.') {
        let name = filename[..dot_pos].to_string();
        let ext = filename[dot_pos..].to_string();
        (name, ext)
    } else {
        (filename.to_string(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_split_name_ext() {
        assert_eq!(split_name_ext("file.mp4"), ("file".to_string(), ".mp4".to_string()));
        assert_eq!(split_name_ext("file.tar.gz"), ("file".to_string(), ".tar.gz".to_string()));
        assert_eq!(split_name_ext("file"), ("file".to_string(), "".to_string()));
        assert_eq!(split_name_ext("my.file.mp4"), ("my.file".to_string(), ".mp4".to_string()));
    }
    
    #[test]
    fn test_auto_rename_nonexistent() {
        let path = PathBuf::from("/tmp/nonexistent_file_12345.mp4");
        assert_eq!(auto_rename(&path), path);
    }
}
