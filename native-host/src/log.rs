// ============================================================
// DarkDM Logger — Escribe a archivo + stderr
// ============================================================

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

static LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init(path: &std::path::Path) {
    if let Ok(mut f) = LOG_FILE.lock() {
        *f = Some(path.join("_darkdm_debug.log"));
    }
    // Log header
    log("=== DarkDM Host Started ===");
}

pub fn log(msg: &str) {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    
    let line = format!("[{}] {}\n", timestamp, msg);
    
    // Write to stderr (for browser console)
    eprint!("{}", line);
    
    // Write to file
    if let Ok(f) = LOG_FILE.lock() {
        if let Some(ref path) = *f {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

pub fn log_fmt(fmt: std::fmt::Arguments) {
    log(&format!("{}", fmt));
}
