// Progress bar — pacman-style (Arch Linux)
//
// Format:
// filename.mp4         45.2 MiB  1234 KiB/s 00:38 [######################] 100%

use std::io::{self, Write};
use std::time::{Duration, Instant};

pub struct ProgressBar {
    filename: String,
    total_size: u64,
    downloaded: u64,
    start_time: Instant,
    last_update: Instant,
    last_downloaded: u64,
    width: usize,
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new(filename: String, total_size: u64) -> Self {
        Self {
            filename,
            total_size,
            downloaded: 0,
            start_time: Instant::now(),
            last_update: Instant::now(),
            last_downloaded: 0,
            width: 40, // width of the bar itself
        }
    }
    
    /// Update progress
    pub fn update(&mut self, downloaded: u64) {
        self.downloaded = downloaded;
        self.render();
    }
    
    /// Render the progress bar (pacman style)
    fn render(&mut self) {
        let now = Instant::now();
        
        // Only update every 100ms to avoid flickering
        if now.duration_since(self.last_update) < Duration::from_millis(100) {
            return;
        }
        
        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        let progress = if self.total_size > 0 {
            self.downloaded as f64 / self.total_size as f64
        } else {
            0.0
        };
        
        // Calculate speed (bytes/sec)
        let speed = if elapsed > 0.0 {
            self.downloaded as f64 / elapsed
        } else {
            0.0
        };
        
        // Calculate ETA
        let remaining_bytes = self.total_size.saturating_sub(self.downloaded);
        let eta_secs = if speed > 0.0 {
            remaining_bytes as f64 / speed
        } else {
            0.0
        };
        
        // Format sizes
        let downloaded_str = format_size(self.downloaded);
        let total_str = format_size(self.total_size);
        let speed_str = format_speed(speed);
        let eta_str = format_time(eta_secs as u64);
        
        // Build progress bar
        let filled = (progress * self.width as f64) as usize;
        let empty = self.width.saturating_sub(filled);
        let bar = format!("{}{}", "#".repeat(filled), "-".repeat(empty));
        
        // Truncate filename if too long
        let max_filename_len = 20;
        let display_filename = if self.filename.len() > max_filename_len {
            format!("{}...", &self.filename[..max_filename_len - 3])
        } else {
            format!("{:width$}", self.filename, width = max_filename_len)
        };
        
        // Print (overwrite previous line)
        print!("\r{} {:>8} / {:>8}  {:>10}  {:>5} [{}] {:>3}%",
            display_filename,
            downloaded_str,
            total_str,
            speed_str,
            eta_str,
            bar,
            (progress * 100.0) as u8
        );
        
        io::stdout().flush().unwrap();
        
        self.last_update = now;
        self.last_downloaded = self.downloaded;
    }
    
    /// Finish the progress bar (print newline)
    pub fn finish(&self) {
        println!();
    }
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    
    if unit == 0 {
        format!("{} {}", size as u64, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

/// Format speed as KiB/s or MiB/s
fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1024.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else if bytes_per_sec < 1024.0 * 1024.0 {
        format!("{:.0} KiB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.1} MiB/s", bytes_per_sec / 1024.0 / 1024.0)
    }
}

/// Format time as MM:SS
fn format_time(seconds: u64) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}", mins, secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_size() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GiB");
    }
    
    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(512.0), "512 B/s");
        assert_eq!(format_speed(1024.0), "1 KiB/s");
        assert_eq!(format_speed(1536.0), "2 KiB/s");
        assert_eq!(format_speed(1024.0 * 1024.0), "1.0 MiB/s");
    }
    
    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "00:00");
        assert_eq!(format_time(30), "00:30");
        assert_eq!(format_time(90), "01:30");
        assert_eq!(format_time(3661), "61:01");
    }
}
