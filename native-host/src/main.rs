// ============================================================
// DarkDM Native Messaging Host
// Puente extensión ↔ sistema + Motor de descarga HLS/DASH
// ============================================================

mod downloader;

use downloader::*;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const OUTPUT_DIR: &str = "Descargas/DarkDM";

#[derive(Deserialize, Debug)]
struct ChromeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    url: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    tab_id: Option<u64>,
    page_url: Option<String>,
    page_title: Option<String>,
    cookies: Option<String>,        // Netscape cookie string from extension
    manifest_url: Option<String>,   // URL of .m3u8/.mpd manifest
    manifest_body: Option<String>,  // Content of the manifest
    headers: Option<String>,        // JSON string of request headers
    title: Option<String>,          // Video title
    #[serde(flatten)]
    extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Debug)]
struct Response {
    #[serde(rename = "type")]
    msg_type: String,
    success: Option<bool>,
    error: Option<String>,
    message: Option<String>,
    progress: Option<f64>,
    filename: Option<String>,
    segments: Option<usize>,
    bytes: Option<u64>,
}

fn main() {
    while let Ok(msg) = read_message() {
        eprintln!("[DarkDM] Received: {}", msg.msg_type);
        let response = handle_message(&msg);
        if write_message(&response).is_err() {
            break;
        }
    }
}

