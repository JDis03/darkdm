// HLS Handler — download HLS streams using ffmpeg
//
// Reference: native-host HLS downloader

use std::process::Command;
use std::path::Path;

/// Check if URL is an HLS stream
pub fn is_hls(url: &str) -> bool {
    url.contains(".m3u8") || url.contains("hls")
}

/// Download HLS stream using ffmpeg
pub async fn download_hls(
    url: &str,
    output_path: &Path,
    show_progress: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if ffmpeg is installed
    if !check_ffmpeg() {
        return Err("ffmpeg not found. Install with: sudo pacman -S ffmpeg".into());
    }
    
    println!("📺 Detected HLS stream, using ffmpeg...");
    
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(url);
    cmd.arg("-c").arg("copy"); // Copy streams without re-encoding
    cmd.arg("-bsf:a").arg("aac_adtstoasc"); // Fix AAC streams
    
    if !show_progress {
        cmd.arg("-loglevel").arg("error");
    }
    
    cmd.arg(output_path);
    
    let status = cmd.status()?;
    
    if !status.success() {
        return Err(format!("ffmpeg failed with exit code: {:?}", status.code()).into());
    }
    
    Ok(())
}

/// Check if ffmpeg is installed
fn check_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_hls() {
        assert!(is_hls("https://example.com/stream.m3u8"));
        assert!(is_hls("https://example.com/hls/master.m3u8"));
        assert!(is_hls("https://manifest.googlevideo.com/.../playlist/index.m3u8"));
        
        assert!(!is_hls("https://example.com/video.mp4"));
        assert!(!is_hls("https://example.com/page.html"));
    }
}
