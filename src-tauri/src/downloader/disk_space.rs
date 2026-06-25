// Disk space check — fail fast before downloading
//
// XDM reference: HTTPDownloaderBase.cs (disk space check)

use std::path::Path;

/// Check if there's enough disk space for the download
pub fn check_disk_space(path: &Path, required_bytes: u64) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::mem;
        
        // Get parent directory (file might not exist yet)
        let dir = path.parent().unwrap_or(path);
        let dir_cstr = CString::new(dir.to_str().unwrap()).unwrap();
        
        unsafe {
            let mut stat: libc::statvfs = mem::zeroed();
            if libc::statvfs(dir_cstr.as_ptr(), &mut stat) != 0 {
                return Err("Failed to get disk space info".to_string());
            }
            
            let available_bytes = stat.f_bavail * stat.f_bsize;
            
            if available_bytes < required_bytes {
                return Err(format!(
                    "Not enough disk space: {} MB available, {} MB required",
                    available_bytes / 1024 / 1024,
                    required_bytes / 1024 / 1024
                ));
            }
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux, skip check (or implement platform-specific)
        let _ = (path, required_bytes);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_disk_space_check() {
        let path = PathBuf::from("/tmp/test_file");
        
        // Should succeed for small file
        assert!(check_disk_space(&path, 1024).is_ok());
        
        // Should fail for impossibly large file (1 PB)
        assert!(check_disk_space(&path, 1024 * 1024 * 1024 * 1024 * 1024).is_err());
    }
}