fn read_message() -> io::Result<ChromeMessage> {
    let mut len_buf = [0u8; 4];
    io::stdin().read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    io::stdin().read_exact(&mut buf)?;
    serde_json::from_slice(&buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_message(response: &Response) -> io::Result<()> {
    let json = serde_json::to_vec(response)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&len.to_le_bytes())?;
    handle.write_all(&json)?;
    handle.flush()?;
    Ok(())
}

fn handle_message(msg: &ChromeMessage) -> Response {
    // Get output directory
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let output_dir = Path::new(&home).join(OUTPUT_DIR);
    std::fs::create_dir_all(&output_dir).unwrap_or_default();

    match msg.msg_type.as_str() {
        // ============================================================
        // STREAM DETECTED - Download HLS/DASH/Direct stream
        // ============================================================
        "STREAM_DETECTED" | "MANIFEST_DETECTED" | "START_DOWNLOAD" => {
            let url = match &msg.url {
                Some(u) => u,
                None => return Response { 
                    msg_type: "ERROR".to_string(), success: Some(false), 
                    error: Some("No URL provided".to_string()),
                    message: None, progress: None, filename: None, 
                    segments: None, bytes: None,
                },
            };
            
            let stream_type = detect_stream_type(url, msg.content_type.as_deref());
            let filename = msg.filename.clone()
                .unwrap_or_else(|| format!("darkdm_video_{}", chrono_name()));
            
            eprintln!("[DarkDM] Downloading {:?} stream: {}", stream_type, url);

            match stream_type {
                StreamType::HLS => {
                    let output_path = output_dir.join(format!("{}.mp4", filename));
                    let output_str = output_path.to_string_lossy().to_string();
                    
                    // 1) Try yt-dlp con impersonación (como navegador real)
                    if which("yt-dlp") {
                        eprintln!("[DarkDM] yt-dlp for HLS: {}", url);
                        let status = Command::new("yt-dlp")
                            .args(["-o", &output_str, "--no-playlist", "--concurrent-fragments", "8",
                                   "--impersonate", "chrome", url])
                            .stdout(Stdio::null()).stderr(Stdio::null())
                            .status();
                        if let Ok(s) = status {
                            if s.success() {
                                let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                                return ok_response(&format!("HLS via yt-dlp: {} bytes", size), &output_str, size);
                            }
                        }
                    }
                    
                    // 2) Try ffmpeg
                    if which("ffmpeg") {
                        eprintln!("[DarkDM] ffmpeg for HLS: {}", url);
                        let status = Command::new("ffmpeg")
                            .args(["-y", "-i", url, "-c", "copy", "-movflags", "+faststart", &output_str])
                            .stdout(Stdio::null()).stderr(Stdio::null())
                            .status();
                        if let Ok(s) = status {
                            if s.success() {
                                let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                                return ok_response(&format!("HLS via ffmpeg: {} bytes", size), &output_str, size);
                            }
                        }
                    }
                    
                    // 3) Custom downloader
                    let result = download_hls(url, &output_dir, Some(&filename), 
                        Some(Arc::new(AtomicBool::new(false))),
                        |c, t| eprintln!("[DarkDM HLS] {}/{}", c, t));
                    
                    Response {
                        msg_type: "DOWNLOAD_RESULT".to_string(),
                        success: Some(result.success), error: result.error,
                        message: Some(format!("HLS: {} segments, {} bytes", 
                            result.segments_downloaded, result.total_bytes)),
                        progress: Some(1.0),
                        filename: Some(result.output_path.to_string_lossy().to_string()),
                        segments: Some(result.segments_downloaded),
                        bytes: Some(result.total_bytes),
                    }
                },
                StreamType::DASH => {
                    let result = download_dash(url, &output_dir, Some(&filename),
                        Some(Arc::new(AtomicBool::new(false))),
                        |current, total| {
                            eprintln!("[DarkDM DASH] Progress: {}/{}", current, total);
                        });
                    
                    Response {
                        msg_type: "DOWNLOAD_RESULT".to_string(),
                        success: Some(result.success),
                        error: result.error,
                        message: Some(format!("DASH download: {} segments, {} bytes",
                            result.segments_downloaded, result.total_bytes)),
                        progress: Some(1.0),
                        filename: Some(result.output_path.to_string_lossy().to_string()),
                        segments: Some(result.segments_downloaded),
                        bytes: Some(result.total_bytes),
                    }
                },
                StreamType::Direct => {
                    // Direct file download via aria2c or wget
                    match launch_direct_download(url, &filename, &output_dir) {
                        Ok(path) => Response {
                            msg_type: "DOWNLOAD_STARTED".to_string(),
                            success: Some(true),
                            error: None,
                            message: Some(format!("Downloading to: {}", path)),
                            progress: None,
                            filename: Some(path),
                            segments: None, bytes: None,
                        },
                        Err(e) => Response {
                            msg_type: "DOWNLOAD_ERROR".to_string(),
                            success: Some(false),
                            error: Some(e),
                            message: None, progress: None, filename: None,
                            segments: None, bytes: None,
                        },
                    }
                },
            }
        },

        // ============================================================
        // EXTRACT PAGE - Use yt-dlp for complex sites (Netflix, etc.)
        // ============================================================
        "EXTRACT_PAGE" => {
            let url = match &msg.url {
                Some(u) => u.clone(),
                None => return error_response("No URL provided"),
            };
            let has_drm = msg.extra.get("hasDrm")
                .and_then(|v| v.as_bool()).unwrap_or(false);
            let site = msg.extra.get("site")
                .and_then(|v| v.as_str()).unwrap_or("");
            let cookies = msg.cookies.as_deref().unwrap_or("");
            
            match extract_with_ytdlp(&url, &output_dir, has_drm, site, cookies) {
                Ok(path) => Response {
                    msg_type: "DOWNLOAD_STARTED".to_string(),
                    success: Some(true), error: None,
                    message: Some(format!("yt-dlp extract: {}", path)),
                    progress: None, filename: Some(path),
                    segments: None, bytes: None,
                },
                Err(e) => error_response(&format!("yt-dlp failed: {}", e)),
            }
        },

        // ============================================================
        // DOWNLOAD MANIFEST - Descarga via manifest .m3u8 (como IDM)
        // Extension captura el manifest + cookies + headers del browser
        // Native host descarga todo independientemente del reproductor
        // ============================================================
        "DOWNLOAD_MANIFEST" => {
            let manifest_url = match &msg.manifest_url {
                Some(u) => u.clone(),
                None => return error_response("No manifest URL"),
            };
            let manifest_body = msg.manifest_body.as_deref().unwrap_or("");
            let cookies = msg.cookies.as_deref().unwrap_or("");
            let headers_json = msg.headers.as_deref().unwrap_or("{}");
            let title = msg.title.as_deref().unwrap_or("video");
            
            eprintln!("[DarkDM] DOWNLOAD_MANIFEST: {} (body: {} bytes)", manifest_url, manifest_body.len());
            
            // Sanitize title for filename
            let safe_title: String = title.chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
                .collect::<String>()
                .trim()
                .chars().take(80).collect();
            let filename = if safe_title.is_empty() { "darkdm_video".to_string() } else { safe_title };
            
            let output_path = output_dir.join(format!("{}.mp4", filename));
            let output_str = output_path.to_string_lossy().to_string();
            
            // Write cookies to temp file if provided
            let cookies_path = output_dir.join("_darkdm_cookies.txt");
            if !cookies.is_empty() {
                let _ = std::fs::write(&cookies_path, cookies);
            }
            
            // Try ffmpeg with proper headers + cookies
            if which("ffmpeg") {
                eprintln!("[DarkDM] ffmpeg download from manifest");
                
                let mut ffmpeg_cmd = Command::new("ffmpeg");
                ffmpeg_cmd.args(["-y", "-hide_banner", "-loglevel", "error"]);
                
                // Add cookies if available
                if !cookies.is_empty() && std::fs::metadata(&cookies_path).map(|m| m.len() > 0).unwrap_or(false) {
                    ffmpeg_cmd.args(["-cookies", &cookies_path.to_string_lossy()]);
                }
                
                // Add custom headers (User-Agent, Referer, etc.)
                if let Ok(headers_map) = serde_json::from_str::<std::collections::HashMap<String, String>>(headers_json) {
                    for (k, v) in &headers_map {
                        if k.eq_ignore_ascii_case("user-agent") || k.eq_ignore_ascii_case("referer") {
                            ffmpeg_cmd.args(["-headers", &format!("{}: {}\r\n", k, v)]);
                        }
                    }
                } else {
                    // Fallback User-Agent
                    ffmpeg_cmd.args(["-user_agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36"]);
                }
                
                // Input from manifest URL
                ffmpeg_cmd.args(["-i", &manifest_url]);
                ffmpeg_cmd.args(["-c", "copy", "-movflags", "+faststart", &output_str]);
                
                eprintln!("[DarkDM] Running ffmpeg...");
                let start = std::time::Instant::now();
                let status = ffmpeg_cmd
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .status();
                
                match status {
                    Ok(s) if s.success() => {
                        let elapsed = start.elapsed();
                        let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                        eprintln!("[DarkDM] ffmpeg OK: {} bytes in {:?}", size, elapsed);
                        // Cleanup temp cookies
                        let _ = std::fs::remove_file(&cookies_path);
                        return ok_response(&format!("Downloaded: {} bytes in {:?}", size, elapsed), &output_str, size);
                    },
                    _ => {
                        eprintln!("[DarkDM] ffmpeg failed, trying yt-dlp");
                    }
                }
            }
            
            // Fallback: yt-dlp
            if which("yt-dlp") {
                eprintln!("[DarkDM] yt-dlp download from manifest");
                let ytdlp_path = find_binary("yt-dlp").unwrap_or_else(|| "yt-dlp".to_string());
                
                let mut ytdlp_cmd = Command::new(&ytdlp_path);
                ytdlp_cmd.args(["-o", &output_str, "--no-playlist", "--concurrent-fragments", "8",
                               "--impersonate", "chrome-131", &manifest_url]);
                
                if !cookies.is_empty() && std::fs::metadata(&cookies_path).map(|m| m.len() > 0).unwrap_or(false) {
                    ytdlp_cmd.args(["--cookies", &cookies_path.to_string_lossy()]);
                }
                
                let status = ytdlp_cmd
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .status();
                
                let _ = std::fs::remove_file(&cookies_path);
                
                match status {
                    Ok(s) if s.success() => {
                        let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                        return ok_response(&format!("Downloaded via yt-dlp: {} bytes", size), &output_str, size);
                    },
                    _ => {
                        return error_response("Both ffmpeg and yt-dlp failed. The manifest might be protected or segments are encrypted.");
                    }
                }
            }
            
            let _ = std::fs::remove_file(&cookies_path);
            error_response("No download tool available (need ffmpeg or yt-dlp)")
        },

        // ============================================================
        // SEGMENT DATA - Recibe un segmento .ts desde la extension
        // (interceptado via debugger Network.getResponseBody)
        // ============================================================
        "SEGMENT_DATA" => {
            let session = msg.extra.get("session").and_then(|v| v.as_str()).unwrap_or("unknown");
            let seq = msg.extra.get("seq").and_then(|v| v.as_i64()).unwrap_or(0);
            let data = msg.extra.get("data").and_then(|v| v.as_str()).unwrap_or("");
            
            eprintln!("[DarkDM] Segment: session={} seq={} len={}", session, seq, data.len());
            
            if data.is_empty() {
                return error_response("Empty segment data");
            }
            
            // Decode base64 and write to temp file
            let seg_dir = output_dir.join("_segments").join(session);
            std::fs::create_dir_all(&seg_dir).unwrap_or_default();
            let seg_path = seg_dir.join(format!("{:05}.ts", seq));
            
            use base64::Engine as _;
            let engine = base64::engine::general_purpose::STANDARD;
            if let Ok(bytes) = engine.decode(data) {
                if std::fs::write(&seg_path, &bytes).is_ok() {
                    eprintln!("[DarkDM] Saved segment {} -> {} ({} bytes)", seq, seg_path.display(), bytes.len());
                    Response {
                        msg_type: "SEGMENT_SAVED".to_string(),
                        success: Some(true), error: None,
                        message: Some(format!("Segment {} saved", seq)),
                        progress: Some(seq as f64),
                        filename: Some(seg_path.to_string_lossy().to_string()),
                        segments: None, bytes: Some(bytes.len() as u64),
                    }
                } else {
                    error_response(&format!("Failed to write segment {}", seq))
                }
            } else {
                error_response("Failed to decode base64 segment data")
            }
        },

        // ============================================================
        // CONCATENATE SEGMENTS - Une todos los segmentos .ts en .mp4
        // ============================================================
        "CONCATENATE_SEGMENTS" => {
            let session = msg.extra.get("session").and_then(|v| v.as_str()).unwrap_or("");
            let count = msg.extra.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            let title = msg.extra.get("title").and_then(|v| v.as_str()).unwrap_or("video");
            
            if session.is_empty() || count == 0 {
                return error_response("No session or count specified");
            }
            
            let seg_dir = output_dir.join("_segments").join(session);
            let output_path = output_dir.join(format!("{}.mp4", 
                title.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_' || *c == '-').collect::<String>().trim()));
            let output_str = output_path.to_string_lossy().to_string();
            
            eprintln!("[DarkDM] Concatenating {} segments -> {}", count, output_str);
            
            // Method 1: ffmpeg concat demuxer (fastest, no re-encode)
            if which("ffmpeg") {
                // Create concat file
                let concat_path = seg_dir.join("_concat.txt");
                let concat_content: String = (0..count)
                    .map(|i| format!("file '{:05}.ts'\n", i))
                    .collect();
                std::fs::write(&concat_path, &concat_content).unwrap_or_default();
                
                let status = Command::new("ffmpeg")
                    .args([
                        "-y", "-f", "concat", "-safe", "0",
                        "-i", &concat_path.to_string_lossy(),
                        "-c", "copy", "-movflags", "+faststart",
                        &output_str
                    ])
                    .stdout(Stdio::null()).stderr(Stdio::null())
                    .status();
                
                // Cleanup temp segments
                let _ = std::fs::remove_dir_all(&seg_dir);
                
                match status {
                    Ok(s) if s.success() => {
                        let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                        ok_response(&format!("Concatenated {} segments: {} bytes", count, size), &output_str, size)
                    },
                    _ => error_response("ffmpeg concat failed. Segments preserved in temp dir.")
                }
            } else {
                // Method 2: simple cat (works for MPEG-TS)
                eprintln!("[DarkDM] No ffmpeg, using cat for concatenation");
                let mut all_data: Vec<u8> = Vec::new();
                for i in 0..count {
                    let seg_path = seg_dir.join(format!("{:05}.ts", i));
                    if let Ok(data) = std::fs::read(&seg_path) {
                        all_data.extend_from_slice(&data);
                    }
                }
                if !all_data.is_empty() {
                    std::fs::write(&output_path, &all_data).unwrap_or_default();
                    let _ = std::fs::remove_dir_all(&seg_dir);
                    let size = all_data.len() as u64;
                    ok_response(&format!("Catted {} segments: {} bytes", count, size), &output_str, size)
                } else {
                    error_response("No segment data to concatenate")
                }
            }
        },

        // ============================================================
        // PING - Health check
        // ============================================================
        "PING" => Response {
            msg_type: "PONG".to_string(),
            success: Some(true),
            error: None,
            message: Some(format!("DarkDM Native Host v1.0.0 (HLS+DASH+Direct+SegmentCapture)")),
            progress: None, filename: None,
            segments: None, bytes: None,
        },

        // ============================================================
        // VIDEO INFO
        // ============================================================
        "VIDEO_DETECTED" => Response {
            msg_type: "VIDEO_RECEIVED".to_string(),
            success: Some(true),
            error: None,
            message: Some("Video detected, ready to capture".to_string()),
            progress: None,
            filename: msg.filename.clone(),
            segments: None, bytes: None,
        },

        _ => Response {
            msg_type: "UNKNOWN".to_string(),
            success: Some(false),
            error: Some(format!("Unknown type: {}", msg.msg_type)),
            message: None, progress: None, filename: None,
            segments: None, bytes: None,
        },
    }
}

fn launch_direct_download(url: &str, filename: &str, output_dir: &Path) -> Result<String, String> {
    let output_path = output_dir.join(filename);
    let output_str = output_path.to_string_lossy().to_string();

    // Try aria2c (multi-threaded)
    if which("aria2c") {
        let args = [
            "-x", "16", "-s", "16", "-k", "1M",
            "--continue",
            "--max-connection-per-server=16",
            "--dir", output_dir.to_str().unwrap_or("."),
            "--out", filename,
            "--user-agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
            url
        ];
        Command::new("aria2c")
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("aria2c failed: {}", e))?;
        return Ok(output_str);
    }

    // Try wget (resume support)
    if which("wget") {
        Command::new("wget")
            .args(["-c", "-O", &output_str, url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("wget failed: {}", e))?;
        return Ok(output_str);
    }

    Err("No download tool found. Install aria2c or wget.".to_string())
}

fn which(name: &str) -> bool {
    find_binary(name).is_some()
}

/// Find a binary, checking the DarkDM venv first
fn find_binary(name: &str) -> Option<String> {
    // Check DarkDM venv first (has curl_cffi for impersonation)
    let home = std::env::var("HOME").unwrap_or_default();
    let venv_path = format!("{}/.local/share/darkdm/venv/bin/{}", home, name);
    if Path::new(&venv_path).exists() {
        return Some(venv_path);
    }
    // Fall back to PATH
    std::env::var("PATH").ok().and_then(|path| {
        path.split(':')
            .find(|dir| Path::new(&format!("{}/{}", dir, name)).exists())
            .map(|dir| format!("{}/{}", dir, name))
    })
}

fn chrono_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}", d.as_secs())
}

fn ok_response(msg: &str, filename: &str, size: u64) -> Response {
    Response {
        msg_type: "DOWNLOAD_RESULT".to_string(),
        success: Some(true), error: None,
        message: Some(msg.to_string()),
        progress: Some(1.0),
        filename: Some(filename.to_string()),
        segments: None, bytes: Some(size),
    }
}

fn error_response(msg: &str) -> Response {
    Response {
        msg_type: "ERROR".to_string(),
        success: Some(false), error: Some(msg.to_string()),
        message: None, progress: None, filename: None,
        segments: None, bytes: None,
    }
}

/// Extract video from complex sites using yt-dlp (Netflix, Prime, etc.)
fn extract_with_ytdlp(url: &str, output_dir: &Path, has_drm: bool, site: &str, cookies: &str) -> Result<String, String> {
    // Find yt-dlp (prefer venv which has curl_cffi for impersonation)
    let ytdlp_path = find_binary("yt-dlp").unwrap_or_else(|| "yt-dlp".to_string());
    if !Path::new(&ytdlp_path).exists() {
        return Err("yt-dlp not found. Install it: https://github.com/yt-dlp/yt-dlp".to_string());
    }

    let filename = format!("darkdm_ytdlp_%(title)s_%(id)s.%(ext)s");
    let output_template = output_dir.join(&filename);
    let template_str = output_template.to_string_lossy().to_string();

    eprintln!("[DarkDM] Running yt-dlp at {} for: {} (drm={}, site={})", ytdlp_path, url, has_drm, site);

    // Write cookies from extension to a temp file (extension cookies > browser cookies)
    let cookies_file = if !cookies.is_empty() {
        let path = output_dir.join("_darkdm_cookies.txt");
        if std::fs::write(&path, cookies).is_ok() {
            eprintln!("[DarkDM] Wrote {} cookies to temp file", cookies.lines().count());
            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    // Build base args
    let mut args: Vec<String> = vec![
        "-o".to_string(), template_str.clone(),
        "--no-playlist".to_string(),
        "--limit-rate".to_string(), "50M".to_string(),
        "--concurrent-fragments".to_string(), "8".to_string(),
    ];

    // Use cookies if available (from extension API — already decrypted)
    if let Some(ref cpath) = cookies_file {
        let cpath_str = cpath.to_string_lossy().to_string();
        args.push("--cookies".to_string());
        args.push(cpath_str);
    }

    // DRM sites: impersonation + best format
    if has_drm || !site.is_empty() {
        args.extend_from_slice(&[
            "-f".to_string(), "bestvideo+bestaudio/best".to_string(),
            "--impersonate".to_string(), "chrome-131".to_string(),
        ]);
    } else {
        // Non-DRM sites: basic impersonation
        args.push("--impersonate".to_string());
        args.push("chrome-131".to_string());
    }

    args.push("--verbose".to_string());
    args.push(url.to_string());

    eprintln!("[DarkDM] yt-dlp args: {:?}", args);

    let output = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

    if output.status.success() {
        // Find the actual output file
        let stderr = String::from_utf8_lossy(&output.stderr);
        let actual_file = find_ytdlp_output(&stderr, output_dir);
        eprintln!("[DarkDM] yt-dlp OK: {:?}", actual_file);
        Ok(actual_file.unwrap_or_else(|| {
            output_dir.join("darkdm_video.mp4").to_string_lossy().to_string()
        }))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[DarkDM] yt-dlp failed (1st attempt with cookies): {}", stderr);

        // Retry without cookies (for public sites, still use venv yt-dlp + impersonation)
        let mut retry_args: Vec<String> = vec![
            "-o".to_string(), template_str.clone(),
            "--no-playlist".to_string(),
            "--concurrent-fragments".to_string(), "8".to_string(),
            "--impersonate".to_string(), "chrome-131".to_string(),
        ];
        if has_drm {
            retry_args.extend_from_slice(&["-f".to_string(), "bestvideo+bestaudio/best".to_string()]);
        }
        retry_args.push(url.to_string());

        let output2 = Command::new(&ytdlp_path)
            .args(&retry_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("yt-dlp retry failed: {}", e))?;

        if output2.status.success() {
            let stderr2 = String::from_utf8_lossy(&output2.stderr);
            let actual_file = find_ytdlp_output(&stderr2, output_dir);
            eprintln!("[DarkDM] yt-dlp OK (no cookies): {:?}", actual_file);
            Ok(actual_file.unwrap_or_else(|| {
                output_dir.join("darkdm_video.mp4").to_string_lossy().to_string()
            }))
        } else {
            let err2 = String::from_utf8_lossy(&output2.stderr);
            eprintln!("[DarkDM] yt-dlp failed both attempts: {}", err2);
            Err(format!("yt-dlp extraction failed.\nExport cookies manually and run:\nyt-dlp --cookies cookies.txt --impersonate chrome-131 \"{}\"\nError: {}", 
                       url, extract_error(&err2)))
        }
    }
}

/// Try to extract the actual filename from yt-dlp verbose output
fn find_ytdlp_output(stderr: &str, output_dir: &Path) -> Option<String> {
    // Look for "[Merger] Merging formats into ..." or "[download] Destination: ..."
    for line in stderr.lines() {
        if line.contains("Destination:") {
            if let Some(path) = line.split("Destination:").nth(1) {
                let p = path.trim();
                if !p.is_empty() && Path::new(p).exists() {
                    return Some(p.to_string());
                }
            }
        }
        if line.contains("has already been downloaded") {
            if let Some(path) = line.split(' ').next() {
                let p = path.trim();
                if !p.is_empty() && Path::new(p).exists() {
                    return Some(p.to_string());
                }
            }
        }
    }
    None
}

/// Extract a useful error message from yt-dlp output
fn extract_error(err: &str) -> String {
    let mut lines: Vec<&str> = err.lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && t.len() > 10 && !t.starts_with('[')
        })
        .collect();
    // Take last 3 meaningful lines
    let count = lines.len();
    let relevant: Vec<&str> = if count > 3 {
        lines.split_off(count - 3)
    } else {
        lines
    };
    if relevant.is_empty() {
        "See verbose output above".to_string()
    } else {
        relevant.join(" | ")
    }
}

fn get_browser_name() -> &'static str {
    // Detect which browser to pull cookies from
    let home = std::env::var("HOME").unwrap_or_default();
    if Path::new(&format!("{}/.config/vivaldi", home)).exists() { "vivaldi" }
    else if Path::new(&format!("{}/.config/chromium", home)).exists() { "chromium" }
    else if Path::new(&format!("{}/.config/google-chrome", home)).exists() { "chrome" }
    else if Path::new(&format!("{}/.config/brave-browser", home)).exists() { "brave" }
    else { "chromium" }
}
