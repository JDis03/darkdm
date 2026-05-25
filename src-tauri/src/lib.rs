use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use serde::Serialize;
use tauri::State;

const DOWNLOADS_DIR: &str = "Descargas/DarkDM";

#[derive(Debug, Serialize, Clone)]
struct DownloadFile {
    name: String,
    size: u64,
    size_display: String,
    modified: String,
    is_video: bool,
}

#[derive(Default)]
struct AppState {
    last_scan: Mutex<Vec<DownloadFile>>,
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size > 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

fn is_video_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".mp4") || lower.ends_with(".webm") || lower.ends_with(".mkv") ||
    lower.ends_with(".avi") || lower.ends_with(".mov") || lower.ends_with(".ts") ||
    lower.ends_with(".flv") || lower.ends_with(".m4v")
}

#[tauri::command]
fn list_downloads(state: State<AppState>) -> Vec<DownloadFile> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = PathBuf::from(&home).join(DOWNLOADS_DIR);
    
    let mut files = Vec::new();
    
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    
                    // Skip empty files and test files
                    if meta.len() == 0 || name.starts_with(".") {
                        continue;
                    }
                    
                    let modified = meta.modified()
                        .map(|t| {
                            let dur = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                            let secs = dur.as_secs();
                            let days = secs / 86400;
                            let hours = (secs % 86400) / 3600;
                            let mins = (secs % 3600) / 60;
                            if days > 0 {
                                format!("{}d {}h ago", days, hours)
                            } else if hours > 0 {
                                format!("{}h {}m ago", hours, mins)
                            } else if mins > 0 {
                                format!("{}m ago", mins)
                            } else {
                                "just now".to_string()
                            }
                        })
                        .unwrap_or_default();
                    
                    files.push(DownloadFile {
                        name: name.clone(),
                        size: meta.len(),
                        size_display: format_size(meta.len()),
                        modified,
                        is_video: is_video_ext(&name),
                    });
                }
            }
        }
    }
    
    // Sort: newest first
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    
    *state.last_scan.lock().unwrap() = files.clone();
    files
}

#[tauri::command]
fn downloads_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(&home).join(DOWNLOADS_DIR).to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![list_downloads, downloads_path])
        .run(tauri::generate_context!())
        .expect("error while running DarkDM");
}
