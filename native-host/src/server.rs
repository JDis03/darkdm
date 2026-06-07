use std::env;
use std::io::Cursor;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::log;

const DEFAULT_PORT: u16 = 8765;

pub fn start_server() {
    let port = env::var("DARKDM_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("127.0.0.1:{}", port);
    let server = match Server::http(&addr) {
        Ok(s) => {
            log::log(&format!("HTTP server listening on {}", addr));
            eprintln!("[DarkDM] HTTP server listening on {}", addr);
            Arc::new(s)
        }
        Err(e) => {
            log::log(&format!("Failed to bind {}: {}", addr, e));
            eprintln!("[DarkDM] Failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();

        log::log(&format!("{} {}", method, url));

        match (method, url.as_str()) {
            (Method::Options, _) => handle_options(request),
            (Method::Get, "/health") => handle_health(request),
            (Method::Post, "/download") => handle_download(request),
            _ => {
                let body = r#"{"error":"Not found"}"#;
                let response = Response::from_string(body)
                    .with_status_code(404)
                    .with_header(json_header())
                    .with_header(cors_header());
                let _ = request.respond(response);
            }
        }
    }
}

fn handle_options(request: Request) {
    let response = Response::empty(204)
        .with_header(cors_header())
        .with_header(cors_methods_header())
        .with_header(cors_headers_header());
    let _ = request.respond(response);
}

fn handle_health(request: Request) {
    let body = r#"{"status":"ok","version":"1.0.0"}"#;
    let response = Response::from_string(body)
        .with_status_code(200)
        .with_header(json_header())
        .with_header(cors_header());
    let _ = request.respond(response);
}

fn handle_download(mut request: Request) {
    // Read request body
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        let err = r#"{"success":false,"error":"Failed to read request body"}"#;
        let response = Response::from_string(err)
            .with_status_code(400)
            .with_header(json_header())
            .with_header(cors_header());
        let _ = request.respond(response);
        return;
    }

    // Parse JSON
    let data: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            let err = r#"{"success":false,"error":"Invalid JSON"}"#;
            let response = Response::from_string(err)
                .with_status_code(400)
                .with_header(json_header())
                .with_header(cors_header());
            let _ = request.respond(response);
            return;
        }
    };

    // Validate required fields
    let manifest_url = match data.get("manifest_url").and_then(|v| v.as_str()) {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => {
            let err = r#"{"success":false,"error":"manifest_url is required"}"#;
            let response = Response::from_string(err)
                .with_status_code(400)
                .with_header(json_header())
                .with_header(cors_header());
            let _ = request.respond(response);
            return;
        }
    };

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("video");
    let page_url = data
        .get("page_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract headers
    let headers = data.get("headers").and_then(|v| v.as_object());
    let user_agent = headers
        .and_then(|h| h.get("user-agent").or(h.get("User-Agent")))
        .and_then(|v| v.as_str())
        .unwrap_or("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");
    let referer = headers
        .and_then(|h| h.get("referer").or(h.get("Referer")))
        .and_then(|v| v.as_str())
        .unwrap_or(page_url);

    // Build output path
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let output_dir = std::path::Path::new(&home).join("Descargas/DarkDM");
    std::fs::create_dir_all(&output_dir).unwrap_or_default();

    let safe_title: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect();
    let filename = if safe_title.is_empty() {
        "darkdm_video".to_string()
    } else {
        safe_title
    };
    let output_path = output_dir.join(format!("{}.mp4", filename));
    let output_str = output_path.to_string_lossy().to_string();

    log::log(&format!(
        "Download: {} -> {} (referer: {})",
        manifest_url, output_str, referer
    ));

    // Use manifest_body from extension if provided (browser-fetched, has session cookies)
    // Otherwise fall back to downloading fresh with curl
    let manifest_body_from_ext = data
        .get("manifest_body")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Spawn ffmpeg in background
    let manifest_url_clone = manifest_url.clone();
    let user_agent_clone = user_agent.to_string();
    let referer_clone = referer.to_string();
    let output_str_clone = output_str.clone();
    let output_dir_clone = output_dir.clone();

    thread::spawn(move || {
        // Save manifest body from extension (or download fresh) to temp file
        let manifest_path = output_dir_clone.join("manifest_clean.m3u8");
        let manifest_content = if let Some(body) = manifest_body_from_ext {
            log::log(&format!("Using browser manifest ({} bytes)", body.len()));
            body
        } else {
            // Download with curl
            let out = Command::new("curl")
                .args(["-s", "-L", "-A", &user_agent_clone,
                       "-H", &format!("Referer: {}", referer_clone),
                       &manifest_url_clone])
                .output();
            match out {
                Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
                Err(e) => { log::log(&format!("curl failed: {}", e)); return; }
            }
        };

        if std::fs::write(&manifest_path, &manifest_content).is_err() {
            log::log("Failed to write manifest");
            return;
        }

        // Try ffmpeg first — same approach that works for Jack Ryan
        log::log(&format!("Trying ffmpeg for: {}", manifest_url_clone));
        let ffmpeg_ok = {
            let mut cmd = Command::new("ffmpeg");
            cmd.args(["-y", "-hide_banner", "-loglevel", "error",
                      "-user_agent", &user_agent_clone]);
            if !referer_clone.is_empty() {
                cmd.args(["-referer", &referer_clone]);
            }
            cmd.args(["-i", &manifest_url_clone, "-c", "copy",
                      "-movflags", "+faststart", &output_str_clone]);
            matches!(cmd.status(), Ok(s) if s.success())
        };

        if ffmpeg_ok {
            let size = std::fs::metadata(&output_str_clone).map(|m| m.len()).unwrap_or(0);
            log::log(&format!("ffmpeg OK: {} bytes", size));
        } else {
            // ffmpeg failed (e.g. TikTok CDN segments without .ts extension)
            // Download each segment with curl and concatenate — no remux, no corruption
            log::log("ffmpeg failed, downloading segments manually...");
            download_segments_and_concat(
                &manifest_content,
                &manifest_url_clone,
                &user_agent_clone,
                &referer_clone,
                &output_str_clone,
                &output_dir_clone,
            );
        }

        // Cleanup temp manifest
        let _ = std::fs::remove_file(&manifest_path);
    });

    // Respond immediately
    let resp_body = serde_json::json!({
        "success": true,
        "message": "Download started",
        "output_path": output_str
    });
    let response = Response::from_string(resp_body.to_string())
        .with_status_code(200)
        .with_header(json_header())
        .with_header(cors_header());
    let _ = request.respond(response);
}

fn json_header() -> Header {
    Header::from_bytes("Content-Type", "application/json").unwrap()
}

fn cors_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap()
}

fn cors_methods_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Methods", "POST, GET, OPTIONS").unwrap()
}

fn cors_headers_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap()
}

/// Download each HLS segment and concatenate with ffmpeg concat demuxer (not raw cat)
fn download_segments_and_concat(
    manifest_content: &str,
    base_url: &str,
    user_agent: &str,
    referer: &str,
    output_path: &str,
    work_dir: &std::path::Path,
) {
    // Parse segment URLs from manifest
    let base = if let Some(pos) = base_url.rfind('/') { &base_url[..=pos] } else { base_url };
    let segments: Vec<String> = manifest_content.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            if l.starts_with("http") { l.to_string() }
            else { format!("{}{}", base, l) }
        })
        .collect();

    if segments.is_empty() {
        log::log("No segments found in manifest");
        return;
    }

    // Download first segment to verify it's actual video (not a PNG placeholder)
    let seg_dir = work_dir.join("_segments");
    let _ = std::fs::create_dir_all(&seg_dir);
    
    let first_url = &segments[0];
    let test_file = seg_dir.join("_test");
    let test_status = Command::new("curl")
        .args(["-s", "-L", "-o", test_file.to_str().unwrap_or(""),
               "-A", user_agent, "-H", &format!("Referer: {}", referer),
               "-w", "%{content_type}", first_url])
        .output();

    if let Ok(o) = test_status {
        let ct = String::from_utf8_lossy(&o.stdout);
        if ct.contains("image/png") || ct.contains("image/jpeg") || ct.contains("image/gif") {
            log::log(&format!("ERROR: Segments are images ({}), not video. Site uses proprietary format (TikTok ImageX). Cannot download.", ct));
            let _ = std::fs::remove_dir_all(&seg_dir);
            return;
        }
    }
    let _ = std::fs::remove_file(&test_file);

    log::log(&format!("Downloading {} segments...", segments.len()));

    // Download all segments
    let concat_file = seg_dir.join("_concat.txt");
    let mut concat_list = String::new();
    let total = segments.len();
    
    for (i, seg_url) in segments.iter().enumerate() {
        let seg_name = format!("{:05}.ts", i);
        let seg_path = seg_dir.join(&seg_name);
        let status = Command::new("curl")
            .args(["-s", "-L", "-o", seg_path.to_str().unwrap_or(""),
                   "-A", user_agent, "-H", &format!("Referer: {}", referer), seg_url])
            .status();
        
        if let Ok(s) = status {
            if s.success() {
                concat_list.push_str(&format!("file '{}'\n", seg_name));
            } else {
                log::log(&format!("Segment {} failed", i));
            }
        }
        
        if i % 100 == 0 {
            log::log(&format!("Progress: {}/{}", i, total));
        }
    }
    
    std::fs::write(&concat_file, &concat_list).unwrap_or_default();
    
    // Concatenate with ffmpeg concat demuxer (handles PTS, headers, codecs correctly)
    log::log("Concatenating with ffmpeg concat demuxer...");
    let status = Command::new("ffmpeg")
        .args(["-y", "-f", "concat", "-safe", "0",
               "-i", concat_file.to_str().unwrap_or(""),
               "-c", "copy", output_path])
        .status();
    
    let _ = std::fs::remove_dir_all(&seg_dir);
    
    match status {
        Ok(s) if s.success() => {
            let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
            log::log(&format!("Concat OK: {} bytes -> {}", size, output_path));
        }
        _ => log::log("Concat failed"),
    }
}

/// Use browser-provided manifest body or download fresh, then clean it
fn prepare_manifest(
    body_from_ext: Option<String>,
    url: &str,
    user_agent: &str,
    referer: &str,
    output_dir: &std::path::Path,
) -> Result<String, String> {
    let content = if let Some(body) = body_from_ext {
        log::log(&format!("Using browser-provided manifest ({} bytes)", body.len()));
        body
    } else {
        log::log("Downloading manifest with curl...");
        download_and_clean_manifest(url, user_agent, referer, output_dir)?;
        return Ok(output_dir.join("manifest_clean.m3u8").to_string_lossy().to_string());
    };
    clean_manifest_content(content, url, output_dir)
}

/// Download manifest and filter out ad/tracking URLs
fn download_and_clean_manifest(
    url: &str,
    user_agent: &str,
    referer: &str,
    output_dir: &std::path::Path,
) -> Result<String, String> {
    use std::io::Read;

    // Download manifest using curl
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-L", "-A", user_agent, "-H", &format!("Referer: {}", referer), url]);

    let output = cmd.output().map_err(|e| format!("curl failed: {}", e))?;
    if !output.status.success() {
        return Err(format!("curl exited with status: {}", output.status));
    }

    let manifest_content =
        String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    clean_manifest_content(manifest_content, url, output_dir)
}

/// Clean manifest content: filter ads, resolve relative URLs, save to temp file
fn clean_manifest_content(
    manifest_content: String,
    url: &str,
    output_dir: &std::path::Path,
) -> Result<String, String> {

    // Save original manifest for debugging
    let original_manifest_path = output_dir.join("manifest_original.m3u8");
    let _ = std::fs::write(&original_manifest_path, &manifest_content);

    // Ad/tracking domains to filter out (only filter complete URLs, not partial matches)
    let ad_domains = [
        "tiktokcdn.com",
        "doubleclick.net",
        "googlesyndication.com",
        "googleadservices.com",
        "facebook.com",
        "fbcdn.net",
        "adsrvr.org",
        "adnxs.com",
    ];

    // Extract base URL for resolving relative paths
    let base_url = if let Some(pos) = url.rfind('/') {
        &url[..=pos]
    } else {
        url
    };

    let mut filtered_count = 0;
    let mut filtered_examples: Vec<String> = Vec::new();

    // Filter manifest: when a segment URL is an ad, also remove its preceding #EXTINF tag
    let lines: Vec<&str> = manifest_content.lines().collect();
    let mut cleaned_lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let is_segment_url = !line.starts_with('#') && !line.trim().is_empty();

        if is_segment_url {
            let line_lower = line.to_lowercase();
            let is_ad = ad_domains.iter().any(|domain| line_lower.contains(domain));
            if is_ad {
                // Remove this URL and its preceding #EXTINF (if any)
                if let Some(last) = cleaned_lines.last() {
                    if last.starts_with("#EXTINF") {
                        cleaned_lines.pop();
                    }
                }
                filtered_count += 1;
                if filtered_examples.len() < 3 {
                    filtered_examples.push(line.to_string());
                }
            } else {
                // Resolve relative URL
                if !line.starts_with("http") {
                    cleaned_lines.push(format!("{}{}", base_url, line));
                } else {
                    cleaned_lines.push(line.to_string());
                }
            }
        } else {
            cleaned_lines.push(line.to_string());
        }
        i += 1;
    }

    let cleaned_manifest = cleaned_lines.join("\n");

    // Save to temporary file
    let manifest_path = output_dir.join("manifest_clean.m3u8");
    std::fs::write(&manifest_path, &cleaned_manifest)
        .map_err(|e| format!("Failed to write manifest: {}", e))?;

    // Count real segment lines (non-tag, non-empty)
    let real_segments = cleaned_lines.iter()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .count();

    log::log(&format!(
        "Manifest: {} lines, filtered {} ads, {} real segments remain",
        manifest_content.lines().count(), filtered_count, real_segments
    ));

    if !filtered_examples.is_empty() {
        log::log(&format!("Filtered URL examples: {:?}", filtered_examples));
    }

    // If no real segments remain after filtering, the ad is still playing
    if real_segments == 0 {
        return Err("Ad is still playing — wait for the real video to start, then click Download again.".to_string());
    }

    Ok(manifest_path.to_string_lossy().to_string())
}
